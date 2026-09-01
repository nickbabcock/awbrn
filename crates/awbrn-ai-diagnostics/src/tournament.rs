//! Generic deterministic paired-match execution.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use awbrn_ai::EvalWeights;
use awbrn_ai::FNV1A_OFFSET_BASIS;
use awbrn_ai::agent::{Agent, NodeBudget};
use awbrn_ai::agents::{SearchAgent, StrategicAgent, Weights};
use awbrn_ai::baseline::BaselineConfig;
use awbrn_ai::harness::{Limits, next_command_fingerprint, play_observed_fallible};
use awbrn_ai::rng::Rng;
use awbrn_ai_diagnostic_types::{
    AgentIdentity, PairKey, Reduction, RunLimits, RunManifest, RunManifestError, SeatOrderVariant,
    fingerprint_bytes,
};
use awvm::semantic::Outcome;
use awvm::session::Session;
use awvm::transition::Command;
use serde::{Deserialize, Serialize};

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

/// A factory for a versioned reply-search candidate.
#[derive(Clone, Debug)]
pub struct SearchFactory {
    identity: AgentIdentity,
    weights: Weights,
    eval_weights: EvalWeights,
    node_budget: NodeBudget,
}

/// The executable identity for the reply-search implementation.
pub const SEARCH_EXECUTABLE_FINGERPRINT: &str = "awbrn-ai-search-v1";

impl SearchFactory {
    /// Create a factory from the complete search configuration it will run.
    pub fn new(
        identifier: &str,
        weights: Weights,
        eval_weights: EvalWeights,
        node_budget: NodeBudget,
    ) -> Self {
        let bytes = serde_json::to_vec(&(identifier, weights, eval_weights, node_budget))
            .expect("search configuration serializes");
        let fingerprint = fingerprint_bytes(&bytes);
        Self {
            identity: AgentIdentity {
                identifier: identifier.to_owned(),
                configuration_fingerprint: fingerprint,
                executable_fingerprint: SEARCH_EXECUTABLE_FINGERPRINT.into(),
            },
            weights,
            eval_weights,
            node_budget,
        }
    }
}

impl AgentFactory for SearchFactory {
    fn identity(&self) -> &AgentIdentity {
        &self.identity
    }

    fn create(&self, seed: u64) -> Box<dyn Agent> {
        Box::new(
            SearchAgent::from_seed(seed)
                .with_weights(self.weights)
                .with_evaluator_weights(self.eval_weights)
                .with_node_budget(self.node_budget),
        )
    }
}

/// Summary of one persisted paired tournament.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TournamentSummary {
    pub output: PathBuf,
    pub matches: usize,
    pub valid_matches: usize,
    pub reduction: Reduction,
    pub performance: TournamentPerformance,
}

/// Runtime and invalid-command measurements from completed match attempts.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TournamentPerformance {
    pub matches: usize,
    pub wall_clock_nanos: u64,
    pub total_match_nanos: u64,
    pub mean_match_nanos: u64,
    pub total_commands: u64,
    pub total_invalid_commands: u64,
    #[serde(default)]
    pub total_refusals: u64,
    #[serde(default)]
    pub total_unrealizable_plays: u64,
    pub matches_by_seat_order: BTreeMap<String, usize>,
    pub match_records: Vec<MatchPerformance>,
}

/// Runtime and command measurements from one newly executed match.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MatchPerformance {
    pub match_id: String,
    #[serde(default)]
    pub attempt: u32,
    pub map_id: u32,
    pub seat_order: SeatOrderVariant,
    pub elapsed_nanos: u64,
    pub turns: u32,
    pub days: u32,
    pub commands: u64,
    pub invalid_commands: u64,
    #[serde(default)]
    pub refusals: u64,
    #[serde(default)]
    pub unrealizable_plays: u64,
    pub outcome: String,
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
    let tournament_started = Instant::now();
    fs::create_dir_all(&output)?;
    write_or_validate_manifest(manifest, output.join("manifest.json"))?;
    let prior_performance = read_performance(&output.join("performance.json"))?;
    let prior_wall_clock_nanos = prior_performance
        .as_ref()
        .map_or(0, |performance| performance.wall_clock_nanos);
    let event_path = resolve_event_log_path(&output, manifest)?;
    let mut event_log = EventLogWriter::open(&event_path)?;
    let mut match_performance = Vec::new();

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
            match_performance.push(run_match(
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
            )?);
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
    let current_wall_clock_nanos = tournament_started
        .elapsed()
        .as_nanos()
        .try_into()
        .unwrap_or(u64::MAX);
    let performance = TournamentPerformance::from_matches(
        merge_performance(prior_performance, match_performance),
        prior_wall_clock_nanos.saturating_add(current_wall_clock_nanos),
    );
    fs::write(
        output.join("performance.json"),
        serde_json::to_vec_pretty(&performance)?,
    )?;
    Ok(TournamentSummary {
        output,
        matches: observations.len(),
        valid_matches: observations
            .iter()
            .filter(|observation| observation.valid)
            .count(),
        reduction,
        performance,
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
) -> Result<MatchPerformance, TournamentError> {
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
    let match_started = Instant::now();
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
    let record = result?;
    Ok(MatchPerformance {
        match_id: metadata.match_id,
        attempt,
        map_id: pair.map_id,
        seat_order,
        elapsed_nanos: match_started
            .elapsed()
            .as_nanos()
            .try_into()
            .unwrap_or(u64::MAX),
        turns: record.turns,
        days: record.days,
        commands: record.commands,
        invalid_commands: record.refusals.saturating_add(record.unrealizable_plays),
        refusals: record.refusals,
        unrealizable_plays: record.unrealizable_plays,
        outcome: outcome_name(record.outcome.as_ref()).into(),
    })
}

impl TournamentPerformance {
    fn from_matches(matches: Vec<MatchPerformance>, wall_clock_nanos: u64) -> Self {
        let mut latest = BTreeMap::<&str, &MatchPerformance>::new();
        for record in &matches {
            let replace = latest
                .get(record.match_id.as_str())
                .is_none_or(|existing| record.attempt > existing.attempt);
            if replace {
                latest.insert(record.match_id.as_str(), record);
            }
        }
        let active = latest.values().copied().collect::<Vec<_>>();
        let total_match_nanos = active
            .iter()
            .map(|record| record.elapsed_nanos)
            .sum::<u64>();
        let total_commands = active.iter().map(|record| record.commands).sum();
        let total_invalid_commands = active.iter().map(|record| record.invalid_commands).sum();
        let total_refusals = active.iter().map(|record| record.refusals).sum();
        let total_unrealizable_plays = active.iter().map(|record| record.unrealizable_plays).sum();
        let mut matches_by_seat_order = BTreeMap::new();
        for record in &active {
            *matches_by_seat_order
                .entry(record.seat_order.as_str().to_owned())
                .or_insert(0) += 1;
        }
        let mean_match_nanos = if active.is_empty() {
            0
        } else {
            total_match_nanos / active.len() as u64
        };
        Self {
            matches: active.len(),
            wall_clock_nanos,
            total_match_nanos,
            mean_match_nanos,
            total_commands,
            total_invalid_commands,
            total_refusals,
            total_unrealizable_plays,
            matches_by_seat_order,
            match_records: matches,
        }
    }
}

fn outcome_name(outcome: Option<&Outcome>) -> &'static str {
    match outcome {
        Some(Outcome::Victory { .. }) => "victory",
        Some(Outcome::Draw { .. }) => "draw",
        Some(Outcome::Cancelled { .. }) => "cancelled",
        None => "incomplete",
    }
}

fn read_performance(path: &Path) -> Result<Option<TournamentPerformance>, TournamentError> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn merge_performance(
    prior: Option<TournamentPerformance>,
    current: Vec<MatchPerformance>,
) -> Vec<MatchPerformance> {
    let mut records = prior.map_or_else(Vec::new, |performance| performance.match_records);
    for record in current {
        if let Some(existing) = records.iter_mut().find(|existing| {
            existing.match_id == record.match_id && existing.attempt == record.attempt
        }) {
            *existing = record;
        } else {
            records.push(record);
        }
    }
    records.sort_by(|left, right| left.match_id.cmp(&right.match_id));
    records
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

#[cfg(test)]
mod tests {
    use super::*;
    use awvm::semantic::{AwbwVisibility, observe};

    #[test]
    fn search_factory_budget_controls_agent_execution() {
        let factory = SearchFactory::new(
            "budget-test",
            BaselineConfig::PRODUCTION.weights,
            EvalWeights::STANDARD,
            NodeBudget::ONE,
        );
        let state = awbrn_ai::board::arena(false, 7);
        let view = observe(&AwbwVisibility, &state, &state.turn.active_player)
            .expect("the active player observes the arena");
        let mut agent = factory.create(29);

        agent.act(&view, NodeBudget::SIXTEEN);

        let stats = agent
            .search_stats()
            .expect("the factory creates a search agent");
        assert_eq!(stats.nodes_evaluated, u64::from(NodeBudget::ONE.get()));
    }

    #[test]
    fn performance_uses_latest_attempt_and_keeps_audit_rows() {
        let old = MatchPerformance {
            match_id: "match-a".into(),
            attempt: 0,
            map_id: 1,
            seat_order: SeatOrderVariant::AgentFirst,
            elapsed_nanos: 100,
            turns: 4,
            days: 4,
            commands: 10,
            invalid_commands: 5,
            refusals: 2,
            unrealizable_plays: 3,
            outcome: "incomplete".into(),
        };
        let retry = MatchPerformance {
            match_id: "match-a".into(),
            attempt: 1,
            map_id: 1,
            seat_order: SeatOrderVariant::AgentFirst,
            elapsed_nanos: 20,
            turns: 2,
            days: 2,
            commands: 4,
            invalid_commands: 3,
            refusals: 1,
            unrealizable_plays: 2,
            outcome: "victory".into(),
        };
        let other = MatchPerformance {
            match_id: "match-b".into(),
            attempt: 0,
            map_id: 1,
            seat_order: SeatOrderVariant::BaselineFirst,
            elapsed_nanos: 30,
            turns: 3,
            days: 3,
            commands: 6,
            invalid_commands: 1,
            refusals: 1,
            unrealizable_plays: 0,
            outcome: "draw".into(),
        };

        let performance = TournamentPerformance::from_matches(vec![old, retry, other], 99);
        assert_eq!(performance.matches, 2);
        assert_eq!(performance.total_match_nanos, 50);
        assert_eq!(performance.total_commands, 10);
        assert_eq!(performance.total_invalid_commands, 4);
        assert_eq!(performance.total_refusals, 2);
        assert_eq!(performance.total_unrealizable_plays, 2);
        assert_eq!(performance.match_records.len(), 3);
        assert_eq!(performance.wall_clock_nanos, 99);
    }
}
