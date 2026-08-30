//! Advisory local compatibility checks for archived randomized actions.
//!
//! A replay exposes displayed HP and a recorded post-state, not the exact HP
//! remainders or entropy tokens that produced them. This module expands only
//! those hidden values for one action. Every candidate starts from the same
//! graphical pre-state; a match is evidence of local compatibility and is
//! never selected as the pre-state of a later action.

use awbw_replay::turn_models::Action;
use awvm::commander::Domain;
use awvm::event::AttackTarget;
use awvm::random::{Entropy, Luck, RandomError, RandomToken, RandomTokenKind};
use awvm::semantic::{State, UnitId, WeatherKind};
use awvm::transition::{Command, ExecuteError, ExecuteOutcome, execute_with};

use crate::diagnostic_command;

const CORPUS_EXECUTION_LIMIT: usize = 100_000;

/// The result of asking whether one archived action admits an AWVM execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalCompatibility {
    LocallyCompatible(LocalCompatibilityMatch),
    LocallyDivergent(LocalDivergence),
    InsufficientReplayData(InsufficientReplayData),
}

/// One witness plus the amount of finite search needed to find it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalCompatibilityMatch {
    pub exact_hp: Vec<HpAssignment>,
    pub random: Vec<RandomToken>,
    pub counts: CandidateCounts,
}

/// Search evidence retained when no candidate reproduced the archive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalDivergence {
    pub counts: CandidateCounts,
    pub first_rejection: Option<String>,
    pub first_execution_error: Option<String>,
    pub first_mismatched_components: Vec<&'static str>,
    pub first_accepted_exact_hp: Option<Vec<HpAssignment>>,
    pub first_accepted_random: Option<Vec<RandomToken>>,
}

/// Stable counters suitable for the advisory 5.3b histogram.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CandidateCounts {
    pub hp_assignments: usize,
    pub executions: usize,
    pub accepted: usize,
    pub matching: usize,
    pub rejected: usize,
    pub execution_errors: usize,
}

/// An exact HP value tried for a graphically displayed unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HpAssignment {
    pub unit: UnitId,
    pub hp: u8,
}

/// Why the archive does not contain enough information for this local probe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InsufficientReplayData {
    pub reason: String,
}

/// Diagnose one archived action without advancing either state.
///
/// Every relevant displayed HP bar is expanded to its admitted exact values,
/// and reducer-requested finite entropy domains are enumerated lazily. Both
/// the AWVM result and recorded result are reduced back to graphical HP before
/// comparison.
pub fn diagnose_local_compatibility(
    graphical_pre: &State,
    action: &Action,
    recorded_post: &State,
) -> LocalCompatibility {
    diagnose(graphical_pre, action, recorded_post, false, None)
}

/// Classify one action, stopping as soon as a compatibility witness is found.
///
/// This is the corpus-oriented form used by phase 5.3b. A divergent result
/// still exhausts every candidate; only a compatible result may have partial
/// counters.
pub fn diagnose_local_compatibility_until_match(
    graphical_pre: &State,
    action: &Action,
    recorded_post: &State,
) -> LocalCompatibility {
    diagnose(
        graphical_pre,
        action,
        recorded_post,
        true,
        Some(CORPUS_EXECUTION_LIMIT),
    )
}

fn diagnose(
    graphical_pre: &State,
    action: &Action,
    recorded_post: &State,
    stop_on_match: bool,
    execution_limit: Option<usize>,
) -> LocalCompatibility {
    let command = match diagnostic_command(graphical_pre.turn.active_player.clone(), action) {
        Ok(command) => command,
        Err(error) => return insufficient(error.to_string()),
    };
    if matches!(command, Command::ProduceUnit { .. }) {
        return insufficient("Build does not record the AWVM-assigned unit identifier".into());
    }
    let plan = match candidate_plan(graphical_pre, &command) {
        Ok(plan) => plan,
        Err(reason) => return insufficient(reason),
    };

    let expected = graphical_state(recorded_post);
    let mut counts = CandidateCounts::default();
    let mut witness = None;
    let mut first_rejection = None;
    let mut first_execution_error = None;
    let mut first_mismatched_components = Vec::new();
    let mut first_accepted_exact_hp = None;
    let mut first_accepted_random = None;
    for assignments in &plan.assignments {
        counts.hp_assignments += 1;
        let mut candidate = graphical_pre.clone();
        for assignment in assignments {
            candidate
                .units
                .get_mut(assignment.unit)
                .expect("candidate plan names a present unit")
                .hp = assignment.hp;
        }

        let stopped = enumerate_entropy(
            &candidate,
            &command,
            &mut Vec::new(),
            &mut |random, outcome| {
                counts.executions += 1;
                match outcome {
                    Ok(ExecuteOutcome::Accepted(execution)) => {
                        counts.accepted += 1;
                        if first_accepted_exact_hp.is_none() {
                            first_accepted_exact_hp = Some(assignments.clone());
                            first_accepted_random = Some(random.to_vec());
                        }
                        let actual = graphical_state(&execution.state);
                        if actual == expected {
                            counts.matching += 1;
                            witness.get_or_insert_with(|| LocalCompatibilityMatch {
                                exact_hp: assignments.clone(),
                                random: random.to_vec(),
                                counts: CandidateCounts::default(),
                            });
                        } else if first_mismatched_components.is_empty() {
                            first_mismatched_components = mismatched_components(&actual, &expected);
                        }
                    }
                    Ok(ExecuteOutcome::Rejected(violation)) => {
                        counts.rejected += 1;
                        first_rejection.get_or_insert_with(|| format!("{violation:?}"));
                    }
                    Err(error) => {
                        counts.execution_errors += 1;
                        first_execution_error.get_or_insert_with(|| error.to_string());
                    }
                }
                (stop_on_match && witness.is_some())
                    || execution_limit.is_some_and(|limit| counts.executions >= limit)
            },
        );
        if stopped {
            break;
        }
    }

    if let Some(mut matched) = witness {
        matched.counts = counts;
        LocalCompatibility::LocallyCompatible(matched)
    } else if execution_limit.is_some_and(|limit| counts.executions >= limit) {
        insufficient(format!(
            "{} exceeded the {CORPUS_EXECUTION_LIMIT}-execution advisory search limit",
            action.kind_name()
        ))
    } else if plan.exhaustive {
        LocalCompatibility::LocallyDivergent(LocalDivergence {
            counts,
            first_rejection,
            first_execution_error,
            first_mismatched_components,
            first_accepted_exact_hp,
            first_accepted_random,
        })
    } else {
        insufficient(format!(
            "{} can depend on exact HP for units beyond the bounded local candidate set",
            action.kind_name()
        ))
    }
}

struct CandidatePlan {
    assignments: Vec<Vec<HpAssignment>>,
    exhaustive: bool,
}

fn candidate_plan(state: &State, command: &Command) -> Result<CandidatePlan, String> {
    let (units, exhaustive): (Vec<UnitId>, bool) = match command {
        Command::MoveAttack {
            unit,
            target: AttackTarget::Unit { unit: target },
            ..
        }
        | Command::MoveJoin { unit, target, .. } => (vec![*unit, *target], true),
        Command::MoveAttack {
            unit,
            target: AttackTarget::Tile { .. },
            ..
        }
        | Command::MoveCapture { unit, .. } => (vec![*unit], true),
        Command::MoveRepair { target, .. } => (vec![*target], true),
        Command::MoveExplode { .. }
        | Command::MoveLaunch { .. }
        | Command::ActivatePower { .. }
        | Command::Tag { .. }
        | Command::EndTurn { .. }
        | Command::Resign { .. }
        | Command::Timeout { .. } => (Vec::new(), false),
        Command::MoveWait { .. }
        | Command::DeleteUnit { .. }
        | Command::MoveHide { .. }
        | Command::MoveReveal { .. }
        | Command::ProduceUnit { .. }
        | Command::MoveSupply { .. }
        | Command::MoveLoad { .. }
        | Command::Unload { .. } => (Vec::new(), true),
        Command::Unsupported => return Err("action lowered to an unsupported command".into()),
    };

    let mut assignments = vec![Vec::new()];
    for unit_id in units {
        let hp = state
            .units
            .get(unit_id)
            .ok_or_else(|| format!("unit {unit_id} is absent from the graphical pre-state"))?
            .hp;
        let mut expanded = Vec::with_capacity(assignments.len() * 10);
        for prefix in assignments {
            for exact in admitted_exact_hp(hp) {
                let mut candidate = prefix.clone();
                candidate.push(HpAssignment {
                    unit: unit_id,
                    hp: exact,
                });
                expanded.push(candidate);
            }
        }
        assignments = expanded;
    }
    Ok(CandidatePlan {
        assignments,
        exhaustive,
    })
}

fn admitted_exact_hp(hp: u8) -> impl Iterator<Item = u8> {
    let displayed = hp.div_ceil(10).clamp(1, 10);
    let first = (displayed - 1) * 10 + 1;
    first..=displayed * 10
}

fn graphical_state(state: &State) -> State {
    let mut normalized = state.clone();
    for unit in &mut normalized.units {
        unit.hp = unit.hp.div_ceil(10).clamp(1, 10) * 10;
    }
    normalized
}

fn mismatched_components(actual: &State, expected: &State) -> Vec<&'static str> {
    let mut components = Vec::new();
    if actual.ruleset != expected.ruleset {
        components.push("ruleset");
    }
    if actual.settings != expected.settings {
        components.push("settings");
    }
    if actual.board != expected.board {
        components.push("board");
    }
    if actual.teams != expected.teams {
        components.push("teams");
    }
    if actual.players != expected.players {
        components.push("players");
    }
    if actual.turn != expected.turn {
        components.push("turn");
    }
    if actual.weather != expected.weather {
        components.push("weather");
    }
    if actual.units != expected.units {
        components.push("units");
    }
    if actual.next_unit_id != expected.next_unit_id {
        components.push("next-unit-id");
    }
    if actual.match_state != expected.match_state {
        components.push("match");
    }
    components
}

fn insufficient(reason: String) -> LocalCompatibility {
    LocalCompatibility::InsufficientReplayData(InsufficientReplayData { reason })
}

#[derive(Clone, Copy)]
enum EntropyRequest {
    Luck { polarity: Luck, domain: Domain },
    Weather,
}

struct PrefixEntropy<'a> {
    prefix: &'a [RandomToken],
    cursor: usize,
    request: Option<EntropyRequest>,
}

impl<'a> PrefixEntropy<'a> {
    const fn new(prefix: &'a [RandomToken]) -> Self {
        Self {
            prefix,
            cursor: 0,
            request: None,
        }
    }
}

impl Entropy for PrefixEntropy<'_> {
    fn luck(&mut self, polarity: Luck, domain: Domain) -> Result<i64, RandomError> {
        let expected = match polarity {
            Luck::Good => RandomTokenKind::CombatGoodLuck,
            Luck::Bad => RandomTokenKind::CombatBadLuck,
        };
        let Some(token) = self.prefix.get(self.cursor).copied() else {
            self.request = Some(EntropyRequest::Luck { polarity, domain });
            return Err(RandomError::Missing { expected });
        };
        self.cursor += 1;
        match (polarity, token) {
            (Luck::Good, RandomToken::CombatGoodLuck(value))
            | (Luck::Bad, RandomToken::CombatBadLuck(value))
                if (domain.minimum..=domain.maximum).contains(&value) =>
            {
                Ok(value)
            }
            _ => Err(RandomError::Unexpected {
                expected,
                actual: token.kind(),
            }),
        }
    }

    fn weather(&mut self) -> Result<WeatherKind, RandomError> {
        let Some(token) = self.prefix.get(self.cursor).copied() else {
            self.request = Some(EntropyRequest::Weather);
            return Err(RandomError::Missing {
                expected: RandomTokenKind::WeatherSelection,
            });
        };
        self.cursor += 1;
        match token {
            RandomToken::WeatherSelection(kind) => Ok(kind),
            _ => Err(RandomError::Unexpected {
                expected: RandomTokenKind::WeatherSelection,
                actual: token.kind(),
            }),
        }
    }
}

fn enumerate_entropy(
    state: &State,
    command: &Command,
    prefix: &mut Vec<RandomToken>,
    visit: &mut impl FnMut(&[RandomToken], Result<ExecuteOutcome, ExecuteError>) -> bool,
) -> bool {
    let mut entropy = PrefixEntropy::new(prefix);
    let outcome = execute_with(state, command.clone(), &mut entropy);
    match (&outcome, entropy.request) {
        (Err(ExecuteError::InvalidRandom(RandomError::Missing { .. })), Some(request)) => {
            match request {
                EntropyRequest::Luck { polarity, domain } => {
                    for value in domain.minimum..=domain.maximum {
                        prefix.push(match polarity {
                            Luck::Good => RandomToken::CombatGoodLuck(value),
                            Luck::Bad => RandomToken::CombatBadLuck(value),
                        });
                        if enumerate_entropy(state, command, prefix, visit) {
                            prefix.pop();
                            return true;
                        }
                        prefix.pop();
                    }
                }
                EntropyRequest::Weather => {
                    for kind in [WeatherKind::Clear, WeatherKind::Rain, WeatherKind::Snow] {
                        prefix.push(RandomToken::WeatherSelection(kind));
                        if enumerate_entropy(state, command, prefix, visit) {
                            prefix.pop();
                            return true;
                        }
                        prefix.pop();
                    }
                }
            }
        }
        _ => return visit(prefix, outcome),
    }
    false
}

#[cfg(test)]
mod tests {
    use super::admitted_exact_hp;

    #[test]
    fn graphical_hp_expands_to_its_exact_band() {
        assert_eq!(
            admitted_exact_hp(10).collect::<Vec<_>>(),
            (1..=10).collect::<Vec<_>>()
        );
        assert_eq!(
            admitted_exact_hp(73).collect::<Vec<_>>(),
            (71..=80).collect::<Vec<_>>()
        );
        assert_eq!(
            admitted_exact_hp(100).collect::<Vec<_>>(),
            (91..=100).collect::<Vec<_>>()
        );
    }
}
