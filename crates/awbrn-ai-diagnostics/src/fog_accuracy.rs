//! Compare the fog memory with the true state in played fog matches.
//!
//! The diagnostic plays the matches of an SPRT plan with fog on. Each seat
//! has a fog memory that reads the same views as the agent of that seat. At
//! the start of each turn, the diagnostic compares the memory of the active
//! seat with the true state. The true state is used only for this score.

use std::fs;
use std::path::Path;

use awbrn_ai::agent::{Agent, NodeBudget};
use awbrn_ai::baseline::BaselineConfig;
use awbrn_ai::fog::{FogAccuracy, FogMemory};
use awbrn_ai::harness::{Limits, play_observed};
use awbrn_ai::rng::Rng;
use awvm::semantic::{AwbwVisibility, State, observe};
use awvm::session::Session;
use serde::{Deserialize, Serialize};

use crate::sprt::{SprtError, plan_registry, read_sprt_plan};
use crate::tournament::match_agent_seeds;

/// One comparison of the memory of one seat with the true state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FogAccuracyRow {
    pub map_id: u32,
    pub game: usize,
    pub seat: usize,
    /// Starting funds and all true income of the enemy.
    pub true_income: f64,
    /// All true spending of the enemy.
    pub true_spent: f64,
    #[serde(flatten)]
    pub accuracy: FogAccuracy,
}

/// The mean errors of the samples of one range of days.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FogAccuracySummary {
    pub days: String,
    pub samples: usize,
    /// Mean true enemy army value, in funds.
    pub true_value: f64,
    /// Mean of the ledger estimate less the true value.
    pub ledger_error: f64,
    pub ledger_absolute_error: f64,
    /// The same for the old rule: the enemy army equals ours now.
    pub mirror_error: f64,
    pub mirror_absolute_error: f64,
    pub true_properties: f64,
    /// Mean of the property estimate less the true count.
    pub property_error: f64,
    pub property_absolute_error: f64,
    /// The same for the properties seen as enemy-owned.
    pub known_property_error: f64,
    pub known_property_absolute_error: f64,
    /// Mean tracks for each sample.
    pub tracks: f64,
    /// The share of tracks with a hidden unit of the same kind in the area.
    pub track_precision: f64,
    /// Tracked value as a share of the true hidden value.
    pub tracked_share: f64,
}

/// The result of a fog accuracy run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FogAccuracyResult {
    pub run_id: String,
    pub games: usize,
    pub summaries: Vec<FogAccuracySummary>,
    pub rows: Vec<FogAccuracyRow>,
}

/// Day ranges of the summaries, inclusive.
const RANGES: [(u64, u64); 5] = [(1, 5), (6, 10), (11, 20), (21, u64::MAX), (1, u64::MAX)];

/// Play `games` fog matches of an SPRT plan and score the fog memory.
///
/// Game `n` uses map `n` of the plan in turn. The candidate takes the first
/// seat in even games and the second seat in odd games.
pub fn run_fog_accuracy(
    plan_path: impl AsRef<Path>,
    output: impl AsRef<Path>,
    games: usize,
    jobs: usize,
) -> Result<FogAccuracyResult, SprtError> {
    let plan_path = plan_path.as_ref();
    let plan = read_sprt_plan(plan_path)?;
    let registry = plan_registry(&plan, plan_path)?;
    let (candidate, _) = plan.candidate.materialize(plan_path)?;
    let (baseline, _) = plan.baseline.materialize(plan_path)?;
    let maps = plan
        .maps
        .iter()
        .map(|id| {
            registry
                .get(*id)
                .ok_or_else(|| SprtError::Configuration(format!("map {id} is not loaded")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if maps.is_empty() || games == 0 {
        return Err(SprtError::Configuration(
            "a fog accuracy run needs at least one map and one game".into(),
        ));
    }
    let limits = Limits {
        nodes: NodeBudget::new(plan.limits.node_budget)
            .ok_or_else(|| SprtError::Configuration("node budget must be nonzero".into()))?,
        days: plan.limits.day_limit,
        refusals: plan.limits.refusal_limit,
    };

    let play =
        |game: usize| -> Result<Vec<FogAccuracyRow>, SprtError> {
            let map = maps[game % maps.len()];
            let match_seed = Rng::mix(plan.run_seed ^ (u64::from(map.id) << 32) ^ game as u64);
            let mut state = map.state(match_seed)?;
            state.settings.fog = true;
            let mut session = Session::new(state.clone());
            let mut entropy = Rng::from_seed(BaselineConfig::LOCKED.entropy_seed(match_seed));
            let candidate_seat = game % 2;
            let (candidate_seed, baseline_seed) = match_agent_seeds(
                plan.agent_seed_protocol,
                match_seed,
                candidate_seat,
                1 - candidate_seat,
            );
            let mut candidate_agent = candidate.create(candidate_seed);
            let mut baseline_agent = baseline.create(baseline_seed);
            let mut agents: [&mut dyn Agent; 2] = if candidate_seat == 0 {
                [&mut *candidate_agent, &mut *baseline_agent]
            } else {
                [&mut *baseline_agent, &mut *candidate_agent]
            };
            let mut memories = [FogMemory::new(), FogMemory::new()];
            // The true income and spending of each seat, from its funds.
            let mut funds: Vec<f64> = (0..2).map(|seat| seat_funds(&state, seat)).collect();
            let mut income: Vec<f64> = funds.clone();
            let mut spent = [0.0; 2];
            let mut measured: Vec<Option<u64>> = vec![None, None];
            let mut rows = Vec::new();
            let observer =
                |state: &State, _: Option<&awvm::transition::Command>| {
                    let player = &state.turn.active_player;
                    let Some(seat) = state.players.seat(player) else {
                        return;
                    };
                    let seat = seat.get();
                    for index in 0..funds.len() {
                        let now = seat_funds(state, index);
                        if now > funds[index] {
                            income[index] += now - funds[index];
                        } else {
                            spent[index] += funds[index] - now;
                        }
                        funds[index] = now;
                    }
                    let Ok(view) = observe(&AwbwVisibility, state, player) else {
                        return;
                    };
                    let Some(memory) = memories.get_mut(seat) else {
                        return;
                    };
                    memory.update(&view);
                    if measured[seat] == Some(state.turn.day) {
                        return;
                    }
                    measured[seat] = Some(state.turn.day);
                    rows.extend(memory.accuracy(state).into_iter().map(|accuracy| {
                        FogAccuracyRow {
                            map_id: map.id,
                            game,
                            seat,
                            true_income: income[1 - seat],
                            true_spent: spent[1 - seat],
                            accuracy,
                        }
                    }));
                };
            play_observed(
                state,
                &mut session,
                &mut agents,
                &mut entropy,
                limits,
                observer,
            )?;
            Ok(rows)
        };

    let mut rows = Vec::new();
    crate::workers::run_ordered(games, jobs, play, |game, output| {
        let output = output?;
        eprintln!("game {game:>4}: {} samples", output.len());
        rows.extend(output);
        Ok::<bool, SprtError>(false)
    })?;

    let result = FogAccuracyResult {
        run_id: plan.run_id.clone(),
        games,
        summaries: RANGES
            .iter()
            .map(|(first, last)| summarize(&rows, *first, *last))
            .collect(),
        rows,
    };
    let output = output.as_ref();
    fs::create_dir_all(output)?;
    fs::write(
        output.join("fog-accuracy.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    Ok(result)
}

/// The funds of the seat at `index`.
fn seat_funds(state: &State, index: usize) -> f64 {
    state
        .players
        .seats()
        .nth(index)
        .map_or(0.0, |(_, player)| player.funds as f64)
}

fn summarize(rows: &[FogAccuracyRow], first: u64, last: u64) -> FogAccuracySummary {
    let days = if last == u64::MAX {
        format!("{first}+")
    } else {
        format!("{first}-{last}")
    };
    let samples: Vec<&FogAccuracy> = rows
        .iter()
        .map(|row| &row.accuracy)
        .filter(|accuracy| (first..=last).contains(&accuracy.day))
        .collect();
    if samples.is_empty() {
        return FogAccuracySummary {
            days,
            ..FogAccuracySummary::default()
        };
    }
    let count = samples.len() as f64;
    let mean =
        |value: &dyn Fn(&FogAccuracy) -> f64| samples.iter().map(|s| value(s)).sum::<f64>() / count;
    let tracks: f64 = samples.iter().map(|s| f64::from(s.tracks)).sum();
    let found: f64 = samples.iter().map(|s| f64::from(s.tracks_found)).sum();
    let hidden: f64 = samples.iter().map(|s| s.hidden_value).sum();
    let tracked: f64 = samples.iter().map(|s| s.tracked_value).sum();
    FogAccuracySummary {
        days,
        samples: samples.len(),
        true_value: mean(&|s| s.true_value),
        ledger_error: mean(&|s| s.estimated_value - s.true_value),
        ledger_absolute_error: mean(&|s| (s.estimated_value - s.true_value).abs()),
        mirror_error: mean(&|s| s.mirror_value - s.true_value),
        mirror_absolute_error: mean(&|s| (s.mirror_value - s.true_value).abs()),
        true_properties: mean(&|s| f64::from(s.true_properties)),
        property_error: mean(&|s| s.estimated_properties - f64::from(s.true_properties)),
        property_absolute_error: mean(&|s| {
            (s.estimated_properties - f64::from(s.true_properties)).abs()
        }),
        known_property_error: mean(&|s| {
            f64::from(s.known_properties) - f64::from(s.true_properties)
        }),
        known_property_absolute_error: mean(&|s| {
            (f64::from(s.known_properties) - f64::from(s.true_properties)).abs()
        }),
        tracks: tracks / count,
        track_precision: if tracks > 0.0 { found / tracks } else { 0.0 },
        tracked_share: if hidden > 0.0 { tracked / hidden } else { 0.0 },
    }
}
