//! Generic deterministic paired-match execution.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use awbrn_ai::EvalWeights;
use awbrn_ai::FNV1A_OFFSET_BASIS;
use awbrn_ai::agent::{Agent, NodeBudget, SearchStats};
use awbrn_ai::agents::{SearchAgent, SearchAllocator, StrategicAgent, Weights};
use awbrn_ai::baseline::BaselineConfig;
use awbrn_ai::harness::{Limits, RefusalTrace, next_command_fingerprint, play_observed_fallible};
use awbrn_ai::rng::Rng;
use awbrn_ai::{AiProfile, profile};
use awbrn_ai_diagnostic_types::{
    AgentIdentity, PairKey, Reduction, RunLimits, RunManifest, RunManifestError, SeatOrderVariant,
    fingerprint_bytes,
};
use awvm::semantic::Outcome;
use awvm::session::Session;
use awvm::transition::{Command, ExecuteError};
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

/// The executable identity for a versioned game profile.
pub const AI_PROFILE_EXECUTABLE_FINGERPRINT: &str = "awbrn-ai-versioned-profile-v1";

/// A factory for one versioned game profile.
#[derive(Clone, Debug)]
pub struct AiProfileFactory {
    profile: AiProfile,
    identity: AgentIdentity,
}

impl AiProfileFactory {
    /// Create a factory for a profile stored in match records.
    pub fn new(profile_id: &str) -> Result<Self, String> {
        let profile = profile(profile_id)
            .copied()
            .ok_or_else(|| format!("unknown AI profile {profile_id}"))?;
        Ok(Self {
            profile,
            identity: AgentIdentity {
                identifier: profile.id.to_owned(),
                configuration_fingerprint: profile.configuration_fingerprint(),
                executable_fingerprint: AI_PROFILE_EXECUTABLE_FINGERPRINT.into(),
            },
        })
    }
}

impl AgentFactory for AiProfileFactory {
    fn identity(&self) -> &AgentIdentity {
        &self.identity
    }

    fn create(&self, seed: u64) -> Box<dyn Agent> {
        self.profile.agent(seed)
    }
}

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
    allocator: SearchAllocator,
}

/// The executable identity for the reply-search implementation.
pub const SEARCH_EXECUTABLE_FINGERPRINT: &str = "awbrn-ai-search-v1";

impl SearchFactory {
    /// Create a factory from the search configuration it will run.
    ///
    /// The configuration fingerprint uses the identifier, weights, evaluator
    /// weights, and node budget. It does not include the allocator.
    pub fn new(
        identifier: &str,
        weights: Weights,
        eval_weights: EvalWeights,
        node_budget: NodeBudget,
    ) -> Self {
        Self::from_identity(
            identifier,
            weights,
            eval_weights,
            node_budget,
            SearchAllocator::SequentialQuota,
            Self::configuration_fingerprint(identifier, weights, eval_weights, node_budget),
        )
    }

    /// Create a search factory with an explicit allocator.
    ///
    /// The allocator is not part of the configuration fingerprint.
    pub fn new_with_allocator(
        identifier: &str,
        weights: Weights,
        eval_weights: EvalWeights,
        node_budget: NodeBudget,
        allocator: SearchAllocator,
    ) -> Self {
        Self::from_identity(
            identifier,
            weights,
            eval_weights,
            node_budget,
            allocator,
            Self::configuration_fingerprint(identifier, weights, eval_weights, node_budget),
        )
    }

    fn configuration_fingerprint(
        identifier: &str,
        weights: Weights,
        eval_weights: EvalWeights,
        node_budget: NodeBudget,
    ) -> String {
        let bytes = serde_json::to_vec(&(identifier, weights, eval_weights, node_budget))
            .expect("search configuration serializes");
        fingerprint_bytes(&bytes)
    }

    fn from_identity(
        identifier: &str,
        weights: Weights,
        eval_weights: EvalWeights,
        node_budget: NodeBudget,
        allocator: SearchAllocator,
        fingerprint: String,
    ) -> Self {
        Self {
            identity: AgentIdentity {
                identifier: identifier.to_owned(),
                configuration_fingerprint: fingerprint,
                executable_fingerprint: SEARCH_EXECUTABLE_FINGERPRINT.into(),
            },
            weights,
            eval_weights,
            node_budget,
            allocator,
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
                .with_node_budget(self.node_budget)
                .with_allocator(self.allocator),
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
    /// Commands rejected by deterministic preflight.
    #[serde(default)]
    pub total_preflight_rejections: u64,
    #[serde(default)]
    pub total_unrealizable_plays: u64,
    /// Invalid decisions made by the candidate agent.
    #[serde(default)]
    pub candidate_invalid_commands: u64,
    /// Invalid decisions made by the baseline agent.
    #[serde(default)]
    pub baseline_invalid_commands: u64,
    /// Candidate configuration fingerprint.
    #[serde(default)]
    pub candidate_configuration_fingerprint: String,
    /// Baseline configuration fingerprint.
    #[serde(default)]
    pub baseline_configuration_fingerprint: String,
    /// Candidate commands rejected by deterministic preflight.
    #[serde(default)]
    pub candidate_preflight_rejections: u64,
    /// Baseline commands rejected by deterministic preflight.
    #[serde(default)]
    pub baseline_preflight_rejections: u64,
    /// Candidate complete-turn timing.
    #[serde(default)]
    pub candidate_complete_turn_timing: CompleteTurnTiming,
    /// Baseline complete-turn timing.
    #[serde(default)]
    pub baseline_complete_turn_timing: CompleteTurnTiming,
    /// Unrealizable plays made by the candidate agent.
    #[serde(default)]
    pub candidate_unrealizable_plays: u64,
    /// Unrealizable plays made by the baseline agent.
    #[serde(default)]
    pub baseline_unrealizable_plays: u64,
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
    /// Pair identity shared by both seat orders.
    #[serde(default = "default_pair_key")]
    pub pair: PairKey,
    pub seat_order: SeatOrderVariant,
    pub elapsed_nanos: u64,
    /// Candidate complete accepted-turn durations.
    #[serde(default)]
    pub candidate_complete_turn_times_nanos: Vec<u64>,
    /// Baseline complete accepted-turn durations.
    #[serde(default)]
    pub baseline_complete_turn_times_nanos: Vec<u64>,
    pub turns: u32,
    pub days: u32,
    pub commands: u64,
    pub invalid_commands: u64,
    #[serde(default)]
    pub refusals: u64,
    /// Commands rejected by deterministic preflight.
    pub preflight_rejections: u64,
    #[serde(default)]
    pub unrealizable_plays: u64,
    /// Invalid decisions made by the candidate agent.
    #[serde(default)]
    pub candidate_invalid_commands: u64,
    /// Invalid decisions made by the baseline agent.
    #[serde(default)]
    pub baseline_invalid_commands: u64,
    /// Candidate configuration fingerprint.
    #[serde(default)]
    pub candidate_configuration_fingerprint: String,
    /// Baseline configuration fingerprint.
    #[serde(default)]
    pub baseline_configuration_fingerprint: String,
    /// Candidate commands rejected by deterministic preflight.
    #[serde(default)]
    pub candidate_preflight_rejections: u64,
    /// Baseline commands rejected by deterministic preflight.
    #[serde(default)]
    pub baseline_preflight_rejections: u64,
    /// Unrealizable plays made by the candidate agent.
    #[serde(default)]
    pub candidate_unrealizable_plays: u64,
    /// Unrealizable plays made by the baseline agent.
    #[serde(default)]
    pub baseline_unrealizable_plays: u64,
    pub outcome: String,
    /// Search counters for the candidate, when it is a search agent.
    #[serde(default)]
    pub candidate_search_stats: Option<SearchStats>,
    /// Search counters for the baseline, when it is a search agent.
    #[serde(default)]
    pub baseline_search_stats: Option<SearchStats>,
    /// Candidate search decision times in nanoseconds.
    #[serde(default)]
    pub candidate_decision_times_nanos: Vec<u64>,
    /// Baseline search decision times in nanoseconds.
    #[serde(default)]
    pub baseline_decision_times_nanos: Vec<u64>,
    /// Reducer refusals with optional agent traces.
    #[serde(default)]
    pub refusal_traces: Vec<RefusalTrace>,
}

/// Summary of complete accepted turns for one agent.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CompleteTurnTiming {
    /// Number of samples.
    pub samples: usize,
    /// Median duration in nanoseconds.
    pub median_nanos: u64,
    /// 95th percentile duration in nanoseconds.
    pub p95_nanos: u64,
    /// Mean duration in nanoseconds.
    pub mean_nanos: u64,
    /// Maximum duration in nanoseconds.
    pub maximum_nanos: u64,
}

fn default_pair_key() -> PairKey {
    PairKey::new(0, 0, 0)
}

/// Return the latest record for each match ID.
pub(crate) fn latest_match_records(records: &[MatchPerformance]) -> Vec<&MatchPerformance> {
    let mut latest = BTreeMap::<&str, &MatchPerformance>::new();
    for record in records {
        if latest
            .get(record.match_id.as_str())
            .is_none_or(|existing| record.attempt > existing.attempt)
        {
            latest.insert(record.match_id.as_str(), record);
        }
    }
    latest.into_values().collect()
}

/// Version of the derived search coverage artifact.
pub const SEARCH_COVERAGE_SCHEMA_VERSION: u16 = 2;

/// A search coverage row for one completed match attempt.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchCoverageMatch {
    /// Stable match identity.
    pub match_id: String,
    /// Registered map identity.
    pub map_id: u32,
    /// Seat order used by the match.
    pub seat_order: SeatOrderVariant,
    /// Event-log attempt number.
    pub attempt: u32,
    /// Candidate search counters.
    pub candidate: Option<SearchStats>,
    /// Baseline search counters.
    pub baseline: Option<SearchStats>,
}

/// Machine-readable search coverage derived from match performance rows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchCoverageArtifact {
    /// Artifact schema version.
    pub schema_version: u16,
    /// Source manifest configuration identity.
    pub configuration_fingerprint: String,
    /// Active match rows in stable order.
    pub matches: Vec<SearchCoverageMatch>,
    /// Aggregate candidate counters.
    pub candidate: SearchStats,
    /// Aggregate baseline counters.
    pub baseline: SearchStats,
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
    #[error("match execution error: {0}")]
    Execute(#[from] ExecuteError),
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
    if let Some(performance) = &prior_performance {
        validate_performance_fingerprints(
            &performance.match_records,
            &manifest.agents[0].configuration_fingerprint,
            &manifest.agents[1].configuration_fingerprint,
        )?;
    }
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
    let performance_records = merge_performance(prior_performance, match_performance);
    validate_performance_fingerprints(
        &performance_records,
        &manifest.agents[0].configuration_fingerprint,
        &manifest.agents[1].configuration_fingerprint,
    )?;
    let performance = TournamentPerformance::from_matches(
        performance_records,
        prior_wall_clock_nanos.saturating_add(current_wall_clock_nanos),
        &manifest.agents[0].configuration_fingerprint,
        &manifest.agents[1].configuration_fingerprint,
    );
    fs::write(
        output.join("performance.json"),
        serde_json::to_vec_pretty(&performance)?,
    )?;
    write_search_coverage(&output, manifest, &performance)?;
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

fn write_search_coverage(
    output: &Path,
    manifest: &RunManifest,
    performance: &TournamentPerformance,
) -> Result<(), TournamentError> {
    let latest = latest_match_records(&performance.match_records);
    let mut candidate = SearchStats::default();
    let mut baseline = SearchStats::default();
    let matches = latest
        .into_iter()
        .map(|record| {
            if let Some(stats) = &record.candidate_search_stats {
                candidate.add(stats.clone());
            }
            if let Some(stats) = &record.baseline_search_stats {
                baseline.add(stats.clone());
            }
            SearchCoverageMatch {
                match_id: record.match_id.clone(),
                map_id: record.map_id,
                seat_order: record.seat_order,
                attempt: record.attempt,
                candidate: record.candidate_search_stats.clone(),
                baseline: record.baseline_search_stats.clone(),
            }
        })
        .collect();
    let artifact = SearchCoverageArtifact {
        schema_version: SEARCH_COVERAGE_SCHEMA_VERSION,
        configuration_fingerprint: manifest.configuration_fingerprint.clone(),
        matches,
        candidate,
        baseline,
    };
    fs::write(
        output.join("search-coverage.json"),
        serde_json::to_vec_pretty(&artifact)?,
    )?;
    Ok(())
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
    let candidate_configuration_fingerprint =
        candidate.identity().configuration_fingerprint.clone();
    let baseline_configuration_fingerprint = baseline.identity().configuration_fingerprint.clone();
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
    let result: Result<_, TournamentError> = play_observed_fallible(
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
    let candidate_search_stats = candidate.search_stats();
    let baseline_search_stats = baseline.search_stats();
    let candidate_decision_times_nanos =
        candidate.search_decision_times_nanos().unwrap_or_default();
    let baseline_decision_times_nanos = baseline.search_decision_times_nanos().unwrap_or_default();
    let (candidate_seat, baseline_seat) = match seat_order {
        SeatOrderVariant::AgentFirst => (0, 1),
        SeatOrderVariant::BaselineFirst => (1, 0),
    };
    let candidate_refusals = record
        .refusals_by_seat
        .get(candidate_seat)
        .copied()
        .unwrap_or_default();
    let candidate_preflight_rejections = record
        .preflight_rejections_by_seat
        .get(candidate_seat)
        .copied()
        .unwrap_or_default();
    let baseline_refusals = record
        .refusals_by_seat
        .get(baseline_seat)
        .copied()
        .unwrap_or_default();
    let baseline_preflight_rejections = record
        .preflight_rejections_by_seat
        .get(baseline_seat)
        .copied()
        .unwrap_or_default();
    let candidate_complete_turn_times_nanos = record
        .complete_turn_times_by_seat
        .get(candidate_seat)
        .cloned()
        .unwrap_or_default();
    let baseline_complete_turn_times_nanos = record
        .complete_turn_times_by_seat
        .get(baseline_seat)
        .cloned()
        .unwrap_or_default();
    let candidate_unrealizable_plays = record
        .unrealizable_plays_by_seat
        .get(candidate_seat)
        .copied()
        .unwrap_or_default();
    let baseline_unrealizable_plays = record
        .unrealizable_plays_by_seat
        .get(baseline_seat)
        .copied()
        .unwrap_or_default();
    Ok(MatchPerformance {
        match_id: metadata.match_id,
        attempt,
        map_id: pair.map_id,
        pair: pair.clone(),
        seat_order,
        elapsed_nanos: match_started
            .elapsed()
            .as_nanos()
            .try_into()
            .unwrap_or(u64::MAX),
        candidate_complete_turn_times_nanos,
        baseline_complete_turn_times_nanos,
        turns: record.turns,
        days: record.days,
        commands: record.commands,
        invalid_commands: record
            .refusals
            .saturating_add(record.preflight_rejections)
            .saturating_add(record.unrealizable_plays),
        refusals: record.refusals,
        preflight_rejections: record.preflight_rejections,
        unrealizable_plays: record.unrealizable_plays,
        candidate_invalid_commands: candidate_refusals
            .saturating_add(candidate_preflight_rejections)
            .saturating_add(candidate_unrealizable_plays),
        baseline_invalid_commands: baseline_refusals
            .saturating_add(baseline_preflight_rejections)
            .saturating_add(baseline_unrealizable_plays),
        candidate_configuration_fingerprint,
        baseline_configuration_fingerprint,
        candidate_preflight_rejections,
        baseline_preflight_rejections,
        candidate_unrealizable_plays,
        baseline_unrealizable_plays,
        outcome: outcome_name(record.outcome.as_ref()).into(),
        candidate_search_stats,
        baseline_search_stats,
        candidate_decision_times_nanos,
        baseline_decision_times_nanos,
        refusal_traces: record.refusal_traces,
    })
}

impl TournamentPerformance {
    fn from_matches(
        matches: Vec<MatchPerformance>,
        wall_clock_nanos: u64,
        candidate_configuration_fingerprint: &str,
        baseline_configuration_fingerprint: &str,
    ) -> Self {
        let active = latest_match_records(&matches);
        let total_match_nanos = active
            .iter()
            .map(|record| record.elapsed_nanos)
            .sum::<u64>();
        let total_commands = active.iter().map(|record| record.commands).sum();
        let total_invalid_commands = active.iter().map(|record| record.invalid_commands).sum();
        let total_refusals = active.iter().map(|record| record.refusals).sum();
        let total_preflight_rejections = active
            .iter()
            .map(|record| record.preflight_rejections)
            .sum();
        let total_unrealizable_plays = active.iter().map(|record| record.unrealizable_plays).sum();
        let candidate_invalid_commands = active
            .iter()
            .map(|record| record.candidate_invalid_commands)
            .sum();
        let baseline_invalid_commands = active
            .iter()
            .map(|record| record.baseline_invalid_commands)
            .sum();
        let candidate_preflight_rejections = active
            .iter()
            .map(|record| record.candidate_preflight_rejections)
            .sum();
        let baseline_preflight_rejections = active
            .iter()
            .map(|record| record.baseline_preflight_rejections)
            .sum();
        let candidate_unrealizable_plays = active
            .iter()
            .map(|record| record.candidate_unrealizable_plays)
            .sum();
        let baseline_unrealizable_plays = active
            .iter()
            .map(|record| record.baseline_unrealizable_plays)
            .sum();
        let candidate_complete_turn_timing = summarize_complete_turns(
            active
                .iter()
                .flat_map(|record| record.candidate_complete_turn_times_nanos.iter().copied()),
        );
        let baseline_complete_turn_timing = summarize_complete_turns(
            active
                .iter()
                .flat_map(|record| record.baseline_complete_turn_times_nanos.iter().copied()),
        );
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
            total_preflight_rejections,
            total_unrealizable_plays,
            candidate_invalid_commands,
            baseline_invalid_commands,
            candidate_configuration_fingerprint: candidate_configuration_fingerprint.to_owned(),
            baseline_configuration_fingerprint: baseline_configuration_fingerprint.to_owned(),
            candidate_preflight_rejections,
            baseline_preflight_rejections,
            candidate_complete_turn_timing,
            baseline_complete_turn_timing,
            candidate_unrealizable_plays,
            baseline_unrealizable_plays,
            matches_by_seat_order,
            match_records: matches,
        }
    }
}

pub(crate) fn summarize_complete_turns<I>(samples: I) -> CompleteTurnTiming
where
    I: IntoIterator<Item = u64>,
{
    let mut ordered = samples.into_iter().collect::<Vec<_>>();
    ordered.sort_unstable();
    if ordered.is_empty() {
        return CompleteTurnTiming::default();
    }
    let sum = ordered.iter().copied().map(u128::from).sum::<u128>();
    let p95_index = ((ordered.len() as f64 * 0.95).ceil() as usize).saturating_sub(1);
    let middle = ordered.len() / 2;
    let median_nanos = if ordered.len() % 2 == 0 {
        (u128::from(ordered[middle - 1]) + u128::from(ordered[middle])) / 2
    } else {
        u128::from(ordered[middle])
    };
    CompleteTurnTiming {
        samples: ordered.len(),
        median_nanos: median_nanos.try_into().unwrap_or(u64::MAX),
        p95_nanos: ordered[p95_index],
        mean_nanos: (sum / ordered.len() as u128).try_into().unwrap_or(u64::MAX),
        maximum_nanos: *ordered.last().expect("complete-turn samples are not empty"),
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

fn validate_performance_fingerprints(
    records: &[MatchPerformance],
    candidate: &str,
    baseline: &str,
) -> Result<(), TournamentError> {
    if let Some(record) = records.iter().find(|record| {
        record.candidate_configuration_fingerprint != candidate
            || record.baseline_configuration_fingerprint != baseline
    }) {
        return Err(TournamentError::Configuration(format!(
            "performance record {} attempt {} does not match the manifest agent fingerprints",
            record.match_id, record.attempt
        )));
    }
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
    fn allocator_does_not_change_configuration_fingerprint() {
        let default_allocator = SearchFactory::new(
            "search-sweep",
            BaselineConfig::PRODUCTION.weights,
            EvalWeights::STANDARD,
            NodeBudget::FOUR,
        );
        let sequential_quota = SearchFactory::new_with_allocator(
            "search-sweep",
            BaselineConfig::PRODUCTION.weights,
            EvalWeights::STANDARD,
            NodeBudget::FOUR,
            SearchAllocator::SequentialQuota,
        );
        let round_robin = SearchFactory::new_with_allocator(
            "search-sweep",
            BaselineConfig::PRODUCTION.weights,
            EvalWeights::STANDARD,
            NodeBudget::FOUR,
            SearchAllocator::RoundRobin,
        );
        assert_eq!(
            default_allocator.identity().configuration_fingerprint,
            sequential_quota.identity().configuration_fingerprint,
        );
        assert_eq!(
            sequential_quota.identity().configuration_fingerprint,
            round_robin.identity().configuration_fingerprint
        );

        let larger_budget = SearchFactory::new_with_allocator(
            "search-sweep",
            BaselineConfig::PRODUCTION.weights,
            EvalWeights::STANDARD,
            NodeBudget::SIXTEEN,
            SearchAllocator::SequentialQuota,
        );
        assert_ne!(
            sequential_quota.identity().configuration_fingerprint,
            larger_budget.identity().configuration_fingerprint
        );
    }

    #[test]
    fn performance_uses_latest_attempt_and_keeps_audit_rows() {
        let old = MatchPerformance {
            match_id: "match-a".into(),
            attempt: 0,
            map_id: 1,
            pair: PairKey::new(1, 7, 0),
            seat_order: SeatOrderVariant::AgentFirst,
            elapsed_nanos: 100,
            candidate_complete_turn_times_nanos: Vec::new(),
            baseline_complete_turn_times_nanos: Vec::new(),
            turns: 4,
            days: 4,
            commands: 10,
            invalid_commands: 5,
            refusals: 2,
            preflight_rejections: 0,
            unrealizable_plays: 3,
            candidate_invalid_commands: 0,
            baseline_invalid_commands: 0,
            candidate_configuration_fingerprint: "candidate".into(),
            baseline_configuration_fingerprint: "baseline".into(),
            candidate_preflight_rejections: 0,
            baseline_preflight_rejections: 0,
            candidate_unrealizable_plays: 0,
            baseline_unrealizable_plays: 0,
            outcome: "incomplete".into(),
            candidate_search_stats: None,
            baseline_search_stats: None,
            candidate_decision_times_nanos: Vec::new(),
            baseline_decision_times_nanos: Vec::new(),
            refusal_traces: Vec::new(),
        };
        let retry = MatchPerformance {
            match_id: "match-a".into(),
            attempt: 1,
            map_id: 1,
            pair: PairKey::new(1, 7, 0),
            seat_order: SeatOrderVariant::AgentFirst,
            elapsed_nanos: 20,
            candidate_complete_turn_times_nanos: vec![30, 10, 20],
            baseline_complete_turn_times_nanos: vec![40, 50],
            turns: 2,
            days: 2,
            commands: 4,
            invalid_commands: 3,
            refusals: 1,
            preflight_rejections: 1,
            unrealizable_plays: 2,
            candidate_invalid_commands: 1,
            baseline_invalid_commands: 2,
            candidate_configuration_fingerprint: "candidate".into(),
            baseline_configuration_fingerprint: "baseline".into(),
            candidate_preflight_rejections: 1,
            baseline_preflight_rejections: 0,
            candidate_unrealizable_plays: 1,
            baseline_unrealizable_plays: 1,
            outcome: "victory".into(),
            candidate_search_stats: None,
            baseline_search_stats: None,
            candidate_decision_times_nanos: Vec::new(),
            baseline_decision_times_nanos: Vec::new(),
            refusal_traces: Vec::new(),
        };
        let other = MatchPerformance {
            match_id: "match-b".into(),
            attempt: 0,
            map_id: 1,
            pair: PairKey::new(1, 7, 1),
            seat_order: SeatOrderVariant::BaselineFirst,
            elapsed_nanos: 30,
            candidate_complete_turn_times_nanos: vec![60],
            baseline_complete_turn_times_nanos: vec![70, 80],
            turns: 3,
            days: 3,
            commands: 6,
            invalid_commands: 1,
            refusals: 1,
            preflight_rejections: 0,
            unrealizable_plays: 0,
            candidate_invalid_commands: 1,
            baseline_invalid_commands: 0,
            candidate_configuration_fingerprint: "candidate".into(),
            baseline_configuration_fingerprint: "baseline".into(),
            candidate_preflight_rejections: 0,
            baseline_preflight_rejections: 0,
            candidate_unrealizable_plays: 0,
            baseline_unrealizable_plays: 0,
            outcome: "draw".into(),
            candidate_search_stats: None,
            baseline_search_stats: None,
            candidate_decision_times_nanos: Vec::new(),
            baseline_decision_times_nanos: Vec::new(),
            refusal_traces: Vec::new(),
        };

        let mut mismatched = other.clone();
        mismatched.candidate_configuration_fingerprint = "different".into();
        assert!(validate_performance_fingerprints(&[mismatched], "candidate", "baseline").is_err());

        let performance = TournamentPerformance::from_matches(
            vec![old, retry, other],
            99,
            "candidate",
            "baseline",
        );
        assert_eq!(performance.matches, 2);
        assert_eq!(performance.total_match_nanos, 50);
        assert_eq!(performance.total_commands, 10);
        assert_eq!(performance.total_invalid_commands, 4);
        assert_eq!(performance.total_refusals, 2);
        assert_eq!(performance.total_preflight_rejections, 1);
        assert_eq!(performance.total_unrealizable_plays, 2);
        assert_eq!(performance.candidate_invalid_commands, 2);
        assert_eq!(performance.baseline_invalid_commands, 2);
        assert_eq!(performance.candidate_preflight_rejections, 1);
        assert_eq!(performance.baseline_preflight_rejections, 0);
        assert_eq!(performance.candidate_complete_turn_timing.samples, 4);
        assert_eq!(performance.candidate_complete_turn_timing.median_nanos, 25);
        assert_eq!(performance.candidate_complete_turn_timing.p95_nanos, 60);
        assert_eq!(performance.baseline_complete_turn_timing.samples, 4);
        assert_eq!(performance.baseline_complete_turn_timing.p95_nanos, 80);
        assert_eq!(performance.candidate_configuration_fingerprint, "candidate");
        assert_eq!(performance.baseline_configuration_fingerprint, "baseline");
        assert_eq!(performance.candidate_unrealizable_plays, 1);
        assert_eq!(performance.baseline_unrealizable_plays, 1);
        assert_eq!(performance.match_records.len(), 3);
        assert_eq!(performance.wall_clock_nanos, 99);
    }
}
