//! Generic deterministic paired-match execution.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use awbrn_ai::FNV1A_OFFSET_BASIS;
use awbrn_ai::agent::{Agent, NodeBudget};
use awbrn_ai::agents::StrategicAgent;
use awbrn_ai::baseline::BaselineConfig;
use awbrn_ai::harness::{Limits, next_command_fingerprint, play_observed_fallible};
use awbrn_ai::rng::Rng;
use awbrn_ai_diagnostic_types::{
    AgentIdentity, PairKey, Reduction, RunLimits, RunManifest, RunManifestError, SeatOrderVariant,
};
use awvm::session::Session;
use awvm::transition::Command;
use serde::Serialize;

use crate::events::{
    EventLogError, EventLogWriter, EventMetadata, observations_from_event_log, read_event_log,
    row_for_state, verify_expected_fingerprints, write_derived_outputs, write_event_tables,
};
use crate::manifest::{
    ManifestError, read_manifest, resolve_event_log_path, write_or_validate_manifest,
};
use crate::map_registry::{MapRegistry, RegisteredMap};

/// A factory for one named agent configuration.
pub trait AgentFactory: Send + Sync {
    /// Return the stable identity recorded in the run manifest.
    fn identity(&self) -> &AgentIdentity;

    /// Build a fresh agent for one match stream.
    fn create(&self, seed: u64) -> Box<dyn Agent>;
}

/// A factory for the current strategic agent.
#[derive(Clone, Debug)]
pub struct StrategicFactory {
    identity: AgentIdentity,
    config: BaselineConfig,
}

/// The executable identity for the strategic agent implementation.
pub const STRATEGIC_EXECUTABLE_FINGERPRINT: &str = "awbrn-ai-strategic-v1";

impl StrategicFactory {
    /// Create a factory from the configuration it will run.
    pub fn new(config: BaselineConfig) -> Self {
        Self {
            identity: AgentIdentity {
                identifier: config.identifier.to_owned(),
                configuration_fingerprint: config.fingerprint(),
                executable_fingerprint: STRATEGIC_EXECUTABLE_FINGERPRINT.into(),
            },
            config,
        }
    }
}

impl AgentFactory for StrategicFactory {
    fn identity(&self) -> &AgentIdentity {
        &self.identity
    }

    fn create(&self, seed: u64) -> Box<dyn Agent> {
        Box::new(StrategicAgent::with_config(seed, self.config))
    }
}

/// Summary of one persisted paired tournament.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TournamentSummary {
    pub output: PathBuf,
    pub matches: usize,
    pub valid_matches: usize,
    pub reduction: Reduction,
}

/// Errors from map validation, match execution, or diagnostic persistence.
#[derive(Debug, thiserror::Error)]
pub enum TournamentError {
    #[error("manifest error: {0}")]
    Manifest(#[from] ManifestError),
    #[error("manifest schema error: {0}")]
    Schema(#[from] RunManifestError),
    #[error("map registry error: {0}")]
    Map(#[from] crate::map_registry::MapRegistryError),
    #[error("event log error: {0}")]
    Event(#[from] EventLogError),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("configuration error: {0}")]
    Configuration(String),
}

/// Run all expected map and pair identities in both seat orders.
pub fn run_paired_tournament(
    manifest: &RunManifest,
    registry: &MapRegistry,
    candidate: &dyn AgentFactory,
    baseline: &dyn AgentFactory,
    output: impl AsRef<Path>,
) -> Result<TournamentSummary, TournamentError> {
    manifest.validate().map_err(RunManifestError::Invalid)?;
    validate_agents(manifest, candidate, baseline)?;
    validate_maps(manifest, registry)?;
    if manifest.pairs.is_empty() {
        return Err(TournamentError::Configuration(
            "run manifest has no expected pairs".into(),
        ));
    }

    let output = output.as_ref().to_owned();
    fs::create_dir_all(&output)?;
    write_or_validate_manifest(manifest, output.join("manifest.json"))?;
    let event_path = resolve_event_log_path(&output, manifest)?;
    let mut event_log = EventLogWriter::open(&event_path)?;

    for pair in manifest.expected_pairs() {
        let map = registry.get(pair.map_id).ok_or_else(|| {
            TournamentError::Configuration(format!("map {} is not loaded", pair.map_id))
        })?;
        for seat_order in SeatOrderVariant::ALL {
            let match_id = match_id(&pair, seat_order);
            if event_log.has_terminal_match(&match_id)? {
                continue;
            }
            let attempt = event_log.begin_attempt(&match_id)?;
            run_match(
                manifest,
                map,
                MatchSelection {
                    pair: &pair,
                    seat_order,
                    attempt,
                },
                candidate,
                baseline,
                &mut event_log,
            )?;
        }
    }
    event_log.flush()?;

    // The event log is the resume source. Derived match rows can be rebuilt.
    let events = read_event_log(event_log.path())?;
    let observations = observations_from_event_log(&events, manifest);
    let reduction = write_derived_outputs(&output, &observations, manifest)?;
    write_event_tables(&output, &events)?;
    verify_expected_fingerprints(manifest, event_log.path(), &output)?;
    fs::write(
        output.join("manifest-fingerprint.txt"),
        format!(
            "{}\n",
            manifest
                .fingerprint()
                .map_err(TournamentError::Configuration)?
        ),
    )?;
    Ok(TournamentSummary {
        output,
        matches: observations.len(),
        valid_matches: observations
            .iter()
            .filter(|observation| observation.valid)
            .count(),
        reduction,
    })
}

/// Load a manifest and its fixed map suite, then run the paired tournament.
pub fn run_manifest(
    manifest_path: impl AsRef<Path>,
    output: impl AsRef<Path>,
    candidate: &dyn AgentFactory,
    baseline: &dyn AgentFactory,
) -> Result<TournamentSummary, TournamentError> {
    let manifest = read_manifest(manifest_path)?;
    let registry = crate::map_registry::MapRegistry::load_checked_in()?;
    run_paired_tournament(&manifest, &registry, candidate, baseline, output)
}

fn validate_agents(
    manifest: &RunManifest,
    candidate: &dyn AgentFactory,
    baseline: &dyn AgentFactory,
) -> Result<(), TournamentError> {
    let expected = [&manifest.agents[0], &manifest.agents[1]];
    let actual = [candidate.identity(), baseline.identity()];
    if expected != actual {
        return Err(TournamentError::Configuration(
            "agent identities do not match the run manifest".into(),
        ));
    }
    Ok(())
}

fn validate_maps(manifest: &RunManifest, registry: &MapRegistry) -> Result<(), TournamentError> {
    let identities = registry
        .iter()
        .map(|map| (map.id, map))
        .collect::<BTreeMap<_, _>>();
    for expected in &manifest.maps {
        let Some(map) = identities.get(&expected.map_id) else {
            return Err(TournamentError::Configuration(format!(
                "manifest map {} is not in the fixed registry",
                expected.map_id
            )));
        };
        if expected.source_fingerprint != map.source_fingerprint
            || expected.normalized_fingerprint != map.normalized_fingerprint
        {
            return Err(TournamentError::Configuration(format!(
                "manifest fingerprints differ for map {}",
                expected.map_id
            )));
        }
    }
    Ok(())
}

struct MatchSelection<'a> {
    pair: &'a PairKey,
    seat_order: SeatOrderVariant,
    attempt: u32,
}

fn run_match(
    manifest: &RunManifest,
    map: &RegisteredMap,
    selection: MatchSelection<'_>,
    candidate: &dyn AgentFactory,
    baseline: &dyn AgentFactory,
    event_log: &mut EventLogWriter,
) -> Result<(), TournamentError> {
    let pair = selection.pair;
    let seat_order = selection.seat_order;
    let attempt = selection.attempt;
    let match_seed = match_seed(pair);
    let metadata = EventMetadata {
        match_id: match_id(pair, seat_order),
        attempt,
        pair: pair.clone(),
        match_seed,
        seat_order,
        map_fingerprint: map.normalized_fingerprint.clone(),
        configuration_fingerprint: manifest.configuration_fingerprint.clone(),
    };
    let state = map.state(match_seed)?;
    let mut session = Session::new(state.clone());
    let mut entropy = Rng::from_seed(BaselineConfig::LOCKED.entropy_seed(match_seed));
    let candidate_seed = BaselineConfig::LOCKED.agent_seed(match_seed, 0);
    let baseline_seed = BaselineConfig::LOCKED.agent_seed(match_seed, 1);
    let mut candidate = candidate.create(candidate_seed);
    let mut baseline = baseline.create(baseline_seed);
    let mut agents: [&mut dyn Agent; 2] = match seat_order {
        SeatOrderVariant::AgentFirst => [&mut *candidate, &mut *baseline],
        SeatOrderVariant::BaselineFirst => [&mut *baseline, &mut *candidate],
    };
    let limits = limits(&manifest.limits)?;
    let mut sequence = 0_u64;
    let mut turn_index = 0_u32;
    let mut command_index = 0_u32;
    let mut command_fingerprint = FNV1A_OFFSET_BASIS;
    let result: Result<_, EventLogError> = play_observed_fallible(
        state,
        &mut session,
        &mut agents,
        &mut entropy,
        limits,
        |state, command| {
            let command = command.cloned();
            if let Some(command) = command.as_ref() {
                command_fingerprint = next_command_fingerprint(command_fingerprint, command);
            }
            let row = row_for_state(
                &metadata,
                sequence,
                state,
                command.clone(),
                command_fingerprint,
                turn_index,
                command_index,
            );
            event_log.append(row)?;
            sequence += 1;
            if command.is_some() {
                if matches!(command, Some(Command::EndTurn { .. })) {
                    turn_index += 1;
                    command_index = 0;
                } else {
                    command_index += 1;
                }
            }
            Ok(())
        },
    );
    // A telemetry failure makes the event log incomplete, and the event log is
    // the authority for every derived output. Stop the run instead of turning
    // an I/O failure into a match-level result the caller can discard.
    result?;
    Ok(())
}

fn limits(limits: &RunLimits) -> Result<Limits, TournamentError> {
    let nodes = NodeBudget::new(limits.node_budget)
        .ok_or_else(|| TournamentError::Configuration("node budget must be nonzero".into()))?;
    Ok(Limits {
        nodes,
        days: limits.day_limit,
        refusals: limits.refusal_limit,
    })
}

fn match_seed(pair: &PairKey) -> u64 {
    Rng::mix(pair.run_seed ^ (u64::from(pair.map_id) << 32) ^ pair.pair_index)
}

fn match_id(pair: &PairKey, seat_order: SeatOrderVariant) -> String {
    format!(
        "map-{}-seed-{}-pair-{}-{}",
        pair.map_id,
        pair.run_seed,
        pair.pair_index,
        seat_order.as_str()
    )
}
