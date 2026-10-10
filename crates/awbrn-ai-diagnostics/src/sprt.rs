//! Sequential paired tests for development work.
//!
//! A frozen gate plays a fixed number of games and then compares one mean
//! with one threshold. That design needs many games to find a small gain, and
//! it uses the same number of games for a clear result and for a close one.
//! This module plays paired games until a sequential probability ratio test
//! (SPRT) makes a decision, or until a maximum pair count.
//!
//! One pair is two games on one map with one match seed. The candidate plays
//! the first seat in one game and the second seat in the other game. The pair
//! differential is the candidate points from both games minus one. It is in
//! `[-1, 1]`, and zero means no difference.
//!
//! The command retains its SPRT name. The test uses bounded betting evidence.
//! Each observation is the mean of one complete round of maps. This gives
//! each map the same weight. Bets depend only on previous rounds.
//!
//! Evidence against H0 multiplies `1 + bet * (round_mean - h0)`.
//! Evidence against H1 multiplies `1 + bet * (h1 - round_mean)`.
//! The bets keep each factor positive for every outcome in `[-1, 1]`.
//! Under the corresponding hypothesis the evidence is a nonnegative
//! supermartingale. Ville's inequality bounds the probability that evidence
//! ever exceeds `1 / alpha` or `1 / beta`.
//!
//! The bounds require independent fresh seed samples whose expected round
//! mean satisfies the hypothesis. Repeated tuning on the same seeds does not
//! meet this requirement. Deterministic seed generation alone supplies no
//! probability model.
//!
//! Reference: Waudby-Smith and Ramdas, "Estimating means of bounded random
//! variables by betting", https://arxiv.org/abs/2010.09686.
//!
//! Workers play pairs in parallel. The runner commits results in pair order.
//! The decision is the same for each worker count.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use awbrn_ai::agent::{Agent, NodeBudget};
use awbrn_ai::baseline::BaselineConfig;
use awbrn_ai::harness::{Limits, play_measured};
use awbrn_ai::planner::PlannerStats;
use awbrn_ai::rng::Rng;
use awbrn_ai_diagnostic_types::{
    AgentIdentity, AgentSeedProtocol, RunLimits, SeatOrderVariant, fingerprint_bytes,
};
use awvm::ruleset::CommanderKind;
use awvm::semantic::{Match, Outcome, State};
use awvm::session::Session;
use serde::{Deserialize, Serialize};

use crate::map_registry::{MapManifest, MapRegistry, RegisteredMap};
use crate::plan::{AgentSpec, PlanError};
use crate::source::{SourceProvenance, source_provenance};
use crate::tournament::{
    AgentFactory, CompleteTurnTiming, match_agent_seeds, summarize_complete_turns,
};

/// The current SPRT plan schema.
pub const SPRT_PLAN_SCHEMA_VERSION: u16 = 1;

/// The current SPRT result schema.
pub const SPRT_RESULT_SCHEMA_VERSION: u16 = 4;

/// The minimum number of pairs before the test can make a decision.
///
/// This prevents a decision from a very short run. The evidence thresholds
/// provide the error bounds.
pub const SPRT_MINIMUM_PAIRS: usize = 16;

/// The hypotheses and the error rates of one test.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SprtBounds {
    /// The mean pair differential under the null hypothesis.
    pub h0: f64,
    /// The mean pair differential under the alternative hypothesis.
    pub h1: f64,
    /// The probability to accept `H1` when `H0` is true.
    pub alpha: f64,
    /// The probability to accept `H0` when `H1` is true.
    pub beta: f64,
}

impl SprtBounds {
    /// The default development test: no gain against a gain of 0.05 points.
    pub const DEFAULT: Self = Self {
        h0: 0.0,
        h1: 0.05,
        alpha: 0.05,
        beta: 0.05,
    };

    /// Return the log evidence thresholds against H0 and H1.
    pub fn limits(self) -> (f64, f64) {
        (-self.alpha.ln(), -self.beta.ln())
    }

    fn validate(self) -> Result<(), String> {
        let rate = |value: f64| value > 0.0 && value < 0.5;
        if !(-1.0..1.0).contains(&self.h0) || !(-1.0..1.0).contains(&self.h1) || self.h1 <= self.h0
        {
            return Err("SPRT bounds need -1 <= h0 < h1 < 1".into());
        }
        if !rate(self.alpha) || !rate(self.beta) {
            return Err("SPRT alpha and beta must be in (0, 0.5)".into());
        }
        Ok(())
    }
}

impl Default for SprtBounds {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// One user-authored SPRT run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SprtPlan {
    pub schema_version: u16,
    pub run_id: String,
    pub candidate: AgentSpec,
    pub baseline: AgentSpec,
    /// A map manifest relative to the plan. Without it, the run uses the
    /// checked-in registry.
    #[serde(default)]
    pub map_manifest: Option<PathBuf>,
    /// The maps in the pool. Pairs use the maps in turn.
    pub maps: Vec<u32>,
    /// Set fog for all maps. Without it, each map uses its registry value.
    #[serde(default)]
    pub fog: Option<bool>,
    /// Set the commanders for physical seats in map order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commanders: Option<[CommanderKind; 2]>,
    pub run_seed: u64,
    /// Assign agent random streams by role or by physical seat.
    #[serde(default, skip_serializing_if = "is_role_seeded")]
    pub agent_seed_protocol: AgentSeedProtocol,
    /// The test stops without a decision after this many pairs.
    pub max_pairs: usize,
    /// Stop when evidence crosses a threshold. False plays every pair.
    #[serde(default = "stop_early_default")]
    pub stop_early: bool,
    pub limits: RunLimits,
    #[serde(default)]
    pub bounds: SprtBounds,
}

fn stop_early_default() -> bool {
    true
}

fn is_role_seeded(protocol: &AgentSeedProtocol) -> bool {
    *protocol == AgentSeedProtocol::RoleSeeded
}

/// The test state after some pairs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SprtStatistic {
    pub pairs: usize,
    pub mean: f64,
    pub variance: f64,
    /// A descriptive normal interval half width. Coverage is not preserved
    /// after a sequential stop.
    pub ci95_half_width: f64,
    pub rounds: usize,
    pub log_e_h0: f64,
    pub log_e_h1: f64,
}

/// What the test decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SprtDecision {
    /// The evidence rejects H0.
    AcceptH1,
    /// The evidence rejects H1.
    AcceptH0,
    /// The test reached the pair limit before a decision.
    Inconclusive,
}

/// Pair statistics and evidence from complete rounds of maps.
#[derive(Clone, Copy, Debug)]
pub struct SprtAccumulator {
    bounds: SprtBounds,
    maps_per_round: usize,
    pairs: usize,
    sum: f64,
    sum_squares: f64,
    round_sum: f64,
    rounds: usize,
    round_total: f64,
    round_squares: f64,
    log_e_h0: f64,
    log_e_h1: f64,
}

impl Default for SprtAccumulator {
    fn default() -> Self {
        Self::new(SprtBounds::DEFAULT, std::num::NonZeroUsize::MIN)
            .expect("the default bounds are valid")
    }
}

impl SprtAccumulator {
    /// Build an accumulator for a fixed set of hypotheses and maps.
    pub fn new(
        bounds: SprtBounds,
        maps_per_round: std::num::NonZeroUsize,
    ) -> Result<Self, SprtError> {
        bounds.validate().map_err(SprtError::Configuration)?;
        Ok(Self {
            bounds,
            maps_per_round: maps_per_round.get(),
            pairs: 0,
            sum: 0.0,
            sum_squares: 0.0,
            round_sum: 0.0,
            rounds: 0,
            round_total: 0.0,
            round_squares: 0.0,
            log_e_h0: 0.0,
            log_e_h1: 0.0,
        })
    }

    /// Add a pair differential in `[-1, 1]`.
    pub fn push(&mut self, differential: f64) -> Result<(), SprtError> {
        if !(-1.0..=1.0).contains(&differential) {
            return Err(SprtError::Configuration(
                "pair differential must be in [-1, 1]".into(),
            ));
        }
        self.pairs += 1;
        self.sum += differential;
        self.sum_squares += differential * differential;
        self.round_sum += differential;
        if !self.pairs.is_multiple_of(self.maps_per_round) {
            return Ok(());
        }
        let outcome = self.round_sum / self.maps_per_round as f64;
        self.round_sum = 0.0;
        let n = self.rounds as f64;
        let mean = if self.rounds == 0 {
            (self.bounds.h0 + self.bounds.h1) / 2.0
        } else {
            self.round_total / n
        };
        let variance = if self.rounds < 2 {
            1.0
        } else {
            ((self.round_squares - n * mean * mean) / (n - 1.0)).max(0.0)
        };
        // Estimate a useful bet from previous rounds. The cap keeps each
        // factor at least 0.5 for all possible round outcomes.
        let bet = |edge: f64, bound: f64| {
            (edge.max(0.0) / (variance + edge * edge).max(0.01)).min(0.5 / (1.0 + bound.abs()))
        };
        let up = bet(mean - self.bounds.h0, self.bounds.h0);
        let down = bet(self.bounds.h1 - mean, self.bounds.h1);
        self.log_e_h0 += (up * (outcome - self.bounds.h0)).ln_1p();
        self.log_e_h1 += (down * (self.bounds.h1 - outcome)).ln_1p();
        self.rounds += 1;
        self.round_total += outcome;
        self.round_squares += outcome * outcome;
        Ok(())
    }

    /// Return pair statistics and the current log evidence.
    pub fn statistic(&self) -> SprtStatistic {
        if self.pairs == 0 {
            return SprtStatistic::default();
        }
        let n = self.pairs as f64;
        let mean = self.sum / n;
        let variance = if self.pairs > 1 {
            ((self.sum_squares - n * mean * mean) / (n - 1.0)).max(0.0)
        } else {
            0.0
        };
        SprtStatistic {
            pairs: self.pairs,
            mean,
            variance,
            ci95_half_width: 1.96 * (variance / n).sqrt(),
            rounds: self.rounds,
            log_e_h0: self.log_e_h0,
            log_e_h1: self.log_e_h1,
        }
    }

    /// Return a decision only at the end of a complete round of maps.
    pub fn decision(&self) -> Option<SprtDecision> {
        if self.pairs < SPRT_MINIMUM_PAIRS || !self.pairs.is_multiple_of(self.maps_per_round) {
            return None;
        }
        let (against_h0, against_h1) = self.bounds.limits();
        if self.log_e_h0 >= against_h0 {
            Some(SprtDecision::AcceptH1)
        } else if self.log_e_h1 >= against_h1 {
            Some(SprtDecision::AcceptH0)
        } else {
            None
        }
    }
}

/// One played game of a pair.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SprtGame {
    pub seat_order: SeatOrderVariant,
    /// Candidate points: one for a win, half for a draw, zero for a loss.
    pub candidate_points: f64,
    pub outcome: String,
    /// The ruleset reason. Older results can omit this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome_reason: Option<String>,
    pub days: u32,
    pub commands: u64,
    pub candidate_invalid_commands: u64,
    pub baseline_invalid_commands: u64,
    /// Candidate planner counters for this game.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_planner_stats: Option<PlannerStats>,
    /// Baseline planner counters for this game.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_planner_stats: Option<PlannerStats>,
}

/// One played pair.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SprtPair {
    pub pair_index: usize,
    pub map_id: u32,
    pub match_seed: u64,
    pub games: [SprtGame; 2],
    pub differential: f64,
    pub statistic_after: SprtStatistic,
}

/// The mean differential on one map.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SprtMapSummary {
    pub pairs: usize,
    pub mean: f64,
}

/// The result of one SPRT run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SprtResult {
    /// The agent random stream assignment used by this run.
    #[serde(default, skip_serializing_if = "is_role_seeded")]
    pub agent_seed_protocol: AgentSeedProtocol,
    pub schema_version: u16,
    pub run_id: String,
    pub plan: SprtPlan,
    /// The source state recorded by the plan runner. Older results and
    /// direct library runs can omit it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceProvenance>,
    /// Fingerprint of the exact plan, including map and commander choices.
    #[serde(default)]
    pub plan_fingerprint: String,
    pub map_fingerprints: BTreeMap<u32, String>,
    pub candidate: AgentIdentity,
    pub baseline: AgentIdentity,
    pub bounds: SprtBounds,
    pub method: String,
    pub log_e_h0_threshold: f64,
    pub log_e_h1_threshold: f64,
    pub decision: SprtDecision,
    pub statistic: SprtStatistic,
    pub maps: BTreeMap<u32, SprtMapSummary>,
    pub candidate_complete_turn_timing: CompleteTurnTiming,
    pub baseline_complete_turn_timing: CompleteTurnTiming,
    pub candidate_invalid_commands: u64,
    pub baseline_invalid_commands: u64,
    pub jobs: usize,
    pub wall_clock_nanos: u64,
    pub pairs: Vec<SprtPair>,
}

/// Errors from an SPRT run.
#[derive(Debug, thiserror::Error)]
pub enum SprtError {
    #[error("plan error: {0}")]
    Plan(#[from] PlanError),
    #[error("map registry error: {0}")]
    Map(#[from] crate::map_registry::MapRegistryError),
    #[error("match execution error: {0}")]
    Execute(#[from] awvm::transition::ExecuteError),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("configuration error: {0}")]
    Configuration(String),
}

/// Read an SPRT plan.
pub fn read_sprt_plan(path: impl AsRef<Path>) -> Result<SprtPlan, SprtError> {
    let plan: SprtPlan = serde_json::from_slice(&fs::read(path)?)?;
    if plan.schema_version != SPRT_PLAN_SCHEMA_VERSION {
        return Err(SprtError::Configuration(format!(
            "SPRT plan schema {} is not supported; expected {SPRT_PLAN_SCHEMA_VERSION}",
            plan.schema_version
        )));
    }
    Ok(plan)
}

/// Run an SPRT plan and write `sprt-result.json` to `output`.
pub fn run_sprt_plan(
    plan_path: impl AsRef<Path>,
    output: impl AsRef<Path>,
    jobs: usize,
    progress: impl FnMut(&SprtPair, &SprtStatistic),
) -> Result<SprtResult, SprtError> {
    let plan_path = plan_path.as_ref();
    let plan = read_sprt_plan(plan_path)?;
    let registry = match &plan.map_manifest {
        Some(manifest) => {
            let path = plan_path
                .parent()
                .map_or_else(|| manifest.clone(), |parent| parent.join(manifest));
            MapRegistry::load(&MapManifest::read(path)?)?
        }
        None => MapRegistry::load_checked_in()?,
    };
    validate_commander_assignments(&plan, &registry)?;
    let (candidate, _) = plan.candidate.materialize(plan_path)?;
    let (baseline, _) = plan.baseline.materialize(plan_path)?;
    let source = source_provenance(plan_path)?;
    let mut result = run_sprt(&plan, &registry, &*candidate, &*baseline, jobs, progress)?;
    result.source = Some(source);
    let output = output.as_ref();
    fs::create_dir_all(output)?;
    fs::write(
        output.join("sprt-result.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    Ok(result)
}

/// Run an SPRT with `jobs` workers.
pub fn run_sprt(
    plan: &SprtPlan,
    registry: &MapRegistry,
    candidate: &dyn AgentFactory,
    baseline: &dyn AgentFactory,
    jobs: usize,
    mut progress: impl FnMut(&SprtPair, &SprtStatistic),
) -> Result<SprtResult, SprtError> {
    plan.bounds.validate().map_err(SprtError::Configuration)?;
    if plan.limits.day_limit == 0 || plan.limits.refusal_limit == 0 {
        return Err(SprtError::Configuration(
            "SPRT day and refusal limits must be positive".into(),
        ));
    }
    if plan.maps.is_empty() || plan.max_pairs == 0 {
        return Err(SprtError::Configuration(
            "an SPRT plan needs at least one map and one pair".into(),
        ));
    }
    if plan
        .maps
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        != plan.maps.len()
    {
        return Err(SprtError::Configuration(
            "SPRT maps must be distinct".into(),
        ));
    }
    let maps = plan
        .maps
        .iter()
        .map(|id| {
            registry
                .get(*id)
                .ok_or_else(|| SprtError::Configuration(format!("map {id} is not loaded")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_commander_maps(plan, &maps)?;
    let limits = Limits {
        nodes: NodeBudget::new(plan.limits.node_budget)
            .ok_or_else(|| SprtError::Configuration("node budget must be nonzero".into()))?,
        days: plan.limits.day_limit,
        refusals: plan.limits.refusal_limit,
    };
    let started = Instant::now();
    let play = |pair_index: usize| {
        play_pair(
            plan,
            maps[pair_index % maps.len()],
            pair_index,
            candidate,
            baseline,
            limits,
        )
    };

    let jobs = jobs.max(1);
    let mut accumulator = SprtAccumulator::new(
        plan.bounds,
        std::num::NonZeroUsize::new(maps.len()).expect("maps are nonempty"),
    )?;
    let mut pairs = Vec::new();
    let mut candidate_turns = Vec::new();
    let mut baseline_turns = Vec::new();
    let mut decision = None;
    let mut commit = |output: PairOutput| -> Result<bool, SprtError> {
        let PairOutput {
            mut pair,
            candidate_turn_nanos,
            baseline_turn_nanos,
        } = output;
        accumulator.push(pair.differential)?;
        let statistic = accumulator.statistic();
        pair.statistic_after = statistic;
        progress(&pair, &statistic);
        pairs.push(pair);
        candidate_turns.extend(candidate_turn_nanos);
        baseline_turns.extend(baseline_turn_nanos);
        if accumulator.pairs.is_multiple_of(accumulator.maps_per_round) {
            decision = accumulator.decision();
        }
        Ok((plan.stop_early && decision.is_some()) || pairs.len() >= plan.max_pairs)
    };

    crate::workers::run_ordered(plan.max_pairs, jobs, play, |_, output| commit(output?))?;

    let statistic = accumulator.statistic();
    let mut by_map = BTreeMap::<u32, (usize, f64)>::new();
    for pair in &pairs {
        let entry = by_map.entry(pair.map_id).or_default();
        entry.0 += 1;
        entry.1 += pair.differential;
    }
    let (log_e_h0_threshold, log_e_h1_threshold) = plan.bounds.limits();
    let plan_fingerprint = fingerprint_bytes(&serde_json::to_vec(plan)?);
    let invalid = |candidate: bool| {
        pairs
            .iter()
            .flat_map(|pair| pair.games.iter())
            .map(|game| {
                if candidate {
                    game.candidate_invalid_commands
                } else {
                    game.baseline_invalid_commands
                }
            })
            .sum()
    };
    Ok(SprtResult {
        agent_seed_protocol: plan.agent_seed_protocol,
        schema_version: SPRT_RESULT_SCHEMA_VERSION,
        run_id: plan.run_id.clone(),
        plan: plan.clone(),
        source: None,
        plan_fingerprint,
        map_fingerprints: maps
            .iter()
            .map(|map| (map.id, map.normalized_fingerprint.clone()))
            .collect(),
        candidate: candidate.identity().clone(),
        baseline: baseline.identity().clone(),
        bounds: plan.bounds,
        method: "bounded-betting-map-round-v1".into(),
        log_e_h0_threshold,
        log_e_h1_threshold,
        decision: decision.unwrap_or(SprtDecision::Inconclusive),
        statistic,
        maps: by_map
            .into_iter()
            .map(|(map, (pairs, sum))| {
                (
                    map,
                    SprtMapSummary {
                        pairs,
                        mean: sum / pairs as f64,
                    },
                )
            })
            .collect(),
        candidate_complete_turn_timing: summarize_complete_turns(candidate_turns),
        baseline_complete_turn_timing: summarize_complete_turns(baseline_turns),
        candidate_invalid_commands: invalid(true),
        baseline_invalid_commands: invalid(false),
        jobs,
        wall_clock_nanos: started.elapsed().as_nanos().try_into().unwrap_or(u64::MAX),
        pairs,
    })
}

struct PairOutput {
    pair: SprtPair,
    candidate_turn_nanos: Vec<u64>,
    baseline_turn_nanos: Vec<u64>,
}

fn validate_commander_assignments(
    plan: &SprtPlan,
    registry: &MapRegistry,
) -> Result<(), SprtError> {
    if plan.commanders.is_none() {
        return Ok(());
    }
    let maps = plan
        .maps
        .iter()
        .map(|id| {
            registry
                .get(*id)
                .ok_or_else(|| SprtError::Configuration(format!("map {id} is not loaded")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_commander_maps(plan, &maps)
}

fn validate_commander_maps(plan: &SprtPlan, maps: &[&RegisteredMap]) -> Result<(), SprtError> {
    if plan.commanders.is_none() {
        return Ok(());
    }
    for map in maps {
        let state = map.state(plan.run_seed)?;
        if state.players.seats().count() != 2 {
            return Err(SprtError::Configuration(format!(
                "commander assignments need two playable seats on map {}",
                map.id
            )));
        }
    }
    Ok(())
}

fn assign_commanders(state: &mut State, commanders: [CommanderKind; 2]) -> Result<(), SprtError> {
    let seats = state
        .players
        .seats()
        .map(|(seat, _)| seat)
        .collect::<Vec<_>>();
    if seats.len() != commanders.len() {
        return Err(SprtError::Configuration(
            "commander assignments need two playable seats".into(),
        ));
    }
    for (seat, id) in seats.into_iter().zip(commanders) {
        state.player_mut(seat).commanders = vec![awvm::semantic::Commander {
            id,
            active: true,
            power_charge: 0,
            power_uses: 0,
        }];
    }
    Ok(())
}

fn play_pair(
    plan: &SprtPlan,
    map: &RegisteredMap,
    pair_index: usize,
    candidate: &dyn AgentFactory,
    baseline: &dyn AgentFactory,
    limits: Limits,
) -> Result<PairOutput, SprtError> {
    let match_seed = Rng::mix(plan.run_seed ^ (u64::from(map.id) << 32) ^ pair_index as u64);
    let mut candidate_turn_nanos = Vec::new();
    let mut baseline_turn_nanos = Vec::new();
    let mut games = Vec::with_capacity(2);
    for seat_order in SeatOrderVariant::ALL {
        let mut state = map.state(match_seed)?;
        if let Some(fog) = plan.fog {
            state.settings.fog = fog;
        }
        if let Some(commanders) = plan.commanders {
            assign_commanders(&mut state, commanders)?;
        }
        let mut session = Session::new(state.clone());
        let mut entropy = Rng::from_seed(BaselineConfig::LOCKED.entropy_seed(match_seed));
        let candidate_seat = match seat_order {
            SeatOrderVariant::AgentFirst => 0,
            SeatOrderVariant::BaselineFirst => 1,
        };
        let (candidate_seed, baseline_seed) = match_agent_seeds(
            plan.agent_seed_protocol,
            match_seed,
            candidate_seat,
            1 - candidate_seat,
        );
        let mut candidate_agent = candidate.create(candidate_seed);
        let mut baseline_agent = baseline.create(baseline_seed);
        let (candidate_seat, mut agents): (usize, [&mut dyn Agent; 2]) = match seat_order {
            SeatOrderVariant::AgentFirst => (0, [&mut *candidate_agent, &mut *baseline_agent]),
            SeatOrderVariant::BaselineFirst => (1, [&mut *baseline_agent, &mut *candidate_agent]),
        };
        let record = play_measured(state, &mut session, &mut agents, &mut entropy, limits)?;
        let baseline_seat = 1 - candidate_seat;
        let by_seat = |values: &[u64], seat: usize| values.get(seat).copied().unwrap_or(0);
        let invalid = |seat: usize| {
            by_seat(&record.refusals_by_seat, seat)
                + by_seat(&record.preflight_rejections_by_seat, seat)
                + by_seat(&record.unrealizable_plays_by_seat, seat)
        };
        let times = |seat: usize| {
            record
                .complete_turn_times_by_seat
                .get(seat)
                .cloned()
                .unwrap_or_default()
        };
        candidate_turn_nanos.extend(times(candidate_seat));
        baseline_turn_nanos.extend(times(baseline_seat));
        let final_state = session.state();
        if invalid(candidate_seat) > 0 || invalid(baseline_seat) > 0 {
            return Err(SprtError::Configuration(
                "invalid agent commands cannot enter the sequential test".into(),
            ));
        }
        let points = match candidate_points(final_state, candidate_seat) {
            Some(points) => points,
            None if matches!(final_state.match_state, Match::Active { .. })
                && record.days > limits.days =>
            {
                0.5
            }
            None => {
                return Err(SprtError::Configuration(
                    "the game has no scorable outcome".into(),
                ));
            }
        };
        games.push(SprtGame {
            seat_order,
            candidate_points: points,
            outcome: outcome_name(final_state).into(),
            outcome_reason: outcome_reason(final_state),
            days: record.days,
            commands: record.commands,
            candidate_invalid_commands: invalid(candidate_seat),
            baseline_invalid_commands: invalid(baseline_seat),
            candidate_planner_stats: candidate_agent.planner_stats(),
            baseline_planner_stats: baseline_agent.planner_stats(),
        });
    }
    let games: [SprtGame; 2] = games.try_into().expect("a pair has two games");
    let differential = games[0].candidate_points + games[1].candidate_points - 1.0;
    Ok(PairOutput {
        pair: SprtPair {
            pair_index,
            map_id: map.id,
            match_seed,
            games,
            differential,
            statistic_after: SprtStatistic::default(),
        },
        candidate_turn_nanos,
        baseline_turn_nanos,
    })
}

/// Return the candidate points from a final state.
///
/// A game that did not finish gives `None`. Only a clean day-limit exit
/// may be scored as a draw by the caller.
fn candidate_points(state: &State, candidate_seat: usize) -> Option<f64> {
    let Match::Finished { outcome } = &state.match_state else {
        return None;
    };
    let team = state
        .players
        .seats()
        .find(|(seat, _)| seat.get() == candidate_seat)
        .map(|(_, player)| &player.team)?;
    match outcome {
        Outcome::Victory { winners, .. } => Some(if winners.contains(team) { 1.0 } else { 0.0 }),
        Outcome::Draw { .. } => Some(0.5),
        Outcome::Cancelled { .. } => None,
    }
}

fn outcome_name(state: &State) -> &'static str {
    match &state.match_state {
        Match::Finished {
            outcome: Outcome::Victory { .. },
        } => "victory",
        Match::Finished {
            outcome: Outcome::Draw { .. },
        } => "draw",
        Match::Finished {
            outcome: Outcome::Cancelled { .. },
        } => "cancelled",
        _ => "incomplete",
    }
}

fn outcome_reason(state: &State) -> Option<String> {
    let Match::Finished { outcome } = &state.match_state else {
        return None;
    };
    serde_json::to_value(outcome)
        .ok()?
        .get("reason")?
        .as_str()
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use awbrn_ai::agent::Play;
    use awvm::semantic::{Observation, ObservedPlayer};

    #[test]
    fn a_clear_gain_accepts_h1_and_a_clear_loss_accepts_h0() {
        let mut gain = SprtAccumulator::default();
        let mut loss = SprtAccumulator::default();
        for index in 0..400 {
            // A spread of results with mean +0.25 and mean -0.25.
            let noise = [0.5, 0.0, 0.5, 0.0][index % 4];
            gain.push(noise).unwrap();
            loss.push(-noise).unwrap();
            if gain.decision().is_some() && loss.decision().is_some() {
                break;
            }
        }
        assert_eq!(gain.decision(), Some(SprtDecision::AcceptH1));
        assert_eq!(loss.decision(), Some(SprtDecision::AcceptH0));
    }

    #[test]
    fn no_decision_comes_before_the_minimum_pair_count() {
        let mut accumulator = SprtAccumulator::default();
        for _ in 0..SPRT_MINIMUM_PAIRS - 1 {
            accumulator.push(1.0).unwrap();
        }
        assert_eq!(accumulator.decision(), None);
        for _ in 0..32 {
            accumulator.push(1.0).unwrap();
        }
        assert_eq!(accumulator.decision(), Some(SprtDecision::AcceptH1));
    }

    #[test]
    fn the_statistic_reports_the_sample_mean_and_variance() {
        let mut accumulator = SprtAccumulator::default();
        for value in [1.0, -1.0, 0.5, -0.5] {
            accumulator.push(value).unwrap();
        }
        let statistic = accumulator.statistic();
        assert_eq!(statistic.pairs, 4);
        assert!(statistic.mean.abs() < 1e-12);
        assert!((statistic.variance - 2.5 / 3.0).abs() < 1e-12);
    }
    #[test]
    fn rare_wins_do_not_trigger_the_old_false_rejection() {
        let mut accumulator = SprtAccumulator::default();
        for _ in 0..24 {
            accumulator.push(0.0).unwrap();
        }
        assert_eq!(accumulator.decision(), None);
        let mut zeros = 24;
        while accumulator.decision().is_none() && zeros < 1000 {
            accumulator.push(0.0).unwrap();
            zeros += 1;
        }
        assert_eq!(accumulator.decision(), Some(SprtDecision::AcceptH0));
        assert!(0.95_f64.powi(zeros) <= SprtBounds::DEFAULT.beta);
    }

    #[test]
    fn evidence_uses_complete_map_rounds() {
        let mut accumulator =
            SprtAccumulator::new(SprtBounds::DEFAULT, std::num::NonZeroUsize::new(3).unwrap())
                .unwrap();
        for _ in 0..9 {
            accumulator.push(1.0).unwrap();
            accumulator.push(0.0).unwrap();
            let before = accumulator.statistic();
            assert_eq!(accumulator.decision(), None);
            accumulator.push(-1.0).unwrap();
            let after = accumulator.statistic();
            assert_eq!(after.rounds, before.rounds + 1);
            assert_eq!(after.log_e_h0, 0.0);
        }
        assert_eq!(accumulator.statistic().mean, 0.0);
    }

    #[test]
    fn invalid_differentials_do_not_change_the_accumulator() {
        let mut accumulator = SprtAccumulator::default();
        for value in [f64::NAN, f64::INFINITY, -1.1, 1.1] {
            accumulator.push(value).unwrap_err();
        }
        assert_eq!(accumulator.statistic(), SprtStatistic::default());
    }

    #[test]
    fn hypotheses_must_fit_the_outcome_range() {
        for (h0, h1) in [(-1.1, 0.05), (0.0, 1.0), (0.1, 0.05), (f64::NAN, 0.05)] {
            SprtAccumulator::new(
                SprtBounds {
                    h0,
                    h1,
                    ..SprtBounds::DEFAULT
                },
                std::num::NonZeroUsize::MIN,
            )
            .unwrap_err();
        }
    }

    #[test]
    fn zero_limits_are_rejected_before_agents_are_created() {
        let registry = MapRegistry::load_checked_in().unwrap();
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/ai-diagnostics/sprt/hard-vs-hard-null.json");
        let plan = read_sprt_plan(path).unwrap();
        let candidate = SeedRecordingFactory::new();
        let baseline = SeedRecordingFactory::new();
        for day_limit in [true, false] {
            let mut invalid = plan.clone();
            if day_limit {
                invalid.limits.day_limit = 0;
            } else {
                invalid.limits.refusal_limit = 0;
            }
            run_sprt(&invalid, &registry, &candidate, &baseline, 2, |_, _| {}).unwrap_err();
        }
        assert!(candidate.seeds.lock().unwrap().is_empty());
        assert!(baseline.seeds.lock().unwrap().is_empty());
    }

    #[test]
    fn fixed_runs_and_early_stops_are_the_same_for_each_worker_count() {
        let registry = MapRegistry::load_checked_in().unwrap();
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/ai-diagnostics/sprt/hard-vs-hard-null.json");
        let mut plan = read_sprt_plan(path).unwrap();
        plan.maps = vec![61748];
        plan.max_pairs = 17;
        plan.limits.day_limit = 1;
        plan.agent_seed_protocol = AgentSeedProtocol::SeatSeeded;
        plan.bounds = SprtBounds {
            h0: -0.9,
            h1: -0.5,
            ..SprtBounds::DEFAULT
        };
        for stop_early in [true, false] {
            plan.stop_early = stop_early;
            let mut serial = None;
            for jobs in [1, 2] {
                let candidate = SeedRecordingFactory::new();
                let baseline = SeedRecordingFactory::new();
                let result =
                    run_sprt(&plan, &registry, &candidate, &baseline, jobs, |_, _| {}).unwrap();
                assert_eq!(result.decision, SprtDecision::AcceptH1);
                assert_eq!(result.pairs.len(), if stop_early { 16 } else { 17 });
                if let Some(serial) = &serial {
                    assert_eq!(&result.pairs, serial);
                } else {
                    serial = Some(result.pairs);
                }
            }
        }
    }

    #[test]
    fn a_partial_final_round_keeps_the_last_complete_round_decision() {
        let registry = MapRegistry::load_checked_in().unwrap();
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/ai-diagnostics/sprt/hard-vs-hard-null.json");
        let mut plan = read_sprt_plan(path).unwrap();
        plan.maps = vec![61748, 67945];
        plan.limits.day_limit = 1;
        plan.stop_early = false;
        plan.bounds = SprtBounds {
            h0: -0.99,
            h1: -0.95,
            ..SprtBounds::DEFAULT
        };
        let factory = SeedRecordingFactory::new();
        for jobs in [1, 2] {
            plan.max_pairs = 32;
            let complete = run_sprt(&plan, &registry, &factory, &factory, jobs, |_, _| {}).unwrap();
            plan.max_pairs = 33;
            let partial = run_sprt(&plan, &registry, &factory, &factory, jobs, |_, _| {}).unwrap();
            assert_eq!(complete.decision, SprtDecision::AcceptH1);
            assert_eq!(partial.decision, complete.decision);
            assert_eq!(partial.statistic.rounds, complete.statistic.rounds);
            assert_eq!(partial.statistic.log_e_h0, complete.statistic.log_e_h0);
            assert_eq!(partial.statistic.log_e_h1, complete.statistic.log_e_h1);
        }
    }

    #[test]
    fn results_record_source_state_and_planner_counters() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/ai-diagnostics/sprt/planner-v3-seat-seeded-smoke.json");
        let output = tempfile::tempdir().unwrap();
        let result = run_sprt_plan(&path, output.path(), 2, |_, _| {}).unwrap();
        let expected = source_provenance(&path).unwrap();
        assert_eq!(result.source.as_ref(), Some(&expected));
        for game in result.pairs.iter().flat_map(|pair| &pair.games) {
            let stats = game.candidate_planner_stats.as_ref().unwrap();
            assert!(stats.plans > 0);
            assert!(stats.evaluations > 0);
            assert!(game.baseline_planner_stats.is_none());
        }
        let mut old = serde_json::to_value(&result).unwrap();
        old.as_object_mut().unwrap().remove("source");
        old.as_object_mut().unwrap().remove("plan_fingerprint");
        old["schema_version"] = serde_json::json!(2);
        for pair in old["pairs"].as_array_mut().unwrap() {
            for game in pair["games"].as_array_mut().unwrap() {
                game.as_object_mut()
                    .unwrap()
                    .remove("candidate_planner_stats");
            }
        }
        let old: SprtResult = serde_json::from_value(old).unwrap();
        assert!(old.source.is_none());
        assert!(
            old.pairs
                .iter()
                .flat_map(|pair| &pair.games)
                .all(|game| game.candidate_planner_stats.is_none())
        );
    }

    #[derive(Debug)]
    struct CommanderRecordingFactory {
        identity: AgentIdentity,
        observations: std::sync::Arc<std::sync::Mutex<Vec<(String, CommanderKind)>>>,
    }

    impl CommanderRecordingFactory {
        fn new(
            observations: std::sync::Arc<std::sync::Mutex<Vec<(String, CommanderKind)>>>,
        ) -> Self {
            Self {
                identity: AgentIdentity {
                    identifier: "commander-recording-agent".into(),
                    configuration_fingerprint: "commander-recording-agent-v1".into(),
                    executable_fingerprint: "commander-recording-agent-v1".into(),
                },
                observations,
            }
        }
    }

    impl AgentFactory for CommanderRecordingFactory {
        fn identity(&self) -> &AgentIdentity {
            &self.identity
        }

        fn create(&self, _seed: u64) -> Box<dyn Agent> {
            Box::new(CommanderRecordingAgent {
                observations: std::sync::Arc::clone(&self.observations),
            })
        }
    }

    struct CommanderRecordingAgent {
        observations: std::sync::Arc<std::sync::Mutex<Vec<(String, CommanderKind)>>>,
    }

    impl Agent for CommanderRecordingAgent {
        fn act(&mut self, view: &Observation, _budget: NodeBudget) -> Option<Play> {
            let commander = view.players.iter().find_map(|player| match player {
                ObservedPlayer::Private { id, commanders, .. } if id == &view.recipient => {
                    commanders.first().map(|commander| commander.id)
                }
                _ => None,
            });
            if let Some(commander) = commander {
                self.observations
                    .lock()
                    .unwrap()
                    .push((view.recipient.as_str().to_owned(), commander));
            }
            None
        }
    }

    #[test]
    fn assigned_commanders_reach_each_physical_seat_and_change_the_plan_fingerprint() {
        use awvm::ruleset::CommanderKind::{Andy, Drake};

        let registry = MapRegistry::load_checked_in().unwrap();
        let map = registry.get(61748).unwrap();
        let initial = map.state(7001).unwrap();
        let expected = initial
            .players
            .seats()
            .zip([Andy, Drake])
            .map(|((_, player), commander)| (player.id().as_str().to_owned(), commander))
            .collect::<BTreeMap<_, _>>();
        let observations = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let factory = CommanderRecordingFactory::new(std::sync::Arc::clone(&observations));
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/ai-diagnostics/sprt/planner-v3-seat-seeded-smoke.json");
        let mut plan = read_sprt_plan(path).unwrap();
        plan.maps = vec![61748];
        plan.commanders = Some([Andy, Drake]);
        plan.run_seed = 7001;
        plan.max_pairs = 1;
        plan.stop_early = false;
        plan.limits = RunLimits {
            day_limit: 1,
            node_budget: 1,
            refusal_limit: 8,
        };

        let result = run_sprt(&plan, &registry, &factory, &factory, 1, |_, _| {}).unwrap();
        let expected_fingerprint = fingerprint_bytes(&serde_json::to_vec(&plan).unwrap());
        assert_eq!(result.plan_fingerprint, expected_fingerprint);
        let assigned = observations.lock().unwrap();
        assert!(!assigned.is_empty());
        for (player, commander) in assigned.iter() {
            assert_eq!(expected.get(player), Some(commander));
        }
        for player in expected.keys() {
            assert!(assigned.iter().any(|(observed, _)| observed == player));
        }
        drop(assigned);

        let mut swapped = plan.clone();
        swapped.commanders = Some([Drake, Andy]);
        assert_ne!(
            result.plan_fingerprint,
            fingerprint_bytes(&serde_json::to_vec(&swapped).unwrap())
        );
    }

    #[derive(Debug)]
    struct SeedRecordingFactory {
        inner: crate::tournament::AiProfileFactory,
        seeds: std::sync::Mutex<Vec<u64>>,
    }

    impl SeedRecordingFactory {
        fn new() -> Self {
            Self {
                inner: crate::tournament::AiProfileFactory::new("ai-hard-v2").unwrap(),
                seeds: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    impl AgentFactory for SeedRecordingFactory {
        fn identity(&self) -> &AgentIdentity {
            self.inner.identity()
        }

        fn create(&self, seed: u64) -> Box<dyn Agent> {
            self.seeds.lock().unwrap().push(seed);
            self.inner.create(seed)
        }
    }

    #[test]
    fn old_sprt_plans_keep_role_seeded_json() {
        let directory =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/ai-diagnostics/sprt");
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            let input: serde_json::Value =
                serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            if input.get("agent_seed_protocol").is_some() {
                continue;
            }
            let plan = read_sprt_plan(path).unwrap();
            assert_eq!(plan.agent_seed_protocol, AgentSeedProtocol::RoleSeeded);
            assert!(
                serde_json::to_value(plan)
                    .unwrap()
                    .get("agent_seed_protocol")
                    .is_none()
            );
        }
    }

    #[test]
    fn seed_protocol_is_used_by_serial_and_parallel_sprt() {
        let registry = MapRegistry::load_checked_in().unwrap();
        for protocol in [AgentSeedProtocol::RoleSeeded, AgentSeedProtocol::SeatSeeded] {
            let plan: SprtPlan = serde_json::from_value(serde_json::json!({
                "schema_version": 1,
                "run_id": "seed-protocol-test",
                "candidate": {"kind": "ai-profile", "profile_id": "ai-hard-v2"},
                "baseline": {"kind": "ai-profile", "profile_id": "ai-hard-v2"},
                "maps": [61748],
                "run_seed": 91,
                "agent_seed_protocol": protocol,
                "max_pairs": 2,
                "limits": {"day_limit": 2, "node_budget": 1, "refusal_limit": 8}
            }))
            .unwrap();
            let mut serial_pairs = None;
            for jobs in [1, 2] {
                let candidate = SeedRecordingFactory::new();
                let baseline = SeedRecordingFactory::new();
                let result =
                    run_sprt(&plan, &registry, &candidate, &baseline, jobs, |_, _| {}).unwrap();
                let mut expected_candidate = Vec::new();
                let mut expected_baseline = Vec::new();
                for pair in &result.pairs {
                    for candidate_seat in [0, 1] {
                        let candidate_slot = match protocol {
                            AgentSeedProtocol::RoleSeeded => 0,
                            AgentSeedProtocol::SeatSeeded => candidate_seat,
                        };
                        expected_candidate.push(
                            BaselineConfig::LOCKED.agent_seed(pair.match_seed, candidate_slot),
                        );
                        expected_baseline.push(
                            BaselineConfig::LOCKED.agent_seed(pair.match_seed, 1 - candidate_slot),
                        );
                    }
                }
                let mut actual_candidate = candidate.seeds.into_inner().unwrap();
                let mut actual_baseline = baseline.seeds.into_inner().unwrap();
                actual_candidate.sort_unstable();
                actual_baseline.sort_unstable();
                expected_candidate.sort_unstable();
                expected_baseline.sort_unstable();
                assert_eq!(actual_candidate, expected_candidate);
                assert_eq!(actual_baseline, expected_baseline);
                assert_eq!(result.agent_seed_protocol, protocol);
                let encoded = serde_json::to_value(&result).unwrap();
                if protocol == AgentSeedProtocol::SeatSeeded {
                    assert_eq!(encoded["agent_seed_protocol"], "seat-seeded");
                } else {
                    assert!(encoded.get("agent_seed_protocol").is_none());
                }
                let restored: SprtResult = serde_json::from_value(encoded).unwrap();
                assert_eq!(restored.agent_seed_protocol, protocol);
                if let Some(serial) = &serial_pairs {
                    assert_eq!(&result.pairs, serial);
                } else {
                    serial_pairs = Some(result.pairs);
                }
            }
        }
    }
}
