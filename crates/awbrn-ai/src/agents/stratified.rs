//! Mission-aware, one-pass stratified turn generation.

use awvm::commander;
use awvm::ruleset;
use awvm::semantic::{AwbwVisibility, Location, Match, Observation, UnitId, observe_into};
use awvm::session::{OrderKind, Session};
use awvm::transition::Command;

use crate::agent::{NodeBudget, Play};
use crate::agents::classifier::{
    CaptureMissionState, MissionBook, UnitRole, classify_with_missions,
};
use crate::agents::{GreedyAgent, Script, Weights};
use crate::rng::Rng;

/// A coarse unit group that receives one script during a stratified turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stratum {
    Objective,
    Support,
    Direct,
    Rear,
}

impl Stratum {
    /// The deterministic order of the one-pass generator.
    pub const ALL: [Self; 4] = [Self::Objective, Self::Support, Self::Direct, Self::Rear];

    /// A stable name for reports and diagnostics.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Objective => "objective",
            Self::Support => "support",
            Self::Direct => "direct",
            Self::Rear => "rear",
        }
    }
}

/// One script assignment for every stratum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StratifiedScripts {
    pub objective: Script,
    pub support: Script,
    pub direct: Script,
    pub rear: Script,
}

impl StratifiedScripts {
    pub const fn script(self, stratum: Stratum) -> Script {
        match stratum {
            Stratum::Objective => self.objective,
            Stratum::Support => self.support,
            Stratum::Direct => self.direct,
            Stratum::Rear => self.rear,
        }
    }

    /// Return this assignment with one stratum changed.
    pub const fn with_script(self, stratum: Stratum, script: Script) -> Self {
        match stratum {
            Stratum::Objective => Self {
                objective: script,
                ..self
            },
            Stratum::Support => Self {
                support: script,
                ..self
            },
            Stratum::Direct => Self {
                direct: script,
                ..self
            },
            Stratum::Rear => Self {
                rear: script,
                ..self
            },
        }
    }
}

impl Default for StratifiedScripts {
    fn default() -> Self {
        Self {
            objective: Script::CaptureCommitment,
            support: Script::SafePressure,
            direct: Script::FavorableCombat,
            rear: Script::ObjectiveDefense,
        }
    }
}

/// Generate one coherent turn from a script assignment over four strata.
///
/// `missions` is reconciled against the real root. Speculative state changes
/// use a clone, so generating a candidate cannot complete or invalidate the
/// caller's durable missions before the candidate is executed.
pub fn generate_stratified_plan(
    view: &Observation,
    seed: u64,
    missions: &mut MissionBook,
    scripts: StratifiedScripts,
) -> Option<Vec<Play>> {
    missions.update(view);
    let mut working_missions = missions.clone();
    let mut planner = TurnPlanner::new(view, seed)?;

    for stratum in Stratum::ALL {
        let mut agent = GreedyAgent::with_weights(seed, scripts.script(stratum).weights());
        planner.play_stratum(stratum, &mut agent, &mut working_missions)?;
    }

    let mut repair = GreedyAgent::with_weights(seed, Weights::BASELINE);
    planner.repair(&mut repair, &mut working_missions)?;
    Some(planner.plays)
}

struct TurnPlanner {
    session: Session,
    observation: Observation,
    player: awvm::semantic::PlayerId,
    entropy: Rng,
    plays: Vec<Play>,
}

impl TurnPlanner {
    fn new(view: &Observation, seed: u64) -> Option<Self> {
        let session = Session::from_observation(view).ok()?;
        if !session.is_commandable() || session.state().settings.fog {
            return None;
        }
        Some(Self {
            player: session.state().turn.active_player.clone(),
            session,
            observation: view.clone(),
            entropy: Rng::from_seed(Rng::mix(seed ^ 0x06a7_5a71)),
            plays: Vec::new(),
        })
    }

    fn play_stratum(
        &mut self,
        stratum: Stratum,
        agent: &mut GreedyAgent,
        missions: &mut MissionBook,
    ) -> Option<()> {
        while self.active() {
            self.observe()?;
            let assignments = classify_with_missions(&self.observation, missions);
            let mut units = Vec::new();
            let mut scripted_units = Vec::new();
            for assignment in assignments {
                if self.stratum_of(assignment.unit, assignment.role) != Some(stratum) {
                    continue;
                }
                units.push(assignment.unit);
                // Mission units are controlled only by the hard mission
                // selector. An emergency defender is free to attack.
                if !matches!(
                    assignment.role,
                    UnitRole::ActiveCapturer | UnitRole::AssignedCapturer
                ) {
                    scripted_units.push(assignment.unit);
                }
            }

            let mission = (stratum == Stratum::Objective)
                .then(|| mission_order(&self.session, missions, &units))
                .flatten();
            let play = mission.or_else(|| {
                agent.act_for_units(
                    &self.observation,
                    NodeBudget::ONE,
                    &scripted_units,
                    stratum == Stratum::Rear,
                )
            });
            let Some(play) = play else {
                break;
            };
            self.apply(play)?;
        }
        Some(())
    }

    fn repair(&mut self, agent: &mut GreedyAgent, missions: &mut MissionBook) -> Option<()> {
        while self.active() {
            self.observe()?;
            let assignments = classify_with_missions(&self.observation, missions);
            let mission_units: Vec<_> = assignments
                .iter()
                .filter(|assignment| {
                    matches!(
                        assignment.role,
                        UnitRole::ActiveCapturer | UnitRole::AssignedCapturer
                    )
                })
                .map(|assignment| assignment.unit)
                .collect();
            let ordinary_units: Vec<_> = assignments
                .iter()
                .filter(|assignment| !mission_units.contains(&assignment.unit))
                .map(|assignment| assignment.unit)
                .collect();
            let play = mission_order(&self.session, missions, &mission_units).or_else(|| {
                agent.act_for_units(&self.observation, NodeBudget::ONE, &ordinary_units, true)
            });
            let Some(play) = play else {
                self.end_turn()?;
                break;
            };
            self.apply(play)?;
        }
        Some(())
    }

    fn stratum_of(&self, unit: UnitId, role: UnitRole) -> Option<Stratum> {
        Some(match role {
            UnitRole::EmergencyDefender | UnitRole::ActiveCapturer | UnitRole::AssignedCapturer => {
                Stratum::Objective
            }
            UnitRole::TransportMission | UnitRole::ImmediateIndirectAttacker => Stratum::Support,
            UnitRole::ImmediateDirectTactical => Stratum::Direct,
            UnitRole::RearProduction => Stratum::Rear,
            UnitRole::Flex => {
                let unit = self.session.state().units.get(unit)?;
                let profile = ruleset::profile(unit.kind);
                if profile.ammo_weapon.is_some() || profile.unlimited_weapon.is_some() {
                    Stratum::Direct
                } else {
                    Stratum::Rear
                }
            }
        })
    }

    fn active(&self) -> bool {
        self.session.state().turn.active_player == self.player
            && matches!(self.session.state().match_state, Match::Active { .. })
    }

    fn observe(&mut self) -> Option<()> {
        observe_into(
            &AwbwVisibility,
            self.session.state(),
            &self.player,
            &mut self.observation,
        )
        .ok()
    }

    fn apply(&mut self, play: Play) -> Option<()> {
        let command = play.command(&self.session)?;
        let order = self.session.resolve(&command).ok()?;
        self.session.apply(order, &mut self.entropy, &mut ()).ok()?;
        if order.kind() != OrderKind::EndTurn {
            self.plays.push(play);
        }
        Some(())
    }

    fn end_turn(&mut self) -> Option<()> {
        let command = Command::EndTurn {
            player: self.player.clone(),
        };
        let order = self.session.resolve(&command).ok()?;
        self.session.apply(order, &mut self.entropy, &mut ()).ok()?;
        Some(())
    }
}

/// Select the best legal order that preserves one of the supplied missions.
fn mission_order(session: &Session, missions: &MissionBook, eligible: &[UnitId]) -> Option<Play> {
    let state = session.state();
    let friendly = state.players.seat(&state.turn.active_player)?;
    let mut ranked = Vec::new();
    let mut travel = session.travel(friendly)?;
    let mut distance_cache = Vec::new();

    for unit_id in eligible {
        let Some(mission) = missions.capture_mission(*unit_id) else {
            continue;
        };
        if mission.state == CaptureMissionState::SuspendedByEmergency {
            continue;
        }
        let Some(unit) = state.units.get(*unit_id) else {
            continue;
        };
        let Location::Board { position: origin } = unit.location else {
            continue;
        };
        let profile = ruleset::profile(unit.kind);
        let allowance = commander::effective_move(state, unit, profile.movement, profile.domain)
            .min(u64::from(u16::MAX)) as u16;
        let cache_index =
            distance_cache
                .iter()
                .position(|(property, movement_class, cached_allowance, _)| {
                    *property == mission.property
                        && *movement_class == profile.movement_class
                        && *cached_allowance == allowance
                });
        let distances = if let Some(index) = cache_index {
            &distance_cache[index].3
        } else {
            let mut distances = Vec::new();
            travel.points_to(
                profile.movement_class,
                allowance,
                [mission.property],
                &mut distances,
            );
            distance_cache.push((
                mission.property,
                profile.movement_class,
                allowance,
                distances,
            ));
            &distance_cache.last().expect("a distance entry was added").3
        };
        let dimensions = state.board.dimensions();
        let Some(origin_cell) = dimensions.cell_index(origin) else {
            continue;
        };
        let Some(current) = distances
            .get(usize::from(origin_cell.get()))
            .copied()
            .flatten()
        else {
            continue;
        };
        let Some(unit_index) = session.index_of(*unit_id) else {
            continue;
        };
        let mut orders = Vec::new();
        session.legal().unit_orders(unit_index, &mut orders);

        for order in orders {
            let at_target = dimensions.position_of(order.destination()) == Some(mission.property);
            if at_target && order.kind() == OrderKind::Capture {
                ranked.push((0_u8, 0_u16, *unit_id, order));
                continue;
            }
            if mission.state == CaptureMissionState::Capturing {
                continue;
            }
            let Some(remaining) = distances
                .get(usize::from(order.destination().get()))
                .copied()
                .flatten()
            else {
                continue;
            };
            if remaining < current {
                ranked.push((1, remaining, *unit_id, order));
            }
        }
    }

    ranked.sort_unstable();
    let (_, _, _, order) = ranked.into_iter().next()?;
    Play::from_order(session, order)
}

#[cfg(test)]
mod tests {
    use awvm::semantic::observe;
    use awvm::transition::{ExecuteOutcome, execute};

    use super::*;
    use crate::board::arena;

    fn view() -> Observation {
        let mut state = arena(false, 1);
        let player = state.turn.active_player.clone();
        state = match execute(&state, Command::EndTurn { player }, &[]) {
            Ok(ExecuteOutcome::Accepted(execution)) => execution.state,
            other => panic!("end turn did not execute: {other:?}"),
        };
        observe(&AwbwVisibility, &state, &state.turn.active_player)
            .expect("the active player observes the arena")
    }

    #[test]
    fn stratified_turn_is_repeatable_and_preserves_root_missions() {
        let view = view();
        let mut missions = MissionBook::new();
        let first =
            generate_stratified_plan(&view, 41, &mut missions, StratifiedScripts::default());
        let root_missions = missions.clone();
        let second =
            generate_stratified_plan(&view, 41, &mut missions, StratifiedScripts::default());

        assert_eq!(first, second);
        assert_eq!(missions, root_missions);
        assert!(first.is_some_and(|plan| !plan.is_empty()));
    }

    #[test]
    fn mission_order_reduces_distance_to_the_reserved_property() {
        let view = view();
        let session = Session::from_observation(&view).expect("the view opens");
        let mut missions = MissionBook::new();
        missions.update(&view);
        let eligible: Vec<_> = missions
            .capture_missions()
            .iter()
            .filter(|mission| mission.state.is_active())
            .map(|mission| mission.unit)
            .collect();
        let play = mission_order(&session, &missions, &eligible)
            .expect("an assigned capturer can approach its property");
        let mission = missions
            .capture_mission(play.unit().expect("a mission order has a unit"))
            .expect("the selected unit has a mission");
        let unit = session
            .state()
            .units
            .get(mission.unit)
            .expect("the mission unit exists");
        let Location::Board { position: origin } = unit.location else {
            panic!("the mission unit is on the board");
        };
        let profile = ruleset::profile(unit.kind);
        let allowance =
            commander::effective_move(session.state(), unit, profile.movement, profile.domain)
                .min(u64::from(u16::MAX)) as u16;
        let mut travel = session.travel(unit.owner).expect("travel opens");
        let mut distances = Vec::new();
        travel.points_to(
            profile.movement_class,
            allowance,
            [mission.property],
            &mut distances,
        );
        let dimensions = session.state().board.dimensions();
        let distance = |cell: awvm::semantic::CellIdx| {
            distances
                .get(usize::from(cell.get()))
                .copied()
                .flatten()
                .expect("the mission route exists")
        };
        let current = distance(
            dimensions
                .cell_index(origin)
                .expect("the unit stands on the board"),
        );
        let selected = distance(play.destination());
        assert!(
            play.kind() == OrderKind::Capture || selected < current,
            "a mission order must capture or reduce travel cost"
        );
        assert!(mission.state.is_active());
    }
}
