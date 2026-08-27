//! Measure disagreement and counterfactual coverage in the script portfolio.

use anyhow::Result;
use awbrn_ai::adaptive::{
    ENTROPY_SALT, MAX_TURNS, REPLY_SALT, SelectionPolicy, select as adaptive_select,
};
use awbrn_ai::agent::{Agent, NodeBudget, Play};
use awbrn_ai::agents::{
    CaptureMission, CaptureMissionState, GreedyAgent, MissionBook, Script, StratifiedScripts,
    Stratum, Weights, generate_plan, generate_plans, generate_stratum_candidates,
};
use awbrn_ai::board::arena;
use awbrn_ai::eval::{EvalBreakdown, EvalWeights, Evaluator};
use awbrn_ai::harness::{Limits, play_measured, play_observed};
use awbrn_ai::rng::Rng;
use awvm::semantic::{
    AwbwVisibility, Match, Observation, Outcome, State, TeamId, observe, observe_into,
};
use awvm::session::Session;
use awvm::transition::Command;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

const DEFAULT_GAMES: usize = 20;
const DEFAULT_ROOTS: usize = 50;
const DEFAULT_DAYS: u32 = 35;
const STRATIFIED_CANDIDATE_LIMIT: usize = NodeBudget::SIXTEEN.get() as usize;

fn main() {
    let options = match Options::parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}\n\n{USAGE}");
            std::process::exit(2);
        }
    };
    let result = if options.horizon_audit {
        run_horizon_audit(&options)
    } else if options.exposure_sweep {
        run_exposure_sweep(&options)
    } else if options.adaptive_horizon {
        run_adaptive_horizon(&options)
    } else if options.stratified_arena {
        run_stratified_arena(&options)
    } else if options.horizon_sweep {
        run_horizon_sweep(&options)
    } else {
        run(&options)
    };
    if let Err(error) = result {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

const USAGE: &str = "\
usage: portfolio-diagnostic [--seed N] [--games N] [--roots N] [--days N]
                            [--horizon-sweep] [--horizon-audit] [--exposure-sweep]
                            [--adaptive-horizon] [--stratified-arena]

  --seed N       Run seed. Default 101.
  --games N      Paired self-play games used to sample roots. Default 20.
  --roots N      Root positions to measure. Default 50.
  --days N       Counterfactual day cap. Default 35.
  --horizon-sweep Measure horizon selection over deduplicated stratified plans.
  --horizon-audit Audit four-round errors, fit terms, and validate on a fresh seed.
  --exposure-sweep Compare fixed exposure/front multipliers at two and four rounds.
  --adaptive-horizon Use evaluator disagreement to extend only uncertain roots.
  --stratified-arena Compare standard four-round and adaptive stratified agents against baseline.
  --validation-seed N  Seed for held-out horizon validation. Default is derived.
";

#[derive(Clone)]
struct Options {
    seed: u64,
    games: usize,
    roots: usize,
    days: u32,
    horizon_sweep: bool,
    horizon_audit: bool,
    exposure_sweep: bool,
    adaptive_horizon: bool,
    stratified_arena: bool,
    validation_seed: u64,
}

impl Options {
    fn parse(arguments: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut options = Self {
            seed: 101,
            games: DEFAULT_GAMES,
            roots: DEFAULT_ROOTS,
            days: DEFAULT_DAYS,
            horizon_sweep: false,
            horizon_audit: false,
            exposure_sweep: false,
            adaptive_horizon: false,
            stratified_arena: false,
            validation_seed: 0,
        };
        let mut validation_seed = None;
        let mut arguments = arguments.peekable();
        while let Some(argument) = arguments.next() {
            let mut value = || {
                arguments
                    .next()
                    .ok_or_else(|| format!("{argument} needs a value"))
            };
            match argument.as_str() {
                "--seed" => options.seed = number(&value()?)?,
                "--games" => options.games = number(&value()?)?,
                "--roots" => options.roots = number(&value()?)?,
                "--days" => options.days = number(&value()?)?,
                "--horizon-sweep" => options.horizon_sweep = true,
                "--horizon-audit" => options.horizon_audit = true,
                "--exposure-sweep" => options.exposure_sweep = true,
                "--adaptive-horizon" => options.adaptive_horizon = true,
                "--stratified-arena" => options.stratified_arena = true,
                "--validation-seed" => validation_seed = Some(number(&value()?)?),
                "--help" | "-h" => {
                    println!("{USAGE}");
                    std::process::exit(0);
                }
                other => return Err(format!("unknown argument {other}")),
            }
        }
        if options.games == 0 || options.roots == 0 || options.days == 0 {
            return Err("--games, --roots, and --days must be at least 1".to_owned());
        }
        options.validation_seed = validation_seed.unwrap_or(options.seed ^ 0x9e37_79b9_7f4a_7c15);
        Ok(options)
    }
}

fn number<T: std::str::FromStr>(text: &str) -> Result<T, String> {
    text.parse()
        .map_err(|_| format!("{text} is not a number this argument accepts"))
}

#[derive(Clone)]
struct SampledRoot {
    state: State,
    view: Observation,
    seed: u64,
}

#[derive(Clone, Copy)]
struct LineResult {
    leaf_value: Option<f64>,
    result: f64,
    finished: bool,
    turns: u32,
}

#[derive(Clone, Copy, Default)]
struct MissionQuality {
    total: u64,
    preserved: u64,
    completed: u64,
}

impl MissionQuality {
    fn add(&mut self, other: Self) {
        self.total += other.total;
        self.preserved += other.preserved;
        self.completed += other.completed;
    }
}

struct WholeCandidate {
    line: LineResult,
    mission: MissionQuality,
}

struct StratifiedCandidate {
    stratum: Stratum,
    assignment: StratifiedScripts,
    plays: Vec<Play>,
    line: LineResult,
    mission: MissionQuality,
}

struct ScriptCoverage {
    script: Script,
    plans: u64,
    changed_plans: u64,
    order_changes: u64,
    order_slots: u64,
    lines: u64,
    leaf_lines: u64,
    leaf_better: u64,
    result_better: u64,
}

struct StratifiedScriptCoverage {
    stratum: Stratum,
    script: Script,
    generated: u64,
    evaluated: u64,
    duplicate: u64,
    selected: u64,
    leaf_better: u64,
    result_better: u64,
    mission_total: u64,
    mission_preserved: u64,
    capture_completed: u64,
}

#[derive(Clone, Copy)]
enum Horizon {
    Reply,
    OneRound,
    TwoRounds,
    FourRounds,
    EightRounds,
    Terminal,
}

impl Horizon {
    const ALL: [Self; 5] = [
        Self::Reply,
        Self::OneRound,
        Self::TwoRounds,
        Self::FourRounds,
        Self::Terminal,
    ];

    const SELECTION: [Self; 2] = [Self::TwoRounds, Self::FourRounds];

    const fn name(self) -> &'static str {
        match self {
            Self::Reply => "1 reply",
            Self::OneRound => "1 round",
            Self::TwoRounds => "2 rounds",
            Self::FourRounds => "4 rounds",
            Self::EightRounds => "8 rounds",
            Self::Terminal => "terminal",
        }
    }

    const fn turns(self) -> Option<u32> {
        match self {
            Self::Reply => Some(1),
            Self::OneRound => Some(2),
            Self::TwoRounds => Some(4),
            Self::FourRounds => Some(8),
            Self::EightRounds => Some(16),
            Self::Terminal => None,
        }
    }
}

#[derive(Clone, Copy)]
enum ExposureFrontArm {
    Standard,
    Conservative,
    Disabled,
}

#[derive(Clone, Copy)]
enum StratifiedArenaPolicy {
    StandardFour,
    Adaptive,
}

impl StratifiedArenaPolicy {
    const ALL: [Self; 2] = [Self::StandardFour, Self::Adaptive];

    const fn name(self) -> &'static str {
        match self {
            Self::StandardFour => "standard-4",
            Self::Adaptive => "adaptive",
        }
    }
}

impl ExposureFrontArm {
    const ALL: [Self; 3] = [Self::Standard, Self::Conservative, Self::Disabled];

    const fn name(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Conservative => "conservative",
            Self::Disabled => "disabled",
        }
    }

    const fn multiplier(self) -> f64 {
        match self {
            Self::Standard => 1.0,
            Self::Conservative => 0.25,
            Self::Disabled => 0.0,
        }
    }

    fn weights(self) -> EvalWeights {
        let mut weights = EvalWeights::STANDARD;
        let multiplier = self.multiplier();
        weights.exposure *= multiplier;
        weights.front *= multiplier;
        weights
    }
}

#[derive(Clone)]
struct SweepPlan {
    plays: Vec<Play>,
}

impl AsRef<[Play]> for SweepPlan {
    fn as_ref(&self) -> &[Play] {
        &self.plays
    }
}

#[derive(Clone, Copy)]
struct HorizonLine {
    score: f64,
    terminal_result: Option<f64>,
}

struct HorizonContext<'a> {
    root: &'a State,
    seed: u64,
    days: u32,
    friendly_seat: awvm::semantic::PlayerIdx,
    friendly: &'a awvm::semantic::PlayerId,
    evaluator: &'a mut Evaluator,
}

struct HorizonMetrics {
    evaluated_candidates: u64,
    selection_roots: u64,
    non_tied_roots: u64,
    top_plan_hits: u64,
    selected_improvement_roots: u64,
    oracle_improvement_roots: u64,
    improvements_captured: u64,
    regret: f64,
    pairwise_score: f64,
    pairwise_pairs: u64,
    runtime: Duration,
}

impl HorizonMetrics {
    const fn new() -> Self {
        Self {
            evaluated_candidates: 0,
            selection_roots: 0,
            non_tied_roots: 0,
            top_plan_hits: 0,
            selected_improvement_roots: 0,
            oracle_improvement_roots: 0,
            improvements_captured: 0,
            regret: 0.0,
            pairwise_score: 0.0,
            pairwise_pairs: 0,
            runtime: Duration::ZERO,
        }
    }
}

struct HorizonCoverage {
    roots_sampled: usize,
    roots_measured: u64,
    roots_skipped: u64,
    generated_candidates: u64,
    unique_candidates: u64,
    duplicate_candidates: u64,
    terminal_oracle_improvement_roots: u64,
    horizons: [HorizonMetrics; 5],
}

struct ExposureSweepArm {
    arm: ExposureFrontArm,
    horizons: [HorizonMetrics; 2],
}

impl ExposureSweepArm {
    fn new(arm: ExposureFrontArm) -> Self {
        Self {
            arm,
            horizons: std::array::from_fn(|_| HorizonMetrics::new()),
        }
    }
}

struct ExposureSweepCoverage {
    roots_sampled: usize,
    roots_measured: u64,
    roots_skipped: u64,
    generated_candidates: u64,
    unique_candidates: u64,
    duplicate_candidates: u64,
    terminal_oracle_improvement_roots: u64,
    arms: [ExposureSweepArm; 3],
}

struct StratifiedArenaAgent {
    seed: u64,
    days: u32,
    missions: MissionBook,
    policy: StratifiedArenaPolicy,
    turn: u32,
    plays: Vec<Play>,
    next_play: usize,
    end_turn: bool,
    selection_turns: u64,
    replaced_turns: u64,
    selection_margin: f64,
    replacement_margin: f64,
    planning_runtime: Duration,
    mission_stats: ArenaMissionStats,
}

impl StratifiedArenaAgent {
    fn new(seed: u64, days: u32, policy: StratifiedArenaPolicy) -> Self {
        Self {
            seed,
            days,
            missions: MissionBook::new(),
            policy,
            turn: 0,
            plays: Vec::new(),
            next_play: 0,
            end_turn: false,
            selection_turns: 0,
            replaced_turns: 0,
            selection_margin: 0.0,
            replacement_margin: 0.0,
            planning_runtime: Duration::ZERO,
            mission_stats: ArenaMissionStats::default(),
        }
    }

    fn selection_stats(&self) -> ArenaSelectionStats {
        ArenaSelectionStats {
            selection_turns: self.selection_turns,
            replaced_turns: self.replaced_turns,
            selection_margin: self.selection_margin,
            replacement_margin: self.replacement_margin,
            planning_runtime: self.planning_runtime,
            mission_stats: self.mission_stats,
        }
    }

    fn plan(&mut self, view: &Observation) -> Option<()> {
        let planning_started = Instant::now();
        let state = Session::from_observation(view).ok()?.state().clone();
        let root = SampledRoot {
            state,
            view: view.clone(),
            seed: Rng::mix(self.seed ^ (u64::from(self.turn) << 32)),
        };
        let previous_missions = self.missions.capture_missions().to_vec();
        let generated = generate_arena_sweep_plans(&root, self.days, &mut self.missions)?;
        self.mission_stats
            .add(mission_transitions(&previous_missions, &self.missions));
        let unique = deduplicate_sweep_plans(generated);
        let selection_policy = match self.policy {
            StratifiedArenaPolicy::StandardFour => SelectionPolicy::StandardFour,
            StratifiedArenaPolicy::Adaptive => SelectionPolicy::Adaptive,
        };
        let selection =
            adaptive_select(&root.state, &unique, root.seed, self.days, selection_policy)?;
        let best_index = selection.selected_index;
        let baseline_score = selection.baseline_score;
        let selected_score = selection.selected_score;
        let margin = selected_score - baseline_score;
        self.selection_turns += 1;
        self.selection_margin += margin;
        self.planning_runtime += planning_started.elapsed();
        if best_index != 0 {
            self.replaced_turns += 1;
            self.replacement_margin += margin;
        }
        self.plays = unique[best_index].plays.clone();
        self.next_play = 0;
        self.end_turn = self.plays.is_empty();
        self.turn += 1;
        Some(())
    }
}

#[derive(Clone, Copy, Default)]
struct ArenaSelectionStats {
    selection_turns: u64,
    replaced_turns: u64,
    selection_margin: f64,
    replacement_margin: f64,
    planning_runtime: Duration,
    mission_stats: ArenaMissionStats,
}

#[derive(Clone, Copy, Default)]
struct ArenaMissionStats {
    observed: u64,
    preserved: u64,
    completed: u64,
    suspended: u64,
    invalidated: u64,
}

impl ArenaMissionStats {
    fn add(&mut self, other: Self) {
        self.observed += other.observed;
        self.preserved += other.preserved;
        self.completed += other.completed;
        self.suspended += other.suspended;
        self.invalidated += other.invalidated;
    }
}

impl Agent for StratifiedArenaAgent {
    fn act(&mut self, view: &Observation, _budget: NodeBudget) -> Option<Play> {
        if self.end_turn {
            self.end_turn = false;
            self.plays.clear();
            self.next_play = 0;
            return None;
        }
        if self.next_play == self.plays.len() {
            self.plan(view)?;
        }
        let play = *self.plays.get(self.next_play)?;
        self.next_play += 1;
        if self.next_play == self.plays.len() {
            self.end_turn = true;
        }
        Some(play)
    }
}

struct StratifiedArenaTally {
    policy: StratifiedArenaPolicy,
    wins: u64,
    losses: u64,
    draws: u64,
    abandoned: u64,
    points: f64,
    pairs: u64,
    selection_turns: u64,
    replaced_turns: u64,
    selection_margin: f64,
    replacement_margin: f64,
    replacement_wins: u64,
    replacement_losses: u64,
    replacement_draws: u64,
    replacement_abandoned: u64,
    pair_scores: Vec<f64>,
    game_scores: Vec<f64>,
    game_runtime: Duration,
    planning_runtime: Duration,
    mission_stats: ArenaMissionStats,
}

impl StratifiedArenaTally {
    fn new(policy: StratifiedArenaPolicy) -> Self {
        Self {
            policy,
            wins: 0,
            losses: 0,
            draws: 0,
            abandoned: 0,
            points: 0.0,
            pairs: 0,
            selection_turns: 0,
            replaced_turns: 0,
            selection_margin: 0.0,
            replacement_margin: 0.0,
            replacement_wins: 0,
            replacement_losses: 0,
            replacement_draws: 0,
            replacement_abandoned: 0,
            pair_scores: Vec::new(),
            game_scores: Vec::new(),
            game_runtime: Duration::ZERO,
            planning_runtime: Duration::ZERO,
            mission_stats: ArenaMissionStats::default(),
        }
    }

    fn record_selection(&mut self, stats: ArenaSelectionStats) {
        self.selection_turns += stats.selection_turns;
        self.replaced_turns += stats.replaced_turns;
        self.selection_margin += stats.selection_margin;
        self.replacement_margin += stats.replacement_margin;
        self.planning_runtime += stats.planning_runtime;
        self.mission_stats.add(stats.mission_stats);
    }

    fn record_replacement_result(
        &mut self,
        outcome: Option<&Outcome>,
        team: &TeamId,
        replaced: bool,
    ) {
        if !replaced {
            return;
        }
        match outcome {
            Some(Outcome::Victory { winners, .. }) if winners.contains(team) => {
                self.replacement_wins += 1;
            }
            Some(Outcome::Victory { .. }) => {
                self.replacement_losses += 1;
            }
            Some(Outcome::Draw { .. } | Outcome::Cancelled { .. }) => {
                self.replacement_draws += 1;
            }
            None => {
                self.replacement_abandoned += 1;
            }
        }
    }

    fn record(&mut self, outcome: &Outcome, team: &TeamId) -> f64 {
        let points = match outcome {
            Outcome::Victory { winners, .. } if winners.contains(team) => {
                self.wins += 1;
                1.0
            }
            Outcome::Victory { .. } => {
                self.losses += 1;
                0.0
            }
            Outcome::Draw { .. } | Outcome::Cancelled { .. } => {
                self.draws += 1;
                0.5
            }
        };
        self.points += points;
        points
    }

    fn games(&self) -> u64 {
        self.wins + self.losses + self.draws
    }
}

impl ExposureSweepCoverage {
    fn new(roots_sampled: usize) -> Self {
        Self {
            roots_sampled,
            roots_measured: 0,
            roots_skipped: 0,
            generated_candidates: 0,
            unique_candidates: 0,
            duplicate_candidates: 0,
            terminal_oracle_improvement_roots: 0,
            arms: std::array::from_fn(|index| ExposureSweepArm::new(ExposureFrontArm::ALL[index])),
        }
    }
}

#[derive(Clone, Copy)]
struct AdaptiveLine {
    standard_score: f64,
    conservative_score: f64,
}

struct HorizonReplay {
    state: State,
    terminal_result: Option<f64>,
}

struct AdaptiveMetrics {
    selection_roots: u64,
    non_tied_roots: u64,
    top_plan_hits: u64,
    oracle_improvement_roots: u64,
    improvements_captured: u64,
    regret: f64,
    uncertain_roots: u64,
    four_round_replays: u64,
    eight_round_replays: u64,
    runtime: Duration,
    eight_round_runtime: Duration,
}

impl AdaptiveMetrics {
    const fn new() -> Self {
        Self {
            selection_roots: 0,
            non_tied_roots: 0,
            top_plan_hits: 0,
            oracle_improvement_roots: 0,
            improvements_captured: 0,
            regret: 0.0,
            uncertain_roots: 0,
            four_round_replays: 0,
            eight_round_replays: 0,
            runtime: Duration::ZERO,
            eight_round_runtime: Duration::ZERO,
        }
    }
}

struct AdaptiveCoverage {
    roots_sampled: usize,
    roots_measured: u64,
    roots_skipped: u64,
    generated_candidates: u64,
    unique_candidates: u64,
    duplicate_candidates: u64,
    terminal_oracle_improvement_roots: u64,
    standard_four: HorizonMetrics,
    adaptive: AdaptiveMetrics,
    always_eight: HorizonMetrics,
}

impl AdaptiveCoverage {
    fn new(roots_sampled: usize) -> Self {
        Self {
            roots_sampled,
            roots_measured: 0,
            roots_skipped: 0,
            generated_candidates: 0,
            unique_candidates: 0,
            duplicate_candidates: 0,
            terminal_oracle_improvement_roots: 0,
            standard_four: HorizonMetrics::new(),
            adaptive: AdaptiveMetrics::new(),
            always_eight: HorizonMetrics::new(),
        }
    }
}

impl HorizonCoverage {
    fn new(roots_sampled: usize) -> Self {
        Self {
            roots_sampled,
            roots_measured: 0,
            roots_skipped: 0,
            generated_candidates: 0,
            unique_candidates: 0,
            duplicate_candidates: 0,
            terminal_oracle_improvement_roots: 0,
            horizons: std::array::from_fn(|_| HorizonMetrics::new()),
        }
    }
}

#[derive(Clone, Copy)]
enum AuditCategory {
    Army,
    Economy,
    Capture,
    Objective,
    Position,
}

impl AuditCategory {
    const ALL: [Self; 5] = [
        Self::Army,
        Self::Economy,
        Self::Capture,
        Self::Objective,
        Self::Position,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::Army => "army trades",
            Self::Economy => "income/production",
            Self::Capture => "capture progress",
            Self::Objective => "objective defense",
            Self::Position => "exposure/front",
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::Army => 0,
            Self::Economy => 1,
            Self::Capture => 2,
            Self::Objective => 3,
            Self::Position => 4,
        }
    }
}

#[derive(Clone, Copy)]
struct AuditComponents {
    standard: EvalBreakdown,
    categories: [f64; 5],
}

#[derive(Clone)]
struct BranchPair {
    root_index: usize,
    selected_result: f64,
    terminal_best_result: f64,
    first_divergence: Option<u32>,
    selected: AuditComponents,
    terminal_best: AuditComponents,
    category: AuditCategory,
    fit_eligible: bool,
    selected_state: State,
    terminal_best_state: State,
    friendly_seat: awvm::semantic::PlayerIdx,
}

struct AuditCoverage {
    roots: u64,
    skipped: u64,
    pairs: Vec<BranchPair>,
    divergence_boundaries: BTreeMap<u32, u64>,
    unresolved_divergence: u64,
}

impl AuditCoverage {
    fn new() -> Self {
        Self {
            roots: 0,
            skipped: 0,
            pairs: Vec::new(),
            divergence_boundaries: BTreeMap::new(),
            unresolved_divergence: 0,
        }
    }
}

#[derive(Clone)]
struct TraceRun {
    states: Vec<State>,
}

struct FitResult {
    weights: EvalWeights,
    multipliers: [f64; 5],
    before: f64,
    after: f64,
}

impl StratifiedScriptCoverage {
    fn new(stratum: Stratum, script: Script) -> Self {
        Self {
            stratum,
            script,
            generated: 0,
            evaluated: 0,
            duplicate: 0,
            selected: 0,
            leaf_better: 0,
            result_better: 0,
            mission_total: 0,
            mission_preserved: 0,
            capture_completed: 0,
        }
    }
}

impl ScriptCoverage {
    fn new(script: Script) -> Self {
        Self {
            script,
            plans: 0,
            changed_plans: 0,
            order_changes: 0,
            order_slots: 0,
            lines: 0,
            leaf_lines: 0,
            leaf_better: 0,
            result_better: 0,
        }
    }
}

struct Coverage {
    roots_sampled: usize,
    roots_measured: u64,
    roots_skipped: u64,
    roots_with_baseline_disagreement: u64,
    roots_with_portfolio_disagreement: u64,
    unique_plans: u64,
    pairwise_plan_comparisons: u64,
    pairwise_plan_disagreements: u64,
    counterfactual_roots: u64,
    leaf_comparison_roots: u64,
    roots_with_leaf_coverage: u64,
    roots_with_result_coverage: u64,
    best_leaf_delta: f64,
    best_result_delta: f64,
    roots_with_default_stratified_disagreement: u64,
    roots_with_selected_stratified_disagreement: u64,
    roots_with_any_stratified_disagreement: u64,
    stratified_unique_plans: u64,
    stratified_candidates_generated: u64,
    stratified_candidates_evaluated: u64,
    stratified_duplicate_candidates: u64,
    stratified_leaf_comparison_roots: u64,
    stratified_result_comparison_roots: u64,
    roots_with_stratified_leaf_coverage: u64,
    roots_with_stratified_result_coverage: u64,
    roots_with_stratified_leaf_coverage_unique: u64,
    roots_with_stratified_result_coverage_unique: u64,
    roots_with_stratified_selected_leaf_coverage: u64,
    selected_leaf_comparison_roots: u64,
    roots_with_stratified_selected_result_coverage: u64,
    stratified_leaf_delta: f64,
    stratified_result_delta: f64,
    whole_leaf_delta: f64,
    whole_result_delta: f64,
    stratified_beats_whole_leaf: u64,
    stratified_beats_whole_result: u64,
    stratified_leaf_over_whole_delta: f64,
    stratified_result_over_whole_delta: f64,
    evaluator_leaf_oracle_hits: u64,
    evaluator_result_oracle_hits: u64,
    evaluator_selection_roots: u64,
    evaluator_result_selection_roots: u64,
    baseline_missions: MissionQuality,
    best_whole_missions: MissionQuality,
    stratified_oracle_missions: MissionQuality,
    selected_stratified_missions: MissionQuality,
    stratified_candidate_missions: MissionQuality,
    finished_lines: u64,
    total_lines: u64,
    total_turns: u64,
    evaluated_nodes: u64,
    runtime: Duration,
    scripts: [ScriptCoverage; 4],
    stratified_scripts: [[StratifiedScriptCoverage; 4]; 4],
}

impl Coverage {
    fn new(roots_sampled: usize) -> Self {
        Self {
            roots_sampled,
            roots_measured: 0,
            roots_skipped: 0,
            roots_with_baseline_disagreement: 0,
            roots_with_portfolio_disagreement: 0,
            unique_plans: 0,
            pairwise_plan_comparisons: 0,
            pairwise_plan_disagreements: 0,
            counterfactual_roots: 0,
            leaf_comparison_roots: 0,
            roots_with_leaf_coverage: 0,
            roots_with_result_coverage: 0,
            best_leaf_delta: 0.0,
            best_result_delta: 0.0,
            roots_with_default_stratified_disagreement: 0,
            roots_with_selected_stratified_disagreement: 0,
            roots_with_any_stratified_disagreement: 0,
            stratified_unique_plans: 0,
            stratified_candidates_generated: 0,
            stratified_candidates_evaluated: 0,
            stratified_duplicate_candidates: 0,
            stratified_leaf_comparison_roots: 0,
            stratified_result_comparison_roots: 0,
            roots_with_stratified_leaf_coverage: 0,
            roots_with_stratified_result_coverage: 0,
            roots_with_stratified_leaf_coverage_unique: 0,
            roots_with_stratified_result_coverage_unique: 0,
            roots_with_stratified_selected_leaf_coverage: 0,
            selected_leaf_comparison_roots: 0,
            roots_with_stratified_selected_result_coverage: 0,
            stratified_leaf_delta: 0.0,
            stratified_result_delta: 0.0,
            whole_leaf_delta: 0.0,
            whole_result_delta: 0.0,
            stratified_beats_whole_leaf: 0,
            stratified_beats_whole_result: 0,
            stratified_leaf_over_whole_delta: 0.0,
            stratified_result_over_whole_delta: 0.0,
            evaluator_leaf_oracle_hits: 0,
            evaluator_result_oracle_hits: 0,
            evaluator_selection_roots: 0,
            evaluator_result_selection_roots: 0,
            baseline_missions: MissionQuality::default(),
            best_whole_missions: MissionQuality::default(),
            stratified_oracle_missions: MissionQuality::default(),
            selected_stratified_missions: MissionQuality::default(),
            stratified_candidate_missions: MissionQuality::default(),
            finished_lines: 0,
            total_lines: 0,
            total_turns: 0,
            evaluated_nodes: 0,
            runtime: Duration::ZERO,
            scripts: std::array::from_fn(|index| ScriptCoverage::new(Script::ALL[index])),
            stratified_scripts: std::array::from_fn(|stratum| {
                std::array::from_fn(|script| {
                    StratifiedScriptCoverage::new(Stratum::ALL[stratum], Script::ALL[script])
                })
            }),
        }
    }

    fn measure(&mut self, root: &SampledRoot, days: u32) {
        let started = Instant::now();
        self.measure_root(root, days);
        self.runtime += started.elapsed();
    }

    fn measure_root(&mut self, root: &SampledRoot, days: u32) {
        let Some(baseline) = generate_plan(&root.view, root.seed, Weights::BASELINE) else {
            self.roots_skipped += 1;
            return;
        };
        let plans = generate_plans(&root.view, root.seed);
        if plans.len() != Script::ALL.len() {
            self.roots_skipped += 1;
            return;
        }

        self.roots_measured += 1;
        let unique = count_unique(plans.iter().map(|plan| &plan.plays));
        self.unique_plans += unique as u64;
        if unique > 1 {
            self.roots_with_portfolio_disagreement += 1;
        }

        let mut baseline_disagreement = false;
        for script_plan in &plans {
            let index = script_index(script_plan.script);
            let stats = &mut self.scripts[index];
            let changes = order_changes(&baseline, &script_plan.plays);
            let slots = baseline.len().max(script_plan.plays.len());
            stats.plans += 1;
            stats.order_changes += changes as u64;
            stats.order_slots += slots as u64;
            if script_plan.plays != baseline {
                stats.changed_plans += 1;
                baseline_disagreement = true;
            }
        }
        if baseline_disagreement {
            self.roots_with_baseline_disagreement += 1;
        }

        for left in 0..plans.len() {
            for right in (left + 1)..plans.len() {
                self.pairwise_plan_comparisons += 1;
                if plans[left].plays != plans[right].plays {
                    self.pairwise_plan_disagreements += 1;
                }
            }
        }

        let friendly = root.view.turn.active_player.clone();
        let Some(friendly_seat) = root.state.players.seat(&friendly) else {
            self.roots_skipped += 1;
            return;
        };
        let mut evaluator = Evaluator::new(EvalWeights::STANDARD);
        let Some(baseline_line) = forward(
            &root.state,
            &baseline,
            root.seed,
            days,
            friendly_seat,
            &friendly,
            &mut evaluator,
        ) else {
            self.roots_skipped += 1;
            return;
        };

        self.counterfactual_roots += 1;
        let mut root_missions = MissionBook::new();
        root_missions.update(&root.view);
        self.baseline_missions.add(mission_quality(
            &root.state,
            &baseline,
            &root_missions,
            root.seed,
        ));

        let mut whole_candidates = Vec::new();
        for script_plan in &plans {
            let index = script_index(script_plan.script);
            let Some(line) = forward(
                &root.state,
                &script_plan.plays,
                root.seed,
                days,
                friendly_seat,
                &friendly,
                &mut evaluator,
            ) else {
                continue;
            };
            let mission =
                mission_quality(&root.state, &script_plan.plays, &root_missions, root.seed);
            whole_candidates.push(WholeCandidate { line, mission });
            let stats = &mut self.scripts[index];
            stats.lines += 1;
            if let (Some(line_leaf), Some(baseline_leaf)) =
                (line.leaf_value, baseline_line.leaf_value)
            {
                stats.leaf_lines += 1;
                stats.leaf_better += u64::from(line_leaf > baseline_leaf);
            }
            stats.result_better += u64::from(line.result > baseline_line.result);
            self.total_lines += 1;
            self.finished_lines += u64::from(line.finished);
            self.total_turns += u64::from(line.turns);
        }

        let best_whole = best_whole_candidate(&whole_candidates);
        if let Some(best_whole) = best_whole.as_ref() {
            self.whole_result_delta += best_whole.line.result - baseline_line.result;
            self.best_result_delta += (best_whole.line.result - baseline_line.result).max(0.0);
            self.roots_with_result_coverage +=
                u64::from(best_whole.line.result > baseline_line.result);
            self.best_whole_missions.add(best_whole.mission);
            if let Some(best_leaf) = whole_candidates
                .iter()
                .filter_map(|candidate| candidate.line.leaf_value)
                .max_by(f64::total_cmp)
                .zip(baseline_line.leaf_value)
            {
                let leaf_delta = best_leaf.0 - best_leaf.1;
                self.leaf_comparison_roots += 1;
                self.best_leaf_delta += leaf_delta.max(0.0);
                self.whole_leaf_delta += leaf_delta;
                self.roots_with_leaf_coverage += u64::from(leaf_delta > 0.0);
            }
        }

        let default_assignment = StratifiedScripts::default();
        let mut current_assignment = default_assignment;
        let mut candidate_plans = Vec::new();
        let mut generated_plans = Vec::new();
        let mut selected_candidate = None;
        for stratum in Stratum::ALL {
            let mut best_index = None;
            let Some(stratum_candidates) = generate_stratum_candidates(
                &root.view,
                root.seed,
                &mut root_missions,
                current_assignment,
                stratum,
            ) else {
                continue;
            };
            for candidate in stratum_candidates {
                let assignment = candidate.scripts;
                let plays = candidate.plays;
                let stratum_index = stratum_index(stratum);
                let script_index = script_index(assignment.script(stratum));
                let stats = &mut self.stratified_scripts[stratum_index][script_index];
                stats.generated += 1;
                self.stratified_candidates_generated += 1;
                if generated_plans.contains(&plays) {
                    stats.duplicate += 1;
                    self.stratified_duplicate_candidates += 1;
                }
                generated_plans.push(plays.clone());
                let Some(line) = forward(
                    &root.state,
                    &plays,
                    root.seed,
                    days,
                    friendly_seat,
                    &friendly,
                    &mut evaluator,
                ) else {
                    continue;
                };
                let mission = mission_quality(&root.state, &plays, &root_missions, root.seed);
                let candidate_index = candidate_plans.len();
                candidate_plans.push(StratifiedCandidate {
                    stratum,
                    assignment,
                    plays,
                    line,
                    mission,
                });
                stats.evaluated += 1;
                self.stratified_candidates_evaluated += 1;
                self.evaluated_nodes += 1;
                self.stratified_candidate_missions.add(mission);
                self.total_lines += 1;
                self.finished_lines += u64::from(line.finished);
                self.total_turns += u64::from(line.turns);
                stats.mission_total += mission.total;
                stats.mission_preserved += mission.preserved;
                stats.capture_completed += mission.completed;
                stats.leaf_better +=
                    u64::from(matches!((line.leaf_value, baseline_line.leaf_value),
                        (Some(candidate), Some(baseline)) if candidate > baseline));
                stats.result_better += u64::from(line.result > baseline_line.result);
                let should_select = match best_index {
                    None => true,
                    Some(best) => selection_is_better(
                        &candidate_plans[candidate_index],
                        &candidate_plans[best],
                        current_assignment,
                    ),
                };
                if should_select {
                    best_index = Some(candidate_index);
                }
            }
            let Some(best_index) = best_index else {
                continue;
            };
            let best = &candidate_plans[best_index];
            current_assignment = best.assignment;
            self.stratified_scripts[stratum_index(stratum)]
                [script_index(best.assignment.script(stratum))]
            .selected += 1;
            selected_candidate = Some(best_index);
        }

        let default_stratified = candidate_plans
            .iter()
            .find(|candidate| candidate.assignment == default_assignment);
        if let Some(default_stratified) = default_stratified
            && default_stratified.plays != baseline
        {
            self.roots_with_default_stratified_disagreement += 1;
        }

        let unique = count_unique(generated_plans.iter());
        self.stratified_unique_plans += unique as u64;
        if generated_plans
            .iter()
            .any(|candidate| *candidate != baseline)
        {
            self.roots_with_any_stratified_disagreement += 1;
        }
        if candidate_plans.len() > STRATIFIED_CANDIDATE_LIMIT {
            unreachable!("the coordinate sweep exceeded its candidate budget");
        }

        let Some(selected_index) = selected_candidate else {
            return;
        };
        let selected = &candidate_plans[selected_index];
        self.selected_stratified_missions.add(selected.mission);
        if selected.plays != baseline {
            self.roots_with_selected_stratified_disagreement += 1;
        }

        let best_stratified_leaf = candidate_plans
            .iter()
            .filter_map(|candidate| candidate.line.leaf_value)
            .max_by(f64::total_cmp);
        let best_stratified_result = best_stratified_candidate(&candidate_plans);
        if let (Some(best_leaf), Some(baseline_leaf)) =
            (best_stratified_leaf, baseline_line.leaf_value)
        {
            let leaf_delta = best_leaf - baseline_leaf;
            self.stratified_leaf_comparison_roots += 1;
            self.stratified_leaf_delta += leaf_delta;
            self.roots_with_stratified_leaf_coverage += u64::from(leaf_delta > 0.0);
            let whole_leaf_better = whole_candidates.iter().any(|candidate| {
                matches!((candidate.line.leaf_value, baseline_line.leaf_value),
                    (Some(candidate), Some(baseline)) if candidate > baseline)
            });
            self.roots_with_stratified_leaf_coverage_unique +=
                u64::from(leaf_delta > 0.0 && !whole_leaf_better);
            if let Some(best_whole_leaf) = whole_candidates
                .iter()
                .filter_map(|candidate| candidate.line.leaf_value)
                .max_by(f64::total_cmp)
            {
                self.stratified_leaf_over_whole_delta += best_leaf - best_whole_leaf;
                self.stratified_beats_whole_leaf += u64::from(best_leaf > best_whole_leaf);
            }
        }
        if let Some(best_stratified) = best_stratified_result {
            let result_delta = best_stratified.line.result - baseline_line.result;
            self.stratified_result_comparison_roots += 1;
            self.stratified_result_delta += result_delta;
            self.roots_with_stratified_result_coverage += u64::from(result_delta > 0.0);
            let whole_result_better = best_whole
                .as_ref()
                .is_some_and(|candidate| candidate.line.result > baseline_line.result);
            self.roots_with_stratified_result_coverage_unique +=
                u64::from(result_delta > 0.0 && !whole_result_better);
            if let Some(best_whole) = best_whole.as_ref() {
                self.stratified_result_over_whole_delta +=
                    best_stratified.line.result - best_whole.line.result;
                self.stratified_beats_whole_result +=
                    u64::from(best_stratified.line.result > best_whole.line.result);
            }
            self.stratified_oracle_missions.add(best_stratified.mission);
        }
        if let (Some(best_leaf), Some(selected_leaf)) =
            (best_stratified_leaf, selected.line.leaf_value)
        {
            self.evaluator_selection_roots += 1;
            self.evaluator_leaf_oracle_hits += u64::from(selected_leaf == best_leaf);
            if let Some(baseline_leaf) = baseline_line.leaf_value {
                self.selected_leaf_comparison_roots += 1;
                self.roots_with_stratified_selected_leaf_coverage +=
                    u64::from(selected_leaf > baseline_leaf);
            }
        }
        if let Some(best_stratified) = best_stratified_result {
            self.evaluator_result_selection_roots += 1;
            self.evaluator_result_oracle_hits +=
                u64::from(selected.line.result == best_stratified.line.result);
            self.roots_with_stratified_selected_result_coverage +=
                u64::from(selected.line.result > baseline_line.result);
        }
    }
}

const fn script_index(script: Script) -> usize {
    match script {
        Script::CaptureCommitment => 0,
        Script::FavorableCombat => 1,
        Script::SafePressure => 2,
        Script::ObjectiveDefense => 3,
    }
}

const fn stratum_index(stratum: Stratum) -> usize {
    match stratum {
        Stratum::Objective => 0,
        Stratum::Support => 1,
        Stratum::Direct => 2,
        Stratum::Rear => 3,
    }
}

fn best_whole_candidate(candidates: &[WholeCandidate]) -> Option<&WholeCandidate> {
    let mut best = None;
    for candidate in candidates {
        if best.is_none_or(|current: &WholeCandidate| candidate.line.result > current.line.result) {
            best = Some(candidate);
        }
    }
    best
}

fn best_stratified_candidate(candidates: &[StratifiedCandidate]) -> Option<&StratifiedCandidate> {
    let mut best = None;
    for candidate in candidates {
        if best
            .is_none_or(|current: &StratifiedCandidate| candidate.line.result > current.line.result)
        {
            best = Some(candidate);
        }
    }
    best
}

fn selection_is_better(
    candidate: &StratifiedCandidate,
    best: &StratifiedCandidate,
    current: StratifiedScripts,
) -> bool {
    let candidate_rank = selection_rank(
        candidate.assignment.script(candidate.stratum),
        current,
        candidate.stratum,
    );
    let best_rank = selection_rank(best.assignment.script(best.stratum), current, best.stratum);
    match (candidate.line.leaf_value, best.line.leaf_value) {
        (Some(candidate), Some(best)) => {
            candidate > best || (candidate == best && candidate_rank < best_rank)
        }
        (Some(_), None) => true,
        (None, Some(_)) => false,
        _ => {
            candidate.line.result > best.line.result
                || (candidate.line.result == best.line.result && candidate_rank < best_rank)
        }
    }
}

fn selection_rank(script: Script, current: StratifiedScripts, stratum: Stratum) -> usize {
    if script == current.script(stratum) {
        0
    } else {
        1 + script_index(script)
    }
}

fn mission_quality(
    root: &State,
    plays: &[Play],
    missions: &MissionBook,
    seed: u64,
) -> MissionQuality {
    let active: Vec<_> = missions
        .capture_missions()
        .iter()
        .filter(|mission| mission.state.is_active())
        .copied()
        .collect();
    let mut quality = MissionQuality {
        total: active.len() as u64,
        ..MissionQuality::default()
    };
    if active.is_empty() {
        return quality;
    }

    let friendly = root.turn.active_player.clone();
    let mut session = Session::new(root.clone());
    let mut entropy = Rng::from_seed(Rng::mix(seed ^ ENTROPY_SALT));
    for play in plays {
        if session.state().turn.active_player != friendly {
            return quality;
        }
        let Some(command) = play.command(&session) else {
            return quality;
        };
        let Ok(order) = session.resolve(&command) else {
            return quality;
        };
        if session.apply(order, &mut entropy, &mut ()).is_err() {
            return quality;
        }
    }

    let Ok(view) = observe(&AwbwVisibility, session.state(), &friendly) else {
        return quality;
    };
    let mut after = missions.clone();
    after.update(&view);
    for mission in active {
        let Some(result) = after.capture_missions().iter().find(|candidate| {
            candidate.unit == mission.unit && candidate.property == mission.property
        }) else {
            continue;
        };
        if result.state != awbrn_ai::agents::CaptureMissionState::Invalid {
            quality.preserved += 1;
        }
        if result.state == awbrn_ai::agents::CaptureMissionState::Complete {
            quality.completed += 1;
        }
    }
    quality
}

fn mission_transitions(previous: &[CaptureMission], current: &MissionBook) -> ArenaMissionStats {
    let mut stats = ArenaMissionStats::default();
    for mission in previous.iter().filter(|mission| mission.state.is_active()) {
        stats.observed += 1;
        let state = current
            .capture_missions()
            .iter()
            .find(|candidate| {
                candidate.unit == mission.unit && candidate.property == mission.property
            })
            .map(|candidate| candidate.state);
        match state {
            Some(CaptureMissionState::Approaching | CaptureMissionState::Capturing) => {
                stats.preserved += 1;
            }
            Some(CaptureMissionState::SuspendedByEmergency) => {
                stats.preserved += 1;
                stats.suspended += 1;
            }
            Some(CaptureMissionState::Complete) => {
                stats.preserved += 1;
                stats.completed += 1;
            }
            Some(CaptureMissionState::Invalid) | None => {
                stats.invalidated += 1;
            }
        }
    }
    stats
}

fn run(options: &Options) -> Result<()> {
    let roots = collect_roots(options);
    let mut coverage = Coverage::new(roots.len());
    for root in &roots {
        coverage.measure(root, options.days);
    }
    report(options, &coverage);
    Ok(())
}

fn run_horizon_sweep(options: &Options) -> Result<()> {
    let roots = collect_roots(options);
    let mut coverage = HorizonCoverage::new(roots.len());
    for root in &roots {
        measure_horizon_root(&mut coverage, root, options.days);
    }
    report_horizon_sweep(options, &coverage);
    Ok(())
}

fn run_exposure_sweep(options: &Options) -> Result<()> {
    let roots = collect_roots(options);
    let mut coverage = ExposureSweepCoverage::new(roots.len());
    for root in &roots {
        measure_exposure_root(&mut coverage, root, options.days);
    }
    report_exposure_sweep(options, &coverage);
    Ok(())
}

fn run_stratified_arena(options: &Options) -> Result<()> {
    println!(
        "stratified arena: seed {}  pairs {}  games {}  days {}",
        options.seed,
        options.games,
        options.games * 2,
        options.days
    );
    println!(
        "the standard agent selects at four rounds; the adaptive agent extends only evaluator disagreements to eight rounds"
    );
    println!();

    let tallies: Vec<_> = StratifiedArenaPolicy::ALL
        .into_iter()
        .map(|policy| measure_stratified_arena(options, policy))
        .collect();
    for tally in &tallies {
        report_stratified_arena(tally);
    }
    if let [standard, adaptive] = tallies.as_slice() {
        report_stratified_difference(standard, adaptive);
    }
    Ok(())
}

fn run_adaptive_horizon(options: &Options) -> Result<()> {
    let roots = collect_roots(options);
    let mut coverage = AdaptiveCoverage::new(roots.len());
    for root in &roots {
        measure_adaptive_root(&mut coverage, root, options.days);
    }
    report_adaptive_horizon(options, &coverage);
    Ok(())
}

fn measure_adaptive_root(coverage: &mut AdaptiveCoverage, root: &SampledRoot, days: u32) {
    let Some(baseline) = generate_plan(&root.view, root.seed, Weights::BASELINE) else {
        coverage.roots_skipped += 1;
        return;
    };
    let Some(generated) = generate_sweep_plans(root, days) else {
        coverage.roots_skipped += 1;
        return;
    };
    let generated_count = generated.len();
    let unique = deduplicate_sweep_plans(generated);
    if unique.is_empty() {
        coverage.roots_skipped += 1;
        return;
    }
    let Some(friendly_seat) = root.state.players.seat(&root.view.turn.active_player) else {
        coverage.roots_skipped += 1;
        return;
    };
    let friendly = root.view.turn.active_player.clone();
    let mut standard_terminal_evaluator = Evaluator::new(EvalWeights::STANDARD);
    let mut terminal_context = HorizonContext {
        root: &root.state,
        seed: root.seed,
        days,
        friendly_seat,
        friendly: &friendly,
        evaluator: &mut standard_terminal_evaluator,
    };
    let Some(baseline_line) = evaluate_horizon(&mut terminal_context, &baseline, Horizon::Terminal)
    else {
        coverage.roots_skipped += 1;
        return;
    };
    let terminal_lines: Vec<_> = unique
        .iter()
        .map(|plan| evaluate_horizon(&mut terminal_context, &plan.plays, Horizon::Terminal))
        .collect();
    let terminal_results: Vec<_> = terminal_lines
        .iter()
        .map(|line| line.and_then(|line| line.terminal_result))
        .collect();
    let Some(terminal_oracle) = terminal_results
        .iter()
        .flatten()
        .copied()
        .max_by(f64::total_cmp)
    else {
        coverage.roots_skipped += 1;
        return;
    };
    let baseline_result = baseline_line.terminal_result.unwrap_or(baseline_line.score);
    let oracle_improves = terminal_oracle > baseline_result;
    let terminal_ties = terminal_results
        .iter()
        .flatten()
        .filter(|result| **result == terminal_oracle)
        .count();

    let mut pair_context_evaluator = Evaluator::new(EvalWeights::STANDARD);
    let mut standard_four_evaluator = Evaluator::new(EvalWeights::STANDARD);
    let mut conservative_four_evaluator = Evaluator::new(ExposureFrontArm::Conservative.weights());
    let four_context = HorizonContext {
        root: &root.state,
        seed: root.seed,
        days,
        friendly_seat,
        friendly: &friendly,
        evaluator: &mut pair_context_evaluator,
    };
    let four_started = Instant::now();
    let four_lines: Vec<_> = unique
        .iter()
        .map(|plan| {
            evaluate_horizon_both(
                &four_context,
                &plan.plays,
                Horizon::FourRounds,
                &mut standard_four_evaluator,
                &mut conservative_four_evaluator,
            )
        })
        .collect();
    let four_runtime = four_started.elapsed();
    coverage.standard_four.runtime += four_runtime;
    coverage.standard_four.evaluated_candidates +=
        four_lines.iter().filter(|line| line.is_some()).count() as u64;
    let standard_four_lines: Vec<_> = four_lines
        .iter()
        .map(|line| {
            line.map(|line| HorizonLine {
                score: line.standard_score,
                terminal_result: None,
            })
        })
        .collect();
    record_horizon_metrics(
        &mut coverage.standard_four,
        &standard_four_lines,
        &terminal_results,
        baseline_result,
        terminal_oracle,
        oracle_improves,
        terminal_ties,
    );

    let standard_top = top_two_adaptive(&four_lines, true);
    let conservative_top = top_two_adaptive(&four_lines, false);
    let Some(standard_selected) = standard_top.first().copied() else {
        coverage.roots_skipped += 1;
        return;
    };
    let Some(conservative_selected) = conservative_top.first().copied() else {
        coverage.roots_skipped += 1;
        return;
    };
    let mut selected_index = standard_selected;
    let mut adaptive_eight_runtime = Duration::ZERO;
    let mut extended = Vec::new();
    if standard_selected != conservative_selected {
        coverage.adaptive.uncertain_roots += 1;
        extended.extend(standard_top.iter().copied());
        for index in conservative_top {
            if !extended.contains(&index) {
                extended.push(index);
            }
        }
        let eight_started = Instant::now();
        let mut standard_eight_evaluator = Evaluator::new(EvalWeights::STANDARD);
        let mut conservative_eight_evaluator =
            Evaluator::new(ExposureFrontArm::Conservative.weights());
        let eight_lines: Vec<_> = extended
            .iter()
            .map(|index| {
                evaluate_horizon_both(
                    &four_context,
                    &unique[*index].plays,
                    Horizon::EightRounds,
                    &mut standard_eight_evaluator,
                    &mut conservative_eight_evaluator,
                )
            })
            .collect();
        adaptive_eight_runtime = eight_started.elapsed();
        coverage.adaptive.eight_round_runtime += adaptive_eight_runtime;
        coverage.adaptive.eight_round_replays += extended.len() as u64;
        if let Some((position, _)) = eight_lines
            .iter()
            .enumerate()
            .filter_map(|(position, line)| line.map(|line| (position, line)))
            .max_by(|left, right| {
                joint_score(left.1)
                    .total_cmp(&joint_score(right.1))
                    .then_with(|| right.0.cmp(&left.0))
            })
        {
            selected_index = extended[position];
        }
    }
    coverage.adaptive.runtime += four_runtime + adaptive_eight_runtime;
    coverage.adaptive.four_round_replays += unique.len() as u64;
    record_adaptive_metrics(
        &mut coverage.adaptive,
        selected_index,
        &terminal_results,
        baseline_result,
        terminal_oracle,
        oracle_improves,
        terminal_ties,
    );

    let mut always_eight_evaluator = Evaluator::new(EvalWeights::STANDARD);
    let mut always_eight_context = HorizonContext {
        root: &root.state,
        seed: root.seed,
        days,
        friendly_seat,
        friendly: &friendly,
        evaluator: &mut always_eight_evaluator,
    };
    let always_started = Instant::now();
    let always_eight_lines: Vec<_> = unique
        .iter()
        .map(|plan| evaluate_horizon(&mut always_eight_context, &plan.plays, Horizon::EightRounds))
        .collect();
    coverage.always_eight.runtime += always_started.elapsed();
    coverage.always_eight.evaluated_candidates += always_eight_lines
        .iter()
        .filter(|line| line.is_some())
        .count() as u64;
    record_horizon_metrics(
        &mut coverage.always_eight,
        &always_eight_lines,
        &terminal_results,
        baseline_result,
        terminal_oracle,
        oracle_improves,
        terminal_ties,
    );

    coverage.roots_measured += 1;
    coverage.generated_candidates += generated_count as u64;
    coverage.unique_candidates += unique.len() as u64;
    coverage.duplicate_candidates += (generated_count - unique.len()) as u64;
    coverage.terminal_oracle_improvement_roots += u64::from(oracle_improves);
}

fn top_two_adaptive(lines: &[Option<AdaptiveLine>], standard: bool) -> Vec<usize> {
    let mut indices: Vec<_> = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| line.map(|_| index))
        .collect();
    indices.sort_by(|left, right| {
        let left_score = adaptive_score(lines[*left], standard);
        let right_score = adaptive_score(lines[*right], standard);
        right_score
            .total_cmp(&left_score)
            .then_with(|| left.cmp(right))
    });
    indices.truncate(2);
    indices
}

fn adaptive_score(line: Option<AdaptiveLine>, standard: bool) -> f64 {
    let line = line.expect("adaptive score needs a valid line");
    if standard {
        line.standard_score
    } else {
        line.conservative_score
    }
}

fn joint_score(line: AdaptiveLine) -> f64 {
    (line.standard_score + line.conservative_score) / 2.0
}

fn record_adaptive_metrics(
    metrics: &mut AdaptiveMetrics,
    selected_index: usize,
    terminal_results: &[Option<f64>],
    baseline_result: f64,
    terminal_oracle: f64,
    oracle_improves: bool,
    terminal_ties: usize,
) {
    metrics.oracle_improvement_roots += u64::from(oracle_improves);
    metrics.selection_roots += 1;
    let Some(selected_result) = terminal_results[selected_index] else {
        return;
    };
    metrics.improvements_captured +=
        u64::from(oracle_improves && selected_result > baseline_result);
    metrics.regret += terminal_oracle - selected_result;
    if terminal_ties == 1 {
        metrics.non_tied_roots += 1;
        metrics.top_plan_hits += u64::from(selected_result == terminal_oracle);
    }
}

fn measure_stratified_arena(
    options: &Options,
    policy: StratifiedArenaPolicy,
) -> StratifiedArenaTally {
    let mut tally = StratifiedArenaTally::new(policy);
    let mut session = Session::new(arena(false, options.seed));
    for pair in 0..options.games {
        let game = Rng::mix(options.seed ^ ((pair as u64) << 32));
        let mut pair_score = 0.0;
        for stratified_first in [true, false] {
            let game_started = Instant::now();
            let state = arena(false, game);
            let teams: Vec<_> = state
                .players
                .seats()
                .map(|(_, player)| player.team.clone())
                .collect();
            let stratified_seed = Rng::mix(game ^ 0x2);
            let baseline_seed = Rng::mix(game ^ 0x3);
            let mut stratified = StratifiedArenaAgent::new(stratified_seed, options.days, policy);
            let mut baseline = GreedyAgent::with_weights(baseline_seed, Weights::BASELINE);
            let mut agents: [&mut dyn Agent; 2] = if stratified_first {
                [&mut stratified, &mut baseline]
            } else {
                [&mut baseline, &mut stratified]
            };
            let stratified_seat = usize::from(!stratified_first);
            let mut entropy = Rng::from_seed(Rng::mix(game ^ 0x1));
            let record = play_measured(
                state,
                &mut session,
                &mut agents,
                &mut entropy,
                Limits {
                    nodes: NodeBudget::ONE,
                    days: options.days,
                    ..Limits::DEFAULT
                },
            );
            let selection = stratified.selection_stats();
            tally.record_selection(selection);
            let game_score = match &record.outcome {
                Some(outcome) => {
                    tally.record_replacement_result(
                        Some(outcome),
                        &teams[stratified_seat],
                        selection.replaced_turns > 0,
                    );
                    tally.record(outcome, &teams[stratified_seat])
                }
                None => {
                    tally.record_replacement_result(
                        None,
                        &teams[stratified_seat],
                        selection.replaced_turns > 0,
                    );
                    tally.abandoned += 1;
                    0.5
                }
            };
            pair_score += game_score;
            tally.game_scores.push(game_score);
            tally.game_runtime += game_started.elapsed();
        }
        tally.pair_scores.push(pair_score / 2.0);
        tally.pairs += 1;
    }
    tally
}

fn report_stratified_arena(tally: &StratifiedArenaTally) {
    let games = tally.games();
    let (low, high) = paired_interval(&tally.pair_scores);
    println!(
        "  {:<14} wins {:>3}  losses {:>3}  draws {:>3}  abandoned {:>3}  score {:>5.1}%  pairs {:>3}",
        tally.policy.name(),
        tally.wins,
        tally.losses,
        tally.draws,
        tally.abandoned,
        100.0 * tally.points / (games.max(1) as f64),
        tally.pairs,
    );
    println!(
        "    paired score {:>6.3}  bootstrap 95% CI [{:>6.3}, {:>6.3}]",
        average(&tally.pair_scores),
        low,
        high,
    );
    println!(
        "    replacements {:>4}/{:<4} ({:>5.1}%), mean margin {:>8.3}, replacement margin {:>8.3}",
        tally.replaced_turns,
        tally.selection_turns,
        percent(tally.replaced_turns, tally.selection_turns),
        tally.selection_margin / tally.selection_turns.max(1) as f64,
        tally.replacement_margin / tally.replaced_turns.max(1) as f64,
    );
    println!(
        "    games with replacement: wins {:>3}  losses {:>3}  draws {:>3}  abandoned {:>3}",
        tally.replacement_wins,
        tally.replacement_losses,
        tally.replacement_draws,
        tally.replacement_abandoned,
    );
    let mission = tally.mission_stats;
    println!(
        "    missions observed {:>4}; preserved {:>4}/{:<4} ({:>5.1}%), completed {:>4}/{:<4} ({:>5.1}%), suspended {:>4}/{:<4} ({:>5.1}%), invalidated {:>4}/{:<4} ({:>5.1}%)",
        mission.observed,
        mission.preserved,
        mission.observed,
        percent(mission.preserved, mission.observed),
        mission.completed,
        mission.observed,
        percent(mission.completed, mission.observed),
        mission.suspended,
        mission.observed,
        percent(mission.suspended, mission.observed),
        mission.invalidated,
        mission.observed,
        percent(mission.invalidated, mission.observed),
    );
    let game_count = tally.game_scores.len().max(1) as f64;
    println!(
        "    runtime per game {:>8.2} ms; planning per turn {:>8.2} ms; planning turns/game {:>6.2}",
        1_000.0 * tally.game_runtime.as_secs_f64() / game_count,
        1_000.0 * tally.planning_runtime.as_secs_f64() / tally.selection_turns.max(1) as f64,
        tally.selection_turns as f64 / game_count,
    );
}

fn report_stratified_difference(standard: &StratifiedArenaTally, adaptive: &StratifiedArenaTally) {
    let differences: Vec<_> = adaptive
        .pair_scores
        .iter()
        .zip(&standard.pair_scores)
        .map(|(adaptive, standard)| adaptive - standard)
        .collect();
    let (low, high) = paired_interval(&differences);
    println!();
    println!("adaptive versus standard-4");
    println!(
        "  paired score difference {:>6.3}; bootstrap 95% CI [{:>6.3}, {:>6.3}]",
        average(&differences),
        low,
        high,
    );
}

fn average(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len().max(1) as f64
}

fn paired_interval(scores: &[f64]) -> (f64, f64) {
    const RESAMPLES: usize = 10_000;
    assert!(!scores.is_empty(), "an interval needs at least one pair");

    let mut rng = Rng::from_seed(0x7061_6972_2d63_6921);
    let mut means = Vec::with_capacity(RESAMPLES);
    for _ in 0..RESAMPLES {
        let sum = (0..scores.len())
            .map(|_| scores[rng.below(scores.len() as u64) as usize])
            .sum::<f64>();
        means.push(sum / scores.len() as f64);
    }
    means.sort_unstable_by(f64::total_cmp);
    let percentile = |numerator: usize| means[(RESAMPLES - 1) * numerator / 1_000];
    (percentile(25), percentile(975))
}

fn measure_exposure_root(coverage: &mut ExposureSweepCoverage, root: &SampledRoot, days: u32) {
    let Some(baseline) = generate_plan(&root.view, root.seed, Weights::BASELINE) else {
        coverage.roots_skipped += 1;
        return;
    };
    let Some(generated) = generate_sweep_plans(root, days) else {
        coverage.roots_skipped += 1;
        return;
    };
    let generated_count = generated.len();
    let unique = deduplicate_sweep_plans(generated);
    if unique.is_empty() {
        coverage.roots_skipped += 1;
        return;
    }
    let Some(friendly_seat) = root.state.players.seat(&root.view.turn.active_player) else {
        coverage.roots_skipped += 1;
        return;
    };
    let friendly = root.view.turn.active_player.clone();
    let mut terminal_evaluator = Evaluator::new(EvalWeights::STANDARD);
    let mut terminal_context = HorizonContext {
        root: &root.state,
        seed: root.seed,
        days,
        friendly_seat,
        friendly: &friendly,
        evaluator: &mut terminal_evaluator,
    };
    let Some(baseline_line) = evaluate_horizon(&mut terminal_context, &baseline, Horizon::Terminal)
    else {
        coverage.roots_skipped += 1;
        return;
    };
    let mut terminal_lines = Vec::with_capacity(unique.len());
    for plan in &unique {
        terminal_lines.push(evaluate_horizon(
            &mut terminal_context,
            &plan.plays,
            Horizon::Terminal,
        ));
    }
    let terminal_results: Vec<_> = terminal_lines
        .iter()
        .map(|line| line.and_then(|line| line.terminal_result))
        .collect();
    let Some(terminal_oracle) = terminal_results
        .iter()
        .flatten()
        .copied()
        .max_by(f64::total_cmp)
    else {
        coverage.roots_skipped += 1;
        return;
    };
    let baseline_result = baseline_line.terminal_result.unwrap_or(baseline_line.score);
    let oracle_improves = terminal_oracle > baseline_result;
    let terminal_ties = terminal_results
        .iter()
        .flatten()
        .filter(|result| **result == terminal_oracle)
        .count();

    coverage.roots_measured += 1;
    coverage.generated_candidates += generated_count as u64;
    coverage.unique_candidates += unique.len() as u64;
    coverage.duplicate_candidates += (generated_count - unique.len()) as u64;
    coverage.terminal_oracle_improvement_roots += u64::from(oracle_improves);

    for (arm_index, arm) in ExposureFrontArm::ALL.into_iter().enumerate() {
        let mut evaluator = Evaluator::new(arm.weights());
        let mut context = HorizonContext {
            root: &root.state,
            seed: root.seed,
            days,
            friendly_seat,
            friendly: &friendly,
            evaluator: &mut evaluator,
        };
        for (horizon_index, horizon) in Horizon::SELECTION.into_iter().enumerate() {
            let started = Instant::now();
            let lines: Vec<_> = unique
                .iter()
                .map(|plan| evaluate_horizon(&mut context, &plan.plays, horizon))
                .collect();
            coverage.arms[arm_index].horizons[horizon_index].runtime += started.elapsed();
            coverage.arms[arm_index].horizons[horizon_index].evaluated_candidates +=
                lines.iter().filter(|line| line.is_some()).count() as u64;
            record_horizon_metrics(
                &mut coverage.arms[arm_index].horizons[horizon_index],
                &lines,
                &terminal_results,
                baseline_result,
                terminal_oracle,
                oracle_improves,
                terminal_ties,
            );
        }
    }
}

fn record_horizon_metrics(
    metrics: &mut HorizonMetrics,
    lines: &[Option<HorizonLine>],
    terminal_results: &[Option<f64>],
    baseline_result: f64,
    terminal_oracle: f64,
    oracle_improves: bool,
    terminal_ties: usize,
) {
    metrics.oracle_improvement_roots += u64::from(oracle_improves);
    let Some((selected_index, _selected_line)) = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            line.and_then(|line| terminal_results[index].map(|_| (index, line)))
        })
        .max_by(|left, right| {
            left.1
                .score
                .total_cmp(&right.1.score)
                .then_with(|| right.0.cmp(&left.0))
        })
    else {
        return;
    };
    metrics.selection_roots += 1;
    let Some(selected_result) = terminal_results[selected_index] else {
        return;
    };
    metrics.selected_improvement_roots += u64::from(selected_result > baseline_result);
    metrics.improvements_captured +=
        u64::from(oracle_improves && selected_result > baseline_result);
    metrics.regret += terminal_oracle - selected_result;
    if terminal_ties == 1 {
        metrics.non_tied_roots += 1;
        metrics.top_plan_hits += u64::from(selected_result == terminal_oracle);
    }

    for left in 0..lines.len() {
        let (Some(left_line), Some(left_result)) = (lines[left], terminal_results[left]) else {
            continue;
        };
        for right in (left + 1)..lines.len() {
            let (Some(right_line), Some(right_result)) = (lines[right], terminal_results[right])
            else {
                continue;
            };
            let terminal_order = left_result.total_cmp(&right_result);
            if terminal_order == std::cmp::Ordering::Equal {
                continue;
            }
            metrics.pairwise_pairs += 1;
            let score_order = left_line.score.total_cmp(&right_line.score);
            if score_order == terminal_order {
                metrics.pairwise_score += 1.0;
            } else if score_order == std::cmp::Ordering::Equal {
                metrics.pairwise_score += 0.5;
            }
        }
    }
}

fn run_horizon_audit(options: &Options) -> Result<()> {
    let fit_roots = collect_roots(options);
    let audit = collect_audit_pairs(&fit_roots, options.days);
    let fit = fit_eval_weights(&audit.pairs);
    report_horizon_audit(options, &audit, &fit);

    let mut validation_options = options.clone();
    validation_options.seed = options.validation_seed;
    validation_options.horizon_audit = false;
    validation_options.horizon_sweep = true;
    let validation_roots = collect_roots(&validation_options);

    let mut standard = HorizonCoverage::new(validation_roots.len());
    for root in &validation_roots {
        measure_horizon_root_with_weights(&mut standard, root, options.days, EvalWeights::STANDARD);
    }
    report_horizon_sweep_named(
        &validation_options,
        &standard,
        "held-out horizon sweep (standard evaluator)",
    );

    let mut fitted = HorizonCoverage::new(validation_roots.len());
    for root in &validation_roots {
        measure_horizon_root_with_weights(&mut fitted, root, options.days, fit.weights);
    }
    report_horizon_sweep_named(
        &validation_options,
        &fitted,
        "held-out horizon sweep (fitted evaluator)",
    );
    Ok(())
}

fn measure_horizon_root(coverage: &mut HorizonCoverage, root: &SampledRoot, days: u32) {
    measure_horizon_root_with_weights(coverage, root, days, EvalWeights::STANDARD);
}

fn measure_horizon_root_with_weights(
    coverage: &mut HorizonCoverage,
    root: &SampledRoot,
    days: u32,
    eval_weights: EvalWeights,
) {
    let Some(baseline) = generate_plan(&root.view, root.seed, Weights::BASELINE) else {
        coverage.roots_skipped += 1;
        return;
    };
    let Some(generated) = generate_sweep_plans(root, days) else {
        coverage.roots_skipped += 1;
        return;
    };
    let generated_count = generated.len();
    let unique = deduplicate_sweep_plans(generated);
    if unique.is_empty() {
        coverage.roots_skipped += 1;
        return;
    }

    let Some(friendly_seat) = root.state.players.seat(&root.view.turn.active_player) else {
        coverage.roots_skipped += 1;
        return;
    };
    let friendly = root.view.turn.active_player.clone();
    let mut evaluator = Evaluator::new(eval_weights);
    let mut context = HorizonContext {
        root: &root.state,
        seed: root.seed,
        days,
        friendly_seat,
        friendly: &friendly,
        evaluator: &mut evaluator,
    };
    let Some(baseline_line) = evaluate_horizon(&mut context, &baseline, Horizon::Terminal) else {
        coverage.roots_skipped += 1;
        return;
    };

    let mut lines = vec![vec![None; unique.len()]; Horizon::ALL.len()];
    for (horizon_index, horizon) in Horizon::ALL.into_iter().enumerate() {
        let started = Instant::now();
        for (candidate_index, plan) in unique.iter().enumerate() {
            let line = evaluate_horizon(&mut context, &plan.plays, horizon);
            if line.is_some() {
                coverage.horizons[horizon_index].evaluated_candidates += 1;
            }
            lines[horizon_index][candidate_index] = line;
        }
        coverage.horizons[horizon_index].runtime += started.elapsed();
    }

    let terminal_results: Vec<_> = lines[Horizon::ALL.len() - 1]
        .iter()
        .map(|line| line.and_then(|line| line.terminal_result))
        .collect();
    let Some(terminal_oracle) = terminal_results
        .iter()
        .flatten()
        .copied()
        .max_by(f64::total_cmp)
    else {
        return;
    };
    coverage.roots_measured += 1;
    coverage.generated_candidates += generated_count as u64;
    coverage.unique_candidates += unique.len() as u64;
    coverage.duplicate_candidates += (generated_count - unique.len()) as u64;
    let terminal_ties = terminal_results
        .iter()
        .flatten()
        .filter(|result| **result == terminal_oracle)
        .count();
    let baseline_result = baseline_line.terminal_result.unwrap_or(baseline_line.score);
    let oracle_improves = terminal_oracle > baseline_result;
    coverage.terminal_oracle_improvement_roots += u64::from(oracle_improves);

    for (horizon_index, horizon_lines) in lines.iter().enumerate() {
        let metrics = &mut coverage.horizons[horizon_index];
        metrics.oracle_improvement_roots += u64::from(oracle_improves);
        let Some((selected_index, _selected_line)) = horizon_lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| {
                line.and_then(|line| terminal_results[index].map(|_| (index, line)))
            })
            .max_by(|left, right| {
                left.1
                    .score
                    .total_cmp(&right.1.score)
                    .then_with(|| right.0.cmp(&left.0))
            })
        else {
            continue;
        };
        metrics.selection_roots += 1;
        let Some(selected_result) = terminal_results[selected_index] else {
            continue;
        };
        metrics.selected_improvement_roots += u64::from(selected_result > baseline_result);
        metrics.improvements_captured +=
            u64::from(oracle_improves && selected_result > baseline_result);
        metrics.regret += terminal_oracle - selected_result;
        if terminal_ties == 1 {
            metrics.non_tied_roots += 1;
            metrics.top_plan_hits += u64::from(selected_result == terminal_oracle);
        }

        for left in 0..horizon_lines.len() {
            let (Some(left_line), Some(left_result)) =
                (horizon_lines[left], terminal_results[left])
            else {
                continue;
            };
            for right in (left + 1)..horizon_lines.len() {
                let (Some(right_line), Some(right_result)) =
                    (horizon_lines[right], terminal_results[right])
                else {
                    continue;
                };
                let terminal_order = left_result.total_cmp(&right_result);
                if terminal_order == std::cmp::Ordering::Equal {
                    continue;
                }
                metrics.pairwise_pairs += 1;
                let score_order = left_line.score.total_cmp(&right_line.score);
                if score_order == terminal_order {
                    metrics.pairwise_score += 1.0;
                } else if score_order == std::cmp::Ordering::Equal {
                    metrics.pairwise_score += 0.5;
                }
            }
        }
    }
}

fn collect_audit_pairs(roots: &[SampledRoot], days: u32) -> AuditCoverage {
    let mut coverage = AuditCoverage::new();
    for (root_index, root) in roots.iter().enumerate() {
        let Some(generated) = generate_sweep_plans(root, days) else {
            coverage.skipped += 1;
            continue;
        };
        let unique = deduplicate_sweep_plans(generated);
        let Some(friendly_seat) = root.state.players.seat(&root.view.turn.active_player) else {
            coverage.skipped += 1;
            continue;
        };
        let friendly = root.view.turn.active_player.clone();
        let mut evaluator = Evaluator::new(EvalWeights::STANDARD);
        let mut context = HorizonContext {
            root: &root.state,
            seed: root.seed,
            days,
            friendly_seat,
            friendly: &friendly,
            evaluator: &mut evaluator,
        };
        let mut four_round_lines = Vec::with_capacity(unique.len());
        let mut terminal_lines = Vec::with_capacity(unique.len());
        for plan in &unique {
            four_round_lines.push(evaluate_horizon(
                &mut context,
                &plan.plays,
                Horizon::FourRounds,
            ));
            terminal_lines.push(evaluate_horizon(
                &mut context,
                &plan.plays,
                Horizon::Terminal,
            ));
        }
        let Some(terminal_oracle) = terminal_lines
            .iter()
            .filter_map(|line| line.and_then(|line| line.terminal_result))
            .max_by(f64::total_cmp)
        else {
            coverage.skipped += 1;
            continue;
        };
        coverage.roots += 1;

        let Some(selected_index) = best_horizon_index(&four_round_lines, &terminal_lines) else {
            continue;
        };
        let Some(selected_result) =
            terminal_lines[selected_index].and_then(|line| line.terminal_result)
        else {
            continue;
        };
        if selected_result >= terminal_oracle {
            continue;
        }
        let Some(terminal_best_index) = best_horizon_index(&terminal_lines, &terminal_lines) else {
            continue;
        };
        let Some(selected_trace) = trace_branch(
            &root.state,
            &unique[selected_index].plays,
            root.seed,
            days,
            &friendly,
        ) else {
            continue;
        };
        let Some(terminal_best_trace) = trace_branch(
            &root.state,
            &unique[terminal_best_index].plays,
            root.seed,
            days,
            &friendly,
        ) else {
            continue;
        };
        let Some(selected_state) = state_at_round_boundary(&selected_trace.states, 8) else {
            continue;
        };
        let Some(terminal_best_state) = state_at_round_boundary(&terminal_best_trace.states, 8)
        else {
            continue;
        };
        let selected_components = audit_components(&selected_state, friendly_seat);
        let terminal_best_components = audit_components(&terminal_best_state, friendly_seat);
        let fit_eligible = matches!(selected_state.match_state, Match::Active { .. })
            && matches!(terminal_best_state.match_state, Match::Active { .. });
        let first_divergence = first_outcome_divergence(
            &selected_trace.states,
            &terminal_best_trace.states,
            friendly_seat,
        );
        if let Some(boundary) = first_divergence {
            *coverage.divergence_boundaries.entry(boundary).or_default() += 1;
        } else {
            coverage.unresolved_divergence += 1;
        }
        let category = error_category(selected_components, terminal_best_components);
        coverage.pairs.push(BranchPair {
            root_index,
            selected_result,
            terminal_best_result: terminal_oracle,
            first_divergence,
            selected: selected_components,
            terminal_best: terminal_best_components,
            category,
            fit_eligible,
            selected_state,
            terminal_best_state,
            friendly_seat,
        });
    }
    coverage
}

fn best_horizon_index(
    lines: &[Option<HorizonLine>],
    terminal_lines: &[Option<HorizonLine>],
) -> Option<usize> {
    lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            line.and_then(|line| {
                terminal_lines[index]
                    .and_then(|terminal| terminal.terminal_result)
                    .map(|_| (index, line))
            })
        })
        .max_by(|left, right| {
            left.1
                .score
                .total_cmp(&right.1.score)
                .then_with(|| right.0.cmp(&left.0))
        })
        .map(|(index, _)| index)
}

fn trace_branch(
    root: &State,
    plays: &[Play],
    seed: u64,
    days: u32,
    friendly: &awvm::semantic::PlayerId,
) -> Option<TraceRun> {
    let mut state = root.clone();
    state.settings.day_limit = Some(u64::from(days));
    let mut session = Session::new(state);
    let mut entropy = Rng::from_seed(Rng::mix(seed ^ ENTROPY_SALT));
    for play in plays {
        if session.state().turn.active_player != *friendly
            || !matches!(session.state().match_state, Match::Active { .. })
        {
            return None;
        }
        let command = play.command(&session)?;
        let order = session.resolve(&command).ok()?;
        session.apply(order, &mut entropy, &mut ()).ok()?;
    }
    if session.state().turn.active_player == *friendly
        && matches!(session.state().match_state, Match::Active { .. })
    {
        let command = Command::EndTurn {
            player: friendly.clone(),
        };
        let order = session.resolve(&command).ok()?;
        session.apply(order, &mut entropy, &mut ()).ok()?;
    }

    let mut states = vec![session.state().clone()];
    let mut turns = 0;
    while turns < MAX_TURNS && matches!(session.state().match_state, Match::Active { .. }) {
        let turn_seed = Rng::mix(seed ^ REPLY_SALT ^ ((u64::from(turns)) << 32));
        greedy_turn(&mut session, turn_seed, &mut entropy)?;
        turns += 1;
        states.push(session.state().clone());
    }
    Some(TraceRun { states })
}

fn state_at_round_boundary(states: &[State], continuation_turns: usize) -> Option<State> {
    states
        .get(continuation_turns)
        .or_else(|| states.last())
        .cloned()
}

fn first_outcome_divergence(
    selected: &[State],
    terminal_best: &[State],
    friendly_seat: awvm::semantic::PlayerIdx,
) -> Option<u32> {
    for index in 0..selected.len().max(terminal_best.len()) {
        let selected_outcome = selected
            .get(index)
            .and_then(|state| finished_score(state, friendly_seat));
        let terminal_best_outcome = terminal_best
            .get(index)
            .and_then(|state| finished_score(state, friendly_seat));
        if selected_outcome != terminal_best_outcome {
            return Some(index as u32);
        }
    }
    None
}

fn finished_score(state: &State, friendly_seat: awvm::semantic::PlayerIdx) -> Option<f64> {
    match &state.match_state {
        Match::Finished { outcome } => {
            Some(outcome_score(outcome, &state.player(friendly_seat).team))
        }
        Match::Active { .. } => None,
    }
}

fn audit_components(state: &State, seat: awvm::semantic::PlayerIdx) -> AuditComponents {
    let session = Session::new(state.clone());
    let mut evaluator = Evaluator::new(EvalWeights::STANDARD);
    let standard = evaluator.breakdown_in(&session, seat);
    let categories = std::array::from_fn(|index| {
        category_value(state, seat, AuditCategory::ALL[index], standard.score)
    });
    AuditComponents {
        standard,
        categories,
    }
}

fn category_value(
    state: &State,
    seat: awvm::semantic::PlayerIdx,
    category: AuditCategory,
    score: f64,
) -> f64 {
    let mut without = EvalWeights::STANDARD;
    match category {
        AuditCategory::Army => without.army = 0.0,
        AuditCategory::Economy => {
            without.bank = 0.0;
            without.income_days = 0.0;
            without.production = 0.0;
        }
        AuditCategory::Capture => without.capture = 0.0,
        AuditCategory::Objective => {
            without.plurality = 0.0;
            without.hq = 0.0;
        }
        AuditCategory::Position => {
            without.exposure = 0.0;
            without.contest = 0.0;
            without.front = 0.0;
        }
    }
    score - Evaluator::new(without).value(state, seat)
}

fn error_category(selected: AuditComponents, terminal_best: AuditComponents) -> AuditCategory {
    let deltas: [f64; 5] =
        std::array::from_fn(|index| selected.categories[index] - terminal_best.categories[index]);
    let mut category = AuditCategory::ALL[0];
    let mut largest = deltas[0];
    for candidate in AuditCategory::ALL {
        let delta = deltas[candidate.index()];
        if delta > largest {
            category = candidate;
            largest = delta;
        }
    }
    if largest > 0.0 {
        return category;
    }
    for candidate in AuditCategory::ALL {
        let delta = deltas[candidate.index()].abs();
        if delta > largest.abs() {
            category = candidate;
            largest = delta;
        }
    }
    category
}

fn fit_eval_weights(pairs: &[BranchPair]) -> FitResult {
    let mut multipliers = [1.0; 5];
    let before = pair_score(pairs, scaled_eval_weights(multipliers));
    let grid = [0.0, 0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0];
    for _ in 0..2 {
        for category in AuditCategory::ALL {
            let current_score = pair_score(pairs, scaled_eval_weights(multipliers));
            let current_margin = pair_margin(pairs, scaled_eval_weights(multipliers));
            let mut best_multiplier = multipliers[category.index()];
            let mut best_score = current_score;
            let mut best_margin = current_margin;
            for multiplier in grid {
                let mut candidate = multipliers;
                candidate[category.index()] = multiplier;
                let weights = scaled_eval_weights(candidate);
                let score = pair_score(pairs, weights);
                let margin = pair_margin(pairs, weights);
                if score > best_score + f64::EPSILON
                    || (score == best_score
                        && margin > best_margin + f64::EPSILON
                        && (multiplier - 1.0).abs() <= 2.0)
                {
                    best_multiplier = multiplier;
                    best_score = score;
                    best_margin = margin;
                }
            }
            multipliers[category.index()] = best_multiplier;
        }
    }
    let weights = scaled_eval_weights(multipliers);
    FitResult {
        weights,
        multipliers,
        before,
        after: pair_score(pairs, weights),
    }
}

fn scaled_eval_weights(multipliers: [f64; 5]) -> EvalWeights {
    let mut weights = EvalWeights::STANDARD;
    weights.army *= multipliers[AuditCategory::Army.index()];
    weights.bank *= multipliers[AuditCategory::Economy.index()];
    weights.income_days *= multipliers[AuditCategory::Economy.index()];
    weights.production *= multipliers[AuditCategory::Economy.index()];
    weights.capture *= multipliers[AuditCategory::Capture.index()];
    weights.plurality *= multipliers[AuditCategory::Objective.index()];
    weights.hq *= multipliers[AuditCategory::Objective.index()];
    weights.exposure *= multipliers[AuditCategory::Position.index()];
    weights.contest *= multipliers[AuditCategory::Position.index()];
    weights.front *= multipliers[AuditCategory::Position.index()];
    weights
}

fn pair_score(pairs: &[BranchPair], weights: EvalWeights) -> f64 {
    let count = pairs.iter().filter(|pair| pair.fit_eligible).count();
    if count == 0 {
        return 0.0;
    }
    let mut score = 0.0;
    for pair in pairs.iter().filter(|pair| pair.fit_eligible) {
        let mut evaluator = Evaluator::new(weights);
        let selected = evaluator.value(&pair.selected_state, pair.friendly_seat);
        let terminal_best = evaluator.value(&pair.terminal_best_state, pair.friendly_seat);
        match terminal_best.total_cmp(&selected) {
            std::cmp::Ordering::Greater => score += 1.0,
            std::cmp::Ordering::Equal => score += 0.5,
            std::cmp::Ordering::Less => {}
        }
    }
    score / count as f64
}

fn pair_margin(pairs: &[BranchPair], weights: EvalWeights) -> f64 {
    let count = pairs.iter().filter(|pair| pair.fit_eligible).count();
    if count == 0 {
        return 0.0;
    }
    pairs
        .iter()
        .filter(|pair| pair.fit_eligible)
        .map(|pair| {
            let mut evaluator = Evaluator::new(weights);
            evaluator.value(&pair.terminal_best_state, pair.friendly_seat)
                - evaluator.value(&pair.selected_state, pair.friendly_seat)
        })
        .sum::<f64>()
        / count as f64
}

fn generate_sweep_plans(root: &SampledRoot, days: u32) -> Option<Vec<SweepPlan>> {
    let mut missions = MissionBook::new();
    generate_sweep_plans_with_missions(root, days, &mut missions)
}

fn generate_arena_sweep_plans(
    root: &SampledRoot,
    days: u32,
    missions: &mut MissionBook,
) -> Option<Vec<SweepPlan>> {
    let baseline = generate_plan(&root.view, root.seed, Weights::BASELINE)?;
    let mut plans = vec![SweepPlan { plays: baseline }];
    plans.extend(generate_sweep_plans_with_missions(root, days, missions)?);
    Some(plans)
}

fn generate_sweep_plans_with_missions(
    root: &SampledRoot,
    days: u32,
    missions: &mut MissionBook,
) -> Option<Vec<SweepPlan>> {
    let friendly = root.view.turn.active_player.clone();
    let friendly_seat = root.state.players.seat(&friendly)?;
    let mut evaluator = Evaluator::new(EvalWeights::STANDARD);
    missions.update(&root.view);
    let default_assignment = StratifiedScripts::default();
    let mut current_assignment = default_assignment;
    let mut generated = Vec::new();
    let mut evaluated = Vec::new();

    for stratum in Stratum::ALL {
        let mut best_index = None;
        // A stratum the planner cannot plan drops out of the sweep, so that
        // the plans the other strata offer still stand.
        let Some(stratum_candidates) = generate_stratum_candidates(
            &root.view,
            root.seed,
            &mut *missions,
            current_assignment,
            stratum,
        ) else {
            continue;
        };
        for candidate in stratum_candidates {
            let assignment = candidate.scripts;
            let plays = candidate.plays;
            generated.push(SweepPlan {
                plays: plays.clone(),
            });
            let Some(line) = forward(
                &root.state,
                &plays,
                root.seed,
                days,
                friendly_seat,
                &friendly,
                &mut evaluator,
            ) else {
                continue;
            };
            let candidate_index = evaluated.len();
            evaluated.push(StratifiedCandidate {
                stratum,
                assignment,
                plays,
                line,
                mission: MissionQuality::default(),
            });
            let should_select = match best_index {
                None => true,
                Some(best) => selection_is_better(
                    &evaluated[candidate_index],
                    &evaluated[best],
                    current_assignment,
                ),
            };
            if should_select {
                best_index = Some(candidate_index);
            }
        }
        if let Some(best_index) = best_index {
            current_assignment = evaluated[best_index].assignment;
        }
    }
    Some(generated)
}

fn deduplicate_sweep_plans(plans: Vec<SweepPlan>) -> Vec<SweepPlan> {
    let mut unique = Vec::new();
    for plan in plans {
        if unique
            .iter()
            .all(|candidate: &SweepPlan| candidate.plays != plan.plays)
        {
            unique.push(plan);
        }
    }
    unique
}

fn collect_roots(options: &Options) -> Vec<SampledRoot> {
    let mut roots = Vec::new();
    let total_games = options.games * 2;
    for pair in 0..options.games {
        for search_first in [true, false] {
            if roots.len() >= options.roots {
                return roots;
            }
            let game_index = pair * 2 + usize::from(!search_first);
            let game = Rng::mix(options.seed ^ ((pair as u64) << 32) ^ u64::from(!search_first));
            let mut entropy = Rng::from_seed(Rng::mix(game ^ 0x1));
            let mut first = GreedyAgent::with_weights(Rng::mix(game ^ 0x2), Weights::BASELINE);
            let mut second = GreedyAgent::with_weights(Rng::mix(game ^ 0x3), Weights::BASELINE);
            let mut agents: [&mut dyn Agent; 2] = if search_first {
                [&mut first, &mut second]
            } else {
                [&mut second, &mut first]
            };
            let state = arena(false, game);
            let mut session = Session::new(state.clone());
            let root_seed = options.seed ^ ((game_index as u64) << 32);
            let mut game_roots = Vec::new();
            let mut observer = |state: &State, command: Option<&Command>| {
                let at_turn_start = command.is_none()
                    || command.is_some_and(|command| matches!(command, Command::EndTurn { .. }));
                if !at_turn_start {
                    return;
                }
                if !matches!(state.match_state, Match::Active { .. }) {
                    return;
                }
                let Some(player) = state.players.seat(&state.turn.active_player) else {
                    return;
                };
                let Ok(view) = observe(&AwbwVisibility, state, &state.turn.active_player) else {
                    return;
                };
                let sample_seed = Rng::mix(root_seed ^ ((game_roots.len() as u64) << 16));
                game_roots.push(SampledRoot {
                    state: state.clone(),
                    view,
                    seed: sample_seed ^ player.get() as u64,
                });
            };
            play_observed(
                state,
                &mut session,
                &mut agents,
                &mut entropy,
                Limits {
                    nodes: NodeBudget::ONE,
                    days: options.days,
                    ..Limits::DEFAULT
                },
                &mut observer,
            );
            let remaining_roots = options.roots - roots.len();
            let remaining_games = total_games - game_index;
            let take = remaining_roots.div_ceil(remaining_games);
            roots.extend(select_roots(game_roots, take));
        }
    }
    roots
}

fn select_roots(roots: Vec<SampledRoot>, count: usize) -> Vec<SampledRoot> {
    if count == 0 || roots.is_empty() {
        return Vec::new();
    }
    if count >= roots.len() {
        return roots;
    }
    if count == 1 {
        return vec![roots[roots.len() / 2].clone()];
    }
    (0..count)
        .map(|index| roots[index * (roots.len() - 1) / (count - 1)].clone())
        .collect()
}

fn forward(
    root: &State,
    plays: &[Play],
    seed: u64,
    days: u32,
    friendly_seat: awvm::semantic::PlayerIdx,
    friendly: &awvm::semantic::PlayerId,
    evaluator: &mut Evaluator,
) -> Option<LineResult> {
    let mut state = root.clone();
    state.settings.day_limit = Some(u64::from(days));
    let mut session = Session::new(state);
    let mut entropy = Rng::from_seed(Rng::mix(seed ^ ENTROPY_SALT));

    for play in plays {
        if session.state().turn.active_player != *friendly
            || !matches!(session.state().match_state, Match::Active { .. })
        {
            return None;
        }
        let command = play.command(&session)?;
        let order = session.resolve(&command).ok()?;
        session.apply(order, &mut entropy, &mut ()).ok()?;
    }
    if session.state().turn.active_player == *friendly
        && matches!(session.state().match_state, Match::Active { .. })
    {
        let command = Command::EndTurn {
            player: friendly.clone(),
        };
        let order = session.resolve(&command).ok()?;
        session.apply(order, &mut entropy, &mut ()).ok()?;
    }

    let mut turns = 0;
    if matches!(session.state().match_state, Match::Active { .. }) {
        greedy_turn(&mut session, Rng::mix(seed ^ REPLY_SALT), &mut entropy)?;
        turns += 1;
    }
    let leaf_value = matches!(session.state().match_state, Match::Active { .. })
        .then(|| evaluator.value(session.state(), friendly_seat));

    while turns < MAX_TURNS && matches!(session.state().match_state, Match::Active { .. }) {
        let turn_seed = Rng::mix(seed ^ REPLY_SALT ^ ((u64::from(turns)) << 32));
        greedy_turn(&mut session, turn_seed, &mut entropy)?;
        turns += 1;
    }
    let result = match &session.state().match_state {
        Match::Finished { outcome } => {
            outcome_score(outcome, &session.state().player(friendly_seat).team)
        }
        Match::Active { .. } => 0.5,
    };
    Some(LineResult {
        leaf_value,
        result,
        finished: matches!(session.state().match_state, Match::Finished { .. }),
        turns,
    })
}

fn replay_horizon(
    context: &HorizonContext<'_>,
    plays: &[Play],
    horizon: Horizon,
) -> Option<HorizonReplay> {
    let mut state = context.root.clone();
    state.settings.day_limit = Some(u64::from(context.days));
    let mut session = Session::new(state);
    let mut entropy = Rng::from_seed(Rng::mix(context.seed ^ ENTROPY_SALT));

    for play in plays {
        if session.state().turn.active_player != *context.friendly
            || !matches!(session.state().match_state, Match::Active { .. })
        {
            return None;
        }
        let command = play.command(&session)?;
        let order = session.resolve(&command).ok()?;
        session.apply(order, &mut entropy, &mut ()).ok()?;
    }
    if session.state().turn.active_player == *context.friendly
        && matches!(session.state().match_state, Match::Active { .. })
    {
        let command = Command::EndTurn {
            player: context.friendly.clone(),
        };
        let order = session.resolve(&command).ok()?;
        session.apply(order, &mut entropy, &mut ()).ok()?;
    }

    let turn_limit = horizon.turns();
    let mut turns = 0;
    while matches!(session.state().match_state, Match::Active { .. })
        && turns < MAX_TURNS
        && turn_limit.is_none_or(|limit| turns < limit)
    {
        let turn_seed = Rng::mix(context.seed ^ REPLY_SALT ^ ((u64::from(turns)) << 32));
        greedy_turn(&mut session, turn_seed, &mut entropy)?;
        turns += 1;
    }

    let terminal_result = match &session.state().match_state {
        Match::Finished { outcome } => Some(outcome_score(
            outcome,
            &session.state().player(context.friendly_seat).team,
        )),
        Match::Active { .. } => None,
    };
    Some(HorizonReplay {
        state: session.state().clone(),
        terminal_result,
    })
}

fn evaluate_horizon(
    context: &mut HorizonContext<'_>,
    plays: &[Play],
    horizon: Horizon,
) -> Option<HorizonLine> {
    let replay = replay_horizon(context, plays, horizon)?;
    let terminal_result = replay.terminal_result;
    let score = terminal_result.unwrap_or_else(|| {
        if matches!(horizon, Horizon::Terminal) {
            0.5
        } else {
            context
                .evaluator
                .value(&replay.state, context.friendly_seat)
        }
    });
    Some(HorizonLine {
        score,
        terminal_result: matches!(horizon, Horizon::Terminal)
            .then_some(terminal_result.unwrap_or(0.5)),
    })
}

fn evaluate_horizon_both(
    context: &HorizonContext<'_>,
    plays: &[Play],
    horizon: Horizon,
    standard: &mut Evaluator,
    conservative: &mut Evaluator,
) -> Option<AdaptiveLine> {
    let replay = replay_horizon(context, plays, horizon)?;
    let standard_score = replay
        .terminal_result
        .unwrap_or_else(|| standard.value(&replay.state, context.friendly_seat));
    let conservative_score = replay
        .terminal_result
        .unwrap_or_else(|| conservative.value(&replay.state, context.friendly_seat));
    Some(AdaptiveLine {
        standard_score,
        conservative_score,
    })
}

fn greedy_turn(session: &mut Session, seed: u64, entropy: &mut Rng) -> Option<()> {
    let player = session.state().turn.active_player.clone();
    let mut agent = GreedyAgent::with_weights(seed, Weights::BASELINE);
    let mut view = observe(&AwbwVisibility, session.state(), &player).ok()?;
    while session.state().turn.active_player == player
        && matches!(session.state().match_state, Match::Active { .. })
    {
        observe_into(&AwbwVisibility, session.state(), &player, &mut view).ok()?;
        let command = agent
            .act(&view, NodeBudget::ONE)
            .and_then(|play| play.command(session))
            .unwrap_or_else(|| Command::EndTurn {
                player: player.clone(),
            });
        let order = session.resolve(&command).ok()?;
        session.apply(order, entropy, &mut ()).ok()?;
    }
    Some(())
}

fn outcome_score(outcome: &Outcome, team: &TeamId) -> f64 {
    match outcome {
        Outcome::Victory { winners, .. } => f64::from(u8::from(winners.contains(team))),
        Outcome::Draw { .. } | Outcome::Cancelled { .. } => 0.5,
    }
}

fn count_unique<'a>(plans: impl Iterator<Item = &'a Vec<Play>>) -> usize {
    let mut unique: Vec<&Vec<Play>> = Vec::new();
    for plan in plans {
        if !unique.contains(&plan) {
            unique.push(plan);
        }
    }
    unique.len()
}

fn order_changes(left: &[Play], right: &[Play]) -> usize {
    (0..left.len().max(right.len()))
        .filter(|index| left.get(*index) != right.get(*index))
        .count()
}

fn percent(value: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        100.0 * value as f64 / total as f64
    }
}

fn percent_float(value: f64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        100.0 * value / total as f64
    }
}

fn report_horizon_audit(options: &Options, audit: &AuditCoverage, fit: &FitResult) {
    println!(
        "horizon evaluator audit: fit seed {}  roots analyzed {}  roots skipped {}",
        options.seed, audit.roots, audit.skipped
    );
    println!(
        "four-round selections below the terminal oracle: {} branch pairs",
        audit.pairs.len()
    );
    let fit_pairs = audit.pairs.iter().filter(|pair| pair.fit_eligible).count();
    println!(
        "positional component pairs: {}; settled at four-round boundary: {}",
        fit_pairs,
        audit.pairs.len() - fit_pairs
    );
    println!(
        "mean terminal result gap (terminal-best minus selected): {:.3}",
        audit
            .pairs
            .iter()
            .map(|pair| pair.terminal_best_result - pair.selected_result)
            .sum::<f64>()
            / audit.pairs.len().max(1) as f64
    );
    println!();

    println!("first outcome divergence");
    if audit.divergence_boundaries.is_empty() {
        println!("  no outcome divergence boundary was recorded");
    } else {
        for (boundary, count) in &audit.divergence_boundaries {
            println!("  continuation boundary {:>3}: {:>4}", boundary, count);
        }
        if audit.unresolved_divergence > 0 {
            println!(
                "  unresolved after terminal replay: {:>4}",
                audit.unresolved_divergence
            );
        }
    }
    println!();

    let mut standard_selected = [0.0; 7];
    let mut standard_terminal_best = [0.0; 7];
    let mut category_selected = [0.0; 5];
    let mut category_terminal_best = [0.0; 5];
    let mut category_correct = [0_u64; 5];
    let mut category_errors = [0_u64; 5];
    for pair in audit.pairs.iter().filter(|pair| pair.fit_eligible) {
        for (total, value) in standard_selected
            .iter_mut()
            .zip(breakdown_values(pair.selected.standard))
        {
            *total += value;
        }
        for (total, value) in standard_terminal_best
            .iter_mut()
            .zip(breakdown_values(pair.terminal_best.standard))
        {
            *total += value;
        }
        for category in AuditCategory::ALL {
            let index = category.index();
            category_selected[index] += pair.selected.categories[index];
            category_terminal_best[index] += pair.terminal_best.categories[index];
            category_correct[index] +=
                u64::from(pair.terminal_best.categories[index] > pair.selected.categories[index]);
            category_errors[index] += u64::from(pair.category.index() == index);
        }
    }
    let count = fit_pairs.max(1) as f64;
    println!("component deltas against terminal result");
    println!("  standard component       selected mean   terminal-best mean   delta");
    for (name, index) in [
        ("score", 0),
        ("army", 1),
        ("income", 2),
        ("exposure", 3),
        ("contest", 4),
        ("front", 5),
        ("other", 6),
    ] {
        println!(
            "  {:<22} {:>14.1} {:>19.1} {:+10.1}",
            name,
            standard_selected[index] / count,
            standard_terminal_best[index] / count,
            (standard_terminal_best[index] - standard_selected[index]) / count,
        );
    }
    println!();
    println!("error groups");
    println!(
        "  group                  selected mean   terminal-best mean   delta   correct direction   leading errors"
    );
    for category in AuditCategory::ALL {
        let index = category.index();
        let delta = category_terminal_best[index] - category_selected[index];
        println!(
            "  {:<22} {:>14.1} {:>19.1} {:+8.1} {:>7}/{:<7} ({:>5.1}%) {:>14}",
            category.name(),
            category_selected[index] / count,
            category_terminal_best[index] / count,
            delta / count,
            category_correct[index],
            fit_pairs,
            percent(category_correct[index], fit_pairs as u64),
            category_errors[index],
        );
    }
    println!();

    println!("branch-pair records");
    println!(
        "  root selected->best first-div category              score       army      income exposure  contest     front     other"
    );
    for pair in &audit.pairs {
        let selected = pair.selected.standard;
        let terminal_best = pair.terminal_best.standard;
        let divergence = pair
            .first_divergence
            .map_or_else(|| "none".to_owned(), |boundary| boundary.to_string());
        let state_kind = if pair.fit_eligible {
            "active"
        } else {
            "settled"
        };
        println!(
            "  {:>4} {:.1}->{:.1} {:>6} {:<20} {:<7} {:.0}/{:.0} {:>9.0}/{:<9.0} {:>9.0}/{:<9.0} {:>8.0}/{:<8.0} {:>8.0}/{:<8.0} {:>8.0}/{:<8.0} {:>8.0}/{:<8.0}",
            pair.root_index,
            pair.selected_result,
            pair.terminal_best_result,
            divergence,
            pair.category.name(),
            state_kind,
            selected.score,
            terminal_best.score,
            selected.army,
            terminal_best.army,
            selected.income,
            terminal_best.income,
            selected.exposure,
            terminal_best.exposure,
            selected.contest,
            terminal_best.contest,
            selected.front,
            terminal_best.front,
            selected.other,
            terminal_best.other,
        );
    }
    println!();

    println!("fit on positional branch pairs");
    println!(
        "  branch-pair ranking with standard weights: {:>5.1}%",
        100.0 * fit.before
    );
    println!(
        "  branch-pair ranking with fitted weights:   {:>5.1}%",
        100.0 * fit.after
    );
    println!("  fitted category multipliers");
    for category in AuditCategory::ALL {
        println!(
            "    {:<22} {:.2}",
            category.name(),
            fit.multipliers[category.index()]
        );
    }
    println!();
}

fn report_exposure_sweep(options: &Options, coverage: &ExposureSweepCoverage) {
    println!(
        "frozen exposure/front sweep: seed {}  roots requested {}  roots sampled {}  days {}",
        options.seed, options.roots, coverage.roots_sampled, options.days
    );
    println!(
        "roots measured {}  roots skipped {}  terminal-oracle improvements {:>4}/{:<4} ({:>5.1}%)",
        coverage.roots_measured,
        coverage.roots_skipped,
        coverage.terminal_oracle_improvement_roots,
        coverage.roots_measured,
        percent(
            coverage.terminal_oracle_improvement_roots,
            coverage.roots_measured
        )
    );
    println!();

    println!("pre-registered protocol");
    println!(
        "  arms: standard 1.00, conservative 0.25, disabled 0.00; only exposure and front change"
    );
    println!(
        "  primary: improvements captured = selected result > baseline on oracle-improved roots"
    );
    println!(
        "  primary: mean terminal regret = terminal oracle - selected result over all measured roots"
    );
    println!(
        "  secondary: top-plan accuracy on roots with one terminal-best candidate; pairwise ranking"
    );
    println!(
        "  tie rule: retain the first exact plan at equal evaluator score; terminal ties are excluded from top-plan accuracy"
    );
    println!(
        "  controls: exact plans are deduplicated before two- and four-round replays; baseline continuation and entropy are fixed"
    );
    println!();

    println!("candidate pool");
    println!(
        "  generated plans {:>5}, unique exact plans {:>5}, duplicates {:>5}/{:<5} ({:>5.1}%)",
        coverage.generated_candidates,
        coverage.unique_candidates,
        coverage.duplicate_candidates,
        coverage.generated_candidates,
        percent(coverage.duplicate_candidates, coverage.generated_candidates)
    );
    println!(
        "  unique plans per measured root {:.2}; terminal replay is shared by all three arms",
        coverage.unique_candidates as f64 / coverage.roots_measured.max(1) as f64
    );
    println!();

    println!("selection results");
    println!(
        "  arm            horizon       top accuracy       captured       regret       pairwise       ms/unique"
    );
    for (arm_index, arm) in coverage.arms.iter().enumerate() {
        for (horizon_index, metrics) in arm.horizons.iter().enumerate() {
            let horizon = Horizon::SELECTION[horizon_index];
            let milliseconds_per_candidate = if coverage.unique_candidates == 0 {
                0.0
            } else {
                1_000.0 * metrics.runtime.as_secs_f64() / coverage.unique_candidates as f64
            };
            let mean_regret = if metrics.selection_roots == 0 {
                0.0
            } else {
                metrics.regret / metrics.selection_roots as f64
            };
            println!(
                "  {:<14} {:<12} {:>4}/{:<4} ({:>5.1}%) {:>4}/{:<4} ({:>5.1}%) {:>8.3} {:>5.1}% ({:<5}) {:>9.2}",
                arm.arm.name(),
                horizon.name(),
                metrics.top_plan_hits,
                metrics.non_tied_roots,
                percent(metrics.top_plan_hits, metrics.non_tied_roots),
                metrics.improvements_captured,
                metrics.oracle_improvement_roots,
                percent(
                    metrics.improvements_captured,
                    metrics.oracle_improvement_roots
                ),
                mean_regret,
                percent_float(metrics.pairwise_score, metrics.pairwise_pairs),
                metrics.pairwise_pairs,
                milliseconds_per_candidate,
            );
        }
        if arm_index + 1 != coverage.arms.len() {
            println!();
        }
    }
    println!();

    let standard = &coverage.arms[ExposureFrontArm::Standard as usize];
    let conservative = &coverage.arms[ExposureFrontArm::Conservative as usize];
    let four_standard = &standard.horizons[1];
    let four_conservative = &conservative.horizons[1];
    let conservative_confirms = four_conservative.improvements_captured
        > four_standard.improvements_captured
        && mean_regret(four_conservative) < mean_regret(four_standard);
    println!("promotion gate");
    println!(
        "  conservative four-round capture {} vs standard {}; regret {:.3} vs {:.3}",
        four_conservative.improvements_captured,
        four_standard.improvements_captured,
        mean_regret(four_conservative),
        mean_regret(four_standard)
    );
    println!(
        "  conservative four-round result: {}",
        if conservative_confirms {
            "passes the pre-registered promotion gate"
        } else {
            "does not pass the pre-registered promotion gate"
        }
    );
}

fn report_adaptive_horizon(options: &Options, coverage: &AdaptiveCoverage) {
    println!(
        "adaptive horizon: seed {}  roots requested {}  roots sampled {}  days {}",
        options.seed, options.roots, coverage.roots_sampled, options.days
    );
    println!(
        "roots measured {}  roots skipped {}  terminal-oracle improvements {:>4}/{:<4} ({:>5.1}%)",
        coverage.roots_measured,
        coverage.roots_skipped,
        coverage.terminal_oracle_improvement_roots,
        coverage.roots_measured,
        percent(
            coverage.terminal_oracle_improvement_roots,
            coverage.roots_measured
        )
    );
    println!();

    println!("pre-registered adaptive protocol");
    println!("  score every unique plan at 4 rounds with standard and conservative evaluators");
    println!("  accept the 4-round plan when both evaluators select the same plan");
    println!("  on disagreement, extend only the union of their top two plans to 8 rounds");
    println!(
        "  select an extended plan by the mean of its standard and conservative scores; ties keep first"
    );
    println!(
        "  promotion gate: adaptive capture must exceed standard 4-round capture and regret must not increase"
    );
    println!(
        "  controls: exact plans are deduplicated before replay; continuations and entropy are fixed"
    );
    println!();

    println!("candidate pool");
    println!(
        "  generated plans {:>5}, unique exact plans {:>5}, duplicates {:>5}/{:<5} ({:>5.1}%)",
        coverage.generated_candidates,
        coverage.unique_candidates,
        coverage.duplicate_candidates,
        coverage.generated_candidates,
        percent(coverage.duplicate_candidates, coverage.generated_candidates)
    );
    println!(
        "  unique plans per measured root {:.2}; exact deduplication precedes all horizon replays",
        coverage.unique_candidates as f64 / coverage.roots_measured.max(1) as f64
    );
    println!();

    println!("selection results");
    println!(
        "  policy             horizon       top accuracy       captured       regret       ms/root"
    );
    report_adaptive_line("standard", &coverage.standard_four, coverage.roots_measured);
    report_adaptive_metrics_line("adaptive", &coverage.adaptive);
    report_adaptive_line(
        "always-8 standard",
        &coverage.always_eight,
        coverage.roots_measured,
    );
    println!();

    let standard_four_capture = coverage.standard_four.improvements_captured;
    let adaptive_capture = coverage.adaptive.improvements_captured;
    let standard_four_regret = mean_regret(&coverage.standard_four);
    let adaptive_regret = if coverage.adaptive.selection_roots == 0 {
        0.0
    } else {
        coverage.adaptive.regret / coverage.adaptive.selection_roots as f64
    };
    let passes =
        adaptive_capture > standard_four_capture && adaptive_regret <= standard_four_regret;
    println!("uncertainty and cost");
    println!(
        "  evaluator disagreement {:>4}/{:<4} ({:>5.1}%)",
        coverage.adaptive.uncertain_roots,
        coverage.roots_measured,
        percent(coverage.adaptive.uncertain_roots, coverage.roots_measured)
    );
    println!(
        "  adaptive replays per root: 4-round {:.2}, additional 8-round {:.2}, total {:.2}",
        coverage.adaptive.four_round_replays as f64 / coverage.roots_measured.max(1) as f64,
        coverage.adaptive.eight_round_replays as f64 / coverage.roots_measured.max(1) as f64,
        (coverage.adaptive.four_round_replays + coverage.adaptive.eight_round_replays) as f64
            / coverage.roots_measured.max(1) as f64
    );
    println!(
        "  adaptive runtime per root {:.2} ms; additional 8-round runtime per root {:.2} ms",
        1_000.0 * coverage.adaptive.runtime.as_secs_f64() / coverage.roots_measured.max(1) as f64,
        1_000.0 * coverage.adaptive.eight_round_runtime.as_secs_f64()
            / coverage.roots_measured.max(1) as f64
    );
    println!(
        "  always-8 standard runtime per root {:.2} ms; standard 4-round runtime per root {:.2} ms",
        1_000.0 * coverage.always_eight.runtime.as_secs_f64()
            / coverage.roots_measured.max(1) as f64,
        1_000.0 * coverage.standard_four.runtime.as_secs_f64()
            / coverage.roots_measured.max(1) as f64
    );
    println!();
    println!(
        "promotion gate: adaptive capture {} vs standard {}; regret {:.3} vs {:.3}; {}",
        adaptive_capture,
        standard_four_capture,
        adaptive_regret,
        standard_four_regret,
        if passes { "passes" } else { "does not pass" }
    );
}

fn report_adaptive_line(name: &str, metrics: &HorizonMetrics, roots: u64) {
    let runtime = 1_000.0 * metrics.runtime.as_secs_f64() / roots.max(1) as f64;
    println!(
        "  {:<18} {:<12} {:>4}/{:<4} ({:>5.1}%) {:>4}/{:<4} ({:>5.1}%) {:>8.3} {:>9.2}",
        name,
        if name == "always-8 standard" {
            "8 rounds"
        } else if name == "standard" {
            "4 rounds"
        } else {
            "adaptive"
        },
        metrics.top_plan_hits,
        metrics.non_tied_roots,
        percent(metrics.top_plan_hits, metrics.non_tied_roots),
        metrics.improvements_captured,
        metrics.oracle_improvement_roots,
        percent(
            metrics.improvements_captured,
            metrics.oracle_improvement_roots
        ),
        mean_regret(metrics),
        runtime,
    );
}

fn report_adaptive_metrics_line(name: &str, metrics: &AdaptiveMetrics) {
    println!(
        "  {:<18} {:<12} {:>4}/{:<4} ({:>5.1}%) {:>4}/{:<4} ({:>5.1}%) {:>8.3} {:>9.2}",
        name,
        "adaptive",
        metrics.top_plan_hits,
        metrics.non_tied_roots,
        percent(metrics.top_plan_hits, metrics.non_tied_roots),
        metrics.improvements_captured,
        metrics.oracle_improvement_roots,
        percent(
            metrics.improvements_captured,
            metrics.oracle_improvement_roots
        ),
        metrics.regret / metrics.selection_roots.max(1) as f64,
        1_000.0 * metrics.runtime.as_secs_f64() / metrics.selection_roots.max(1) as f64,
    );
}

fn mean_regret(metrics: &HorizonMetrics) -> f64 {
    if metrics.selection_roots == 0 {
        0.0
    } else {
        metrics.regret / metrics.selection_roots as f64
    }
}

fn breakdown_values(breakdown: EvalBreakdown) -> [f64; 7] {
    [
        breakdown.score,
        breakdown.army,
        breakdown.income,
        breakdown.exposure,
        breakdown.contest,
        breakdown.front,
        breakdown.other,
    ]
}

fn report_horizon_sweep(options: &Options, coverage: &HorizonCoverage) {
    report_horizon_sweep_named(options, coverage, "horizon sweep");
}

fn report_horizon_sweep_named(options: &Options, coverage: &HorizonCoverage, label: &str) {
    println!(
        "{label}: seed {}  roots requested {}  roots sampled {}  days {}",
        options.seed, options.roots, coverage.roots_sampled, options.days
    );
    println!(
        "roots measured {}  roots skipped {}",
        coverage.roots_measured, coverage.roots_skipped
    );
    println!();

    println!("candidate pool");
    println!(
        "  generated plans {:>5}, unique exact plans {:>5}, duplicates {:>5}/{:<5} ({:>5.1}%)",
        coverage.generated_candidates,
        coverage.unique_candidates,
        coverage.duplicate_candidates,
        coverage.generated_candidates,
        percent(coverage.duplicate_candidates, coverage.generated_candidates)
    );
    println!(
        "  unique plans per measured root {:.2}; deduplication runs before horizon evaluation",
        coverage.unique_candidates as f64 / coverage.roots_measured.max(1) as f64
    );
    println!(
        "  terminal oracle improves baseline {:>4}/{:<4} ({:>5.1}%)",
        coverage.terminal_oracle_improvement_roots,
        coverage.roots_measured,
        percent(
            coverage.terminal_oracle_improvement_roots,
            coverage.roots_measured
        )
    );
    println!();

    println!("horizon results");
    println!(
        "  horizon       evaluated     ms/unique   top final result   selected improvements   captured    mean regret   pairwise ranking"
    );
    for horizon_index in 0..Horizon::ALL.len() {
        let horizon = Horizon::ALL[horizon_index];
        let metrics = &coverage.horizons[horizon_index];
        let milliseconds_per_candidate = if coverage.unique_candidates == 0 {
            0.0
        } else {
            1_000.0 * metrics.runtime.as_secs_f64() / coverage.unique_candidates as f64
        };
        let mean_regret = if metrics.selection_roots == 0 {
            0.0
        } else {
            metrics.regret / metrics.selection_roots as f64
        };
        println!(
            "  {:<12} {:>5}       {:>7.2}   {:>4}/{:<4} ({:>5.1}%)   {:>4}/{:<4} ({:>5.1}%)       {:>4}/{:<4} ({:>5.1}%)   {:>8.3}      {:>5.1}% ({:<5})",
            horizon.name(),
            metrics.evaluated_candidates,
            milliseconds_per_candidate,
            metrics.top_plan_hits,
            metrics.non_tied_roots,
            percent(metrics.top_plan_hits, metrics.non_tied_roots),
            metrics.selected_improvement_roots,
            metrics.selection_roots,
            percent(metrics.selected_improvement_roots, metrics.selection_roots),
            metrics.improvements_captured,
            metrics.oracle_improvement_roots,
            percent(
                metrics.improvements_captured,
                metrics.oracle_improvement_roots
            ),
            mean_regret,
            percent_float(metrics.pairwise_score, metrics.pairwise_pairs),
            metrics.pairwise_pairs,
        );
    }
    println!();
    println!(
        "  runtime is horizon evaluation only; each replay starts the same entropy stream and uses baseline turns at complete turn boundaries"
    );
}

fn report(options: &Options, coverage: &Coverage) {
    println!(
        "portfolio diagnostic: seed {}  roots requested {}  roots sampled {}  days {}",
        options.seed, options.roots, coverage.roots_sampled, options.days
    );
    println!(
        "roots measured {}  roots skipped {}  counterfactual roots {}",
        coverage.roots_measured, coverage.roots_skipped, coverage.counterfactual_roots
    );
    println!();

    println!("portfolio disagreement");
    println!(
        "  roots with any script different from baseline {:>4}/{:<4} ({:>5.1}%)",
        coverage.roots_with_baseline_disagreement,
        coverage.roots_measured,
        percent(
            coverage.roots_with_baseline_disagreement,
            coverage.roots_measured
        )
    );
    println!(
        "  roots with different portfolio plans             {:>4}/{:<4} ({:>5.1}%)",
        coverage.roots_with_portfolio_disagreement,
        coverage.roots_measured,
        percent(
            coverage.roots_with_portfolio_disagreement,
            coverage.roots_measured
        )
    );
    println!(
        "  unique portfolio plans per root                 {:.2}",
        coverage.unique_plans as f64 / coverage.roots_measured.max(1) as f64
    );
    println!(
        "  pairwise plan disagreements                      {:>4}/{:<4} ({:>5.1}%)",
        coverage.pairwise_plan_disagreements,
        coverage.pairwise_plan_comparisons,
        percent(
            coverage.pairwise_plan_disagreements,
            coverage.pairwise_plan_comparisons
        )
    );
    println!();

    println!("counterfactual coverage");
    println!(
        "  any script beats baseline at non-terminal leaf     {:>4}/{:<4} ({:>5.1}%)",
        coverage.roots_with_leaf_coverage,
        coverage.leaf_comparison_roots,
        percent(
            coverage.roots_with_leaf_coverage,
            coverage.leaf_comparison_roots
        )
    );
    println!(
        "  any script improves the final result               {:>4}/{:<4} ({:>5.1}%)",
        coverage.roots_with_result_coverage,
        coverage.counterfactual_roots,
        percent(
            coverage.roots_with_result_coverage,
            coverage.counterfactual_roots
        )
    );
    println!(
        "  mean best-minus-baseline leaf value             {:+.1}",
        coverage.best_leaf_delta / coverage.leaf_comparison_roots.max(1) as f64
    );
    println!(
        "  mean best-minus-baseline final result           {:+.3}",
        coverage.best_result_delta / coverage.counterfactual_roots.max(1) as f64
    );
    println!(
        "  finished lines {:>4}/{:<4} ({:>5.1}%), mean forward turns {:.1}",
        coverage.finished_lines,
        coverage.total_lines,
        percent(coverage.finished_lines, coverage.total_lines),
        coverage.total_turns as f64 / coverage.total_lines.max(1) as f64
    );
    println!();

    println!("stratified coordinate sweep");
    println!(
        "  default assignment differs from baseline                  {:>4}/{:<4} ({:>5.1}%)",
        coverage.roots_with_default_stratified_disagreement,
        coverage.roots_measured,
        percent(
            coverage.roots_with_default_stratified_disagreement,
            coverage.roots_measured
        )
    );
    println!(
        "  evaluator-selected assignment differs from baseline       {:>4}/{:<4} ({:>5.1}%)",
        coverage.roots_with_selected_stratified_disagreement,
        coverage.counterfactual_roots,
        percent(
            coverage.roots_with_selected_stratified_disagreement,
            coverage.counterfactual_roots
        )
    );
    println!(
        "  any stratified candidate differs from baseline             {:>4}/{:<4} ({:>5.1}%)",
        coverage.roots_with_any_stratified_disagreement,
        coverage.counterfactual_roots,
        percent(
            coverage.roots_with_any_stratified_disagreement,
            coverage.counterfactual_roots
        )
    );
    println!(
        "  unique stratified plans per root                           {:.2}",
        coverage.stratified_unique_plans as f64 / coverage.counterfactual_roots.max(1) as f64
    );
    println!(
        "  candidate plans generated                                  {:>4} (max {} per root)",
        coverage.stratified_candidates_generated, STRATIFIED_CANDIDATE_LIMIT
    );
    println!(
        "  candidate plans evaluated                                   {:>4}/{:<4} ({:>5.1}%)",
        coverage.stratified_candidates_evaluated,
        coverage.stratified_candidates_generated,
        percent(
            coverage.stratified_candidates_evaluated,
            coverage.stratified_candidates_generated
        )
    );
    println!(
        "  duplicate candidate plans                                  {:>4}/{:<4} ({:>5.1}%)",
        coverage.stratified_duplicate_candidates,
        coverage.stratified_candidates_generated,
        percent(
            coverage.stratified_duplicate_candidates,
            coverage.stratified_candidates_generated
        )
    );
    println!(
        "  evaluated nodes per root                                   {:.2}",
        coverage.evaluated_nodes as f64 / coverage.counterfactual_roots.max(1) as f64
    );
    println!();

    println!("oracle coverage");
    println!("  family                              leaf oracle       result oracle");
    println!(
        "  best whole-script candidate          {:>4}/{:<4} ({:>5.1}%) {:>4}/{:<4} ({:>5.1}%)",
        coverage.roots_with_leaf_coverage,
        coverage.leaf_comparison_roots,
        percent(
            coverage.roots_with_leaf_coverage,
            coverage.leaf_comparison_roots
        ),
        coverage.roots_with_result_coverage,
        coverage.counterfactual_roots,
        percent(
            coverage.roots_with_result_coverage,
            coverage.counterfactual_roots
        ),
    );
    println!(
        "  stratified oracle                       {:>4}/{:<4} ({:>5.1}%) {:>4}/{:<4} ({:>5.1}%)",
        coverage.roots_with_stratified_leaf_coverage,
        coverage.stratified_leaf_comparison_roots,
        percent(
            coverage.roots_with_stratified_leaf_coverage,
            coverage.stratified_leaf_comparison_roots
        ),
        coverage.roots_with_stratified_result_coverage,
        coverage.stratified_result_comparison_roots,
        percent(
            coverage.roots_with_stratified_result_coverage,
            coverage.stratified_result_comparison_roots
        ),
    );
    println!(
        "  evaluator-selected stratified           {:>4}/{:<4} ({:>5.1}%) {:>4}/{:<4} ({:>5.1}%)",
        coverage.roots_with_stratified_selected_leaf_coverage,
        coverage.selected_leaf_comparison_roots,
        percent(
            coverage.roots_with_stratified_selected_leaf_coverage,
            coverage.selected_leaf_comparison_roots
        ),
        coverage.roots_with_stratified_selected_result_coverage,
        coverage.evaluator_result_selection_roots,
        percent(
            coverage.roots_with_stratified_selected_result_coverage,
            coverage.evaluator_result_selection_roots
        ),
    );
    println!(
        "  coverage unique to stratification        {:>4}/{:<4} ({:>5.1}%) {:>4}/{:<4} ({:>5.1}%)",
        coverage.roots_with_stratified_leaf_coverage_unique,
        coverage.stratified_leaf_comparison_roots,
        percent(
            coverage.roots_with_stratified_leaf_coverage_unique,
            coverage.stratified_leaf_comparison_roots
        ),
        coverage.roots_with_stratified_result_coverage_unique,
        coverage.stratified_result_comparison_roots,
        percent(
            coverage.roots_with_stratified_result_coverage_unique,
            coverage.stratified_result_comparison_roots
        ),
    );
    println!();

    println!("best-plan comparison");
    println!(
        "  mean best-minus-baseline leaf: whole-script {:+.1}, stratified {:+.1}",
        coverage.whole_leaf_delta / coverage.leaf_comparison_roots.max(1) as f64,
        coverage.stratified_leaf_delta / coverage.stratified_leaf_comparison_roots.max(1) as f64,
    );
    println!(
        "  mean best-minus-baseline result: whole-script {:+.3}, stratified {:+.3}",
        coverage.whole_result_delta / coverage.counterfactual_roots.max(1) as f64,
        coverage.stratified_result_delta
            / coverage.stratified_result_comparison_roots.max(1) as f64,
    );
    println!(
        "  stratified beats whole-script at leaf   {:>4}/{:<4} ({:>5.1}%), mean gap {:+.1}",
        coverage.stratified_beats_whole_leaf,
        coverage.stratified_leaf_comparison_roots,
        percent(
            coverage.stratified_beats_whole_leaf,
            coverage.stratified_leaf_comparison_roots
        ),
        coverage.stratified_leaf_over_whole_delta
            / coverage.stratified_leaf_comparison_roots.max(1) as f64,
    );
    println!(
        "  stratified beats whole-script at result {:>4}/{:<4} ({:>5.1}%), mean gap {:+.3}",
        coverage.stratified_beats_whole_result,
        coverage.stratified_result_comparison_roots,
        percent(
            coverage.stratified_beats_whole_result,
            coverage.stratified_result_comparison_roots
        ),
        coverage.stratified_result_over_whole_delta
            / coverage.stratified_result_comparison_roots.max(1) as f64,
    );
    println!();

    println!("evaluator selection accuracy");
    println!(
        "  selected leaf equals stratified leaf oracle   {:>4}/{:<4} ({:>5.1}%)",
        coverage.evaluator_leaf_oracle_hits,
        coverage.evaluator_selection_roots,
        percent(
            coverage.evaluator_leaf_oracle_hits,
            coverage.evaluator_selection_roots
        )
    );
    println!(
        "  selected result equals stratified result oracle {:>4}/{:<4} ({:>5.1}%)",
        coverage.evaluator_result_oracle_hits,
        coverage.evaluator_result_selection_roots,
        percent(
            coverage.evaluator_result_oracle_hits,
            coverage.evaluator_result_selection_roots
        )
    );
    println!();

    println!("mission adherence and capture completion");
    println!("  plan                              preserved missions       completed captures");
    print_mission_quality("baseline", coverage.baseline_missions);
    print_mission_quality("best whole-script", coverage.best_whole_missions);
    print_mission_quality("stratified oracle", coverage.stratified_oracle_missions);
    print_mission_quality(
        "evaluator-selected stratified",
        coverage.selected_stratified_missions,
    );
    print_mission_quality(
        "all stratified candidates",
        coverage.stratified_candidate_missions,
    );
    println!();

    println!("stratified contribution by stratum and script");
    println!(
        "  stratum   script                 generated eval dup selected leaf>base result>base missions preserved captures"
    );
    for stratum in &coverage.stratified_scripts {
        for stats in stratum {
            println!(
                "  {:<9} {:<22} {:>8} {:>4} {:>3} {:>8} {:>10} {:>11} {:>8}/{:<8} {:>9} ",
                stats.stratum.name(),
                stats.script.name(),
                stats.generated,
                stats.evaluated,
                stats.duplicate,
                stats.selected,
                stats.leaf_better,
                stats.result_better,
                stats.mission_preserved,
                stats.mission_total,
                stats.capture_completed,
            );
        }
    }
    println!();

    println!("measurement cost");
    println!(
        "  measurement runtime {:.3}s, {:.3}s per measured root",
        coverage.runtime.as_secs_f64(),
        coverage.runtime.as_secs_f64() / coverage.roots_measured.max(1) as f64,
    );
    println!(
        "  evaluated stratified nodes {:>4}, {:.2} per measured root",
        coverage.evaluated_nodes,
        coverage.evaluated_nodes as f64 / coverage.roots_measured.max(1) as f64,
    );
    println!();

    println!("per-script coverage against baseline");
    println!(
        "  script                 changed plans   order changes   leaf better   result better"
    );
    for stats in &coverage.scripts {
        println!(
            "  {:<22} {:>5}/{:<5} ({:>4.1}%) {:>6}/{:<6} ({:>4.1}%) {:>7}/{:<7} ({:>4.1}%) {:>7}/{:<7} ({:>4.1}%)",
            stats.script.name(),
            stats.changed_plans,
            stats.plans,
            percent(stats.changed_plans, stats.plans),
            stats.order_changes,
            stats.order_slots,
            percent(stats.order_changes, stats.order_slots),
            stats.leaf_better,
            stats.leaf_lines,
            percent(stats.leaf_better, stats.leaf_lines),
            stats.result_better,
            stats.lines,
            percent(stats.result_better, stats.lines),
        );
    }
}

fn print_mission_quality(label: &str, quality: MissionQuality) {
    println!(
        "  {:<32} {:>6}/{:<6} ({:>5.1}%) {:>8}/{:<8} ({:>5.1}%)",
        label,
        quality.preserved,
        quality.total,
        percent(quality.preserved, quality.total),
        quality.completed,
        quality.total,
        percent(quality.completed, quality.total),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sampled_root() -> SampledRoot {
        let state = arena(false, 1);
        let view = observe(&AwbwVisibility, &state, &state.turn.active_player)
            .expect("the active player observes the arena");
        SampledRoot {
            state,
            view,
            seed: 17,
        }
    }

    fn end_turn(session: &mut Session, entropy: &mut Rng) {
        let player = session.state().turn.active_player.clone();
        let order = session
            .resolve(&Command::EndTurn { player })
            .expect("the active player can end the turn");
        session
            .apply(order, entropy, &mut ())
            .expect("the end turn applies");
    }

    #[test]
    fn order_changes_count_added_and_replaced_orders() {
        let root = sampled_root();
        let baseline =
            generate_plan(&root.view, root.seed, Weights::BASELINE).expect("baseline makes a turn");
        let mut changed = baseline.clone();
        changed.pop();
        assert_eq!(order_changes(&baseline, &changed), 1);
    }

    #[test]
    fn portfolio_measurement_is_repeatable() {
        let root = sampled_root();
        let mut first = Coverage::new(1);
        first.measure(&root, 2);
        let mut second = Coverage::new(1);
        second.measure(&root, 2);

        assert_eq!(first.roots_measured, second.roots_measured);
        assert_eq!(first.unique_plans, second.unique_plans);
        assert_eq!(
            first.pairwise_plan_disagreements,
            second.pairwise_plan_disagreements
        );
        assert_eq!(first.best_leaf_delta, second.best_leaf_delta);
        assert_eq!(first.best_result_delta, second.best_result_delta);
        assert_eq!(
            first.stratified_candidates_generated,
            second.stratified_candidates_generated
        );
        assert_eq!(
            first.stratified_candidates_evaluated,
            second.stratified_candidates_evaluated
        );
        assert_eq!(
            first.stratified_duplicate_candidates,
            second.stratified_duplicate_candidates
        );
    }

    #[test]
    fn coordinate_sweep_keeps_the_sixteen_candidate_limit() {
        let root = sampled_root();
        let mut coverage = Coverage::new(1);
        coverage.measure(&root, 2);

        assert!(coverage.stratified_candidates_generated <= STRATIFIED_CANDIDATE_LIMIT as u64);
        assert!(coverage.stratified_candidates_evaluated <= STRATIFIED_CANDIDATE_LIMIT as u64);
        assert!(
            coverage.stratified_duplicate_candidates <= coverage.stratified_candidates_generated
        );
    }

    #[test]
    fn arena_pool_starts_with_the_complete_baseline_turn() {
        let root = sampled_root();
        let baseline =
            generate_plan(&root.view, root.seed, Weights::BASELINE).expect("baseline makes a turn");
        let mut missions = MissionBook::new();
        let plans = generate_arena_sweep_plans(&root, 2, &mut missions)
            .expect("arena candidates make a pool");

        assert_eq!(
            plans.first().expect("baseline is candidate zero").plays,
            baseline
        );
        assert_eq!(
            deduplicate_sweep_plans(plans)
                .first()
                .expect("baseline remains first")
                .plays,
            baseline
        );
    }

    #[test]
    fn arena_agent_replans_on_two_friendly_turns_and_keeps_missions() {
        let mut session = Session::new(arena(false, 1));
        let mut entropy = Rng::from_seed(29);
        end_turn(&mut session, &mut entropy);
        let first_view = observe(
            &AwbwVisibility,
            session.state(),
            &session.state().turn.active_player,
        )
        .expect("the first friendly root observes");
        let mut agent = StratifiedArenaAgent::new(41, 2, StratifiedArenaPolicy::StandardFour);

        assert!(agent.act(&first_view, NodeBudget::ONE).is_some());
        while agent.act(&first_view, NodeBudget::ONE).is_some() {}
        assert_eq!(agent.selection_turns, 1);
        let first_mission = agent
            .missions
            .capture_missions()
            .first()
            .copied()
            .expect("the first friendly root assigns a capture mission");

        end_turn(&mut session, &mut entropy);
        end_turn(&mut session, &mut entropy);
        let second_view = observe(
            &AwbwVisibility,
            session.state(),
            &session.state().turn.active_player,
        )
        .expect("the second friendly root observes");

        assert!(agent.act(&second_view, NodeBudget::ONE).is_some());
        assert_eq!(agent.selection_turns, 2);
        assert_eq!(
            agent
                .missions
                .capture_mission(first_mission.unit)
                .map(|mission| mission.property),
            Some(first_mission.property)
        );
    }

    #[test]
    fn equal_leaf_values_keep_the_current_assignment() {
        let current = StratifiedScripts::default();
        let candidate = StratifiedCandidate {
            stratum: Stratum::Objective,
            assignment: current.with_script(Stratum::Objective, Script::FavorableCombat),
            plays: Vec::new(),
            line: LineResult {
                leaf_value: Some(10.0),
                result: 1.0,
                finished: false,
                turns: 0,
            },
            mission: MissionQuality::default(),
        };
        let best = StratifiedCandidate {
            stratum: Stratum::Objective,
            assignment: current,
            plays: Vec::new(),
            line: LineResult {
                leaf_value: Some(10.0),
                result: 0.0,
                finished: false,
                turns: 0,
            },
            mission: MissionQuality::default(),
        };

        assert!(!selection_is_better(&candidate, &best, current));
    }

    #[test]
    fn horizon_turn_counts_end_at_complete_boundaries() {
        assert_eq!(Horizon::Reply.turns(), Some(1));
        assert_eq!(Horizon::OneRound.turns(), Some(2));
        assert_eq!(Horizon::TwoRounds.turns(), Some(4));
        assert_eq!(Horizon::FourRounds.turns(), Some(8));
        assert_eq!(Horizon::EightRounds.turns(), Some(16));
        assert_eq!(Horizon::Terminal.turns(), None);
    }

    #[test]
    fn exposure_front_arms_change_only_the_frozen_terms() {
        let standard = EvalWeights::STANDARD;
        for arm in ExposureFrontArm::ALL {
            let weights = arm.weights();
            assert_eq!(weights.army, standard.army);
            assert_eq!(weights.bank, standard.bank);
            assert_eq!(weights.income_days, standard.income_days);
            assert_eq!(weights.income_decay, standard.income_decay);
            assert_eq!(weights.plurality, standard.plurality);
            assert_eq!(weights.production, standard.production);
            assert_eq!(weights.hq, standard.hq);
            assert_eq!(weights.capture, standard.capture);
            assert_eq!(weights.contest, standard.contest);
            assert_eq!(weights.temperature, standard.temperature);
            assert_eq!(weights.exposure, standard.exposure * arm.multiplier());
            assert_eq!(weights.front, standard.front * arm.multiplier());
        }
    }

    #[test]
    fn adaptive_top_two_and_joint_score_are_deterministic() {
        let lines = vec![
            Some(AdaptiveLine {
                standard_score: 30.0,
                conservative_score: 20.0,
            }),
            Some(AdaptiveLine {
                standard_score: 10.0,
                conservative_score: 40.0,
            }),
            Some(AdaptiveLine {
                standard_score: 20.0,
                conservative_score: 30.0,
            }),
        ];
        assert_eq!(top_two_adaptive(&lines, true), vec![0, 2]);
        assert_eq!(top_two_adaptive(&lines, false), vec![1, 2]);
        assert_eq!(joint_score(lines[0].expect("line")), 25.0);
    }

    #[test]
    fn horizon_pool_deduplicates_exact_play_sequences() {
        let first = SweepPlan { plays: Vec::new() };
        let same = SweepPlan { plays: Vec::new() };
        let different = SweepPlan {
            plays: vec![Play::unitless(
                awvm::semantic::CellIdx::from_raw(1),
                awvm::session::OrderKind::EndTurn,
            )],
        };

        assert_eq!(
            deduplicate_sweep_plans(vec![first, same, different]).len(),
            2
        );
    }

    #[test]
    fn horizon_measurement_is_repeatable() {
        let root = sampled_root();
        let mut first = HorizonCoverage::new(1);
        measure_horizon_root(&mut first, &root, 2);
        let mut second = HorizonCoverage::new(1);
        measure_horizon_root(&mut second, &root, 2);

        assert_eq!(first.roots_measured, second.roots_measured);
        assert_eq!(first.generated_candidates, second.generated_candidates);
        assert_eq!(first.unique_candidates, second.unique_candidates);
        assert_eq!(first.duplicate_candidates, second.duplicate_candidates);
        for (left, right) in first.horizons.iter().zip(second.horizons.iter()) {
            assert_eq!(left.evaluated_candidates, right.evaluated_candidates);
            assert_eq!(left.selection_roots, right.selection_roots);
            assert_eq!(left.non_tied_roots, right.non_tied_roots);
            assert_eq!(left.top_plan_hits, right.top_plan_hits);
            assert_eq!(
                left.selected_improvement_roots,
                right.selected_improvement_roots
            );
            assert_eq!(
                left.oracle_improvement_roots,
                right.oracle_improvement_roots
            );
            assert_eq!(left.improvements_captured, right.improvements_captured);
            assert_eq!(left.regret, right.regret);
            assert_eq!(left.pairwise_score, right.pairwise_score);
            assert_eq!(left.pairwise_pairs, right.pairwise_pairs);
        }
    }
}
