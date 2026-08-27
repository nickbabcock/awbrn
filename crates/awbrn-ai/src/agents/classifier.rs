//! Deterministic unit roles for stratified turn planning.

use std::collections::{HashMap, HashSet};

use awvm::commander;
use awvm::ruleset::{self, TerrainTrait};
use awvm::semantic::{Location, Observation, PlayerIdx, Pos, TileOwner, UnitId};
use awvm::session::{Order, OrderKind, Session};

/// A unit's current job, in classification precedence order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum UnitRole {
    /// Can immediately attack a capturer on a threatened owned objective.
    EmergencyDefender,
    /// Is already reducing the capture points of its current property.
    ActiveCapturer,
    /// Is cargo, a loaded carrier, or can load or unload this turn.
    TransportMission,
    /// Is an indirect unit with a legal attack this turn.
    ImmediateIndirectAttacker,
    /// Is a direct unit with a legal attack this turn.
    ImmediateDirectTactical,
    /// Can capture a property immediately.
    AssignedCapturer,
    /// Is a transport, supply unit, or repair unit without a current mission.
    RearProduction,
    /// Has no higher-priority assignment and remains available to a script.
    Flex,
}

/// One stable unit-to-role assignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoleAssignment {
    pub unit: UnitId,
    pub role: UnitRole,
}

/// The durable phase of one capturer-to-property assignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureMissionState {
    Approaching,
    Capturing,
    SuspendedByEmergency,
    Complete,
    Invalid,
}

impl CaptureMissionState {
    pub const fn is_active(self) -> bool {
        matches!(
            self,
            Self::Approaching | Self::Capturing | Self::SuspendedByEmergency
        )
    }
}

/// A persistent assignment between one capturer and one property.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaptureMission {
    pub unit: UnitId,
    pub property: Pos,
    pub state: CaptureMissionState,
    pub assigned_day: u64,
    pub updated_day: u64,
}

/// Mission state retained by an agent between turn roots.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MissionBook {
    capture: Vec<CaptureMission>,
}

impl MissionBook {
    pub const fn new() -> Self {
        Self {
            capture: Vec::new(),
        }
    }

    pub fn capture_missions(&self) -> &[CaptureMission] {
        &self.capture
    }

    /// The active capture mission held by this unit.
    pub fn capture_mission(&self, unit: UnitId) -> Option<&CaptureMission> {
        self.capture
            .iter()
            .find(|mission| mission.unit == unit && mission.state.is_active())
    }

    /// Reconcile old missions and assign currently uncommitted capturers.
    pub fn update(&mut self, view: &Observation) {
        let Ok(session) = Session::from_observation(view) else {
            return;
        };
        if !session.is_commandable() {
            return;
        }
        let state = session.state();
        let Some(friendly) = state.players.seat(&state.turn.active_player) else {
            return;
        };
        let day = state.turn.day;
        let threatened = threatened_objectives(&session, friendly);
        let mut all_orders = Vec::new();
        session.legal().orders(&mut all_orders);
        let orders = orders_by_unit(&session, all_orders);

        for mission in &mut self.capture {
            if !mission.state.is_active() {
                continue;
            }
            let Some(unit) = state.units.get(mission.unit) else {
                mission.state = CaptureMissionState::Invalid;
                mission.updated_day = day;
                continue;
            };
            if unit.owner != friendly || !ruleset::profile(unit.kind).can_capture {
                mission.state = CaptureMissionState::Invalid;
                mission.updated_day = day;
                continue;
            }
            let Some(tile) = state.board.get(mission.property) else {
                mission.state = CaptureMissionState::Invalid;
                mission.updated_day = day;
                continue;
            };
            if matches!(tile.owner, TileOwner::Owned(owner) if owner == friendly) {
                mission.state = CaptureMissionState::Complete;
            } else if !capturable_by(state, friendly, mission.property) {
                mission.state = CaptureMissionState::Invalid;
            } else if orders
                .get(&mission.unit)
                .is_some_and(|candidates| attacks_threat(candidates, &threatened))
            {
                mission.state = CaptureMissionState::SuspendedByEmergency;
            } else if unit.location
                == (Location::Board {
                    position: mission.property,
                })
                && tile.capture_points.is_some_and(|points| points < 20)
            {
                mission.state = CaptureMissionState::Capturing;
            } else {
                mission.state = CaptureMissionState::Approaching;
            }
            mission.updated_day = day;
        }

        self.capture.retain(|mission| mission.state.is_active());
        self.assign_uncommitted(&session, friendly, day);
    }

    fn assign_uncommitted(&mut self, session: &Session, friendly: PlayerIdx, day: u64) {
        let state = session.state();
        let already_committed: HashSet<_> = self
            .capture
            .iter()
            .filter(|mission| mission.state.is_active())
            .map(|mission| mission.unit)
            .collect();
        // A capture already in progress is a stronger assignment than any
        // new property ranking. Bind it before the general matching pass.
        for unit in state.units.iter().filter(|unit| {
            unit.owner == friendly
                && ruleset::profile(unit.kind).can_capture
                && !already_committed.contains(&unit.id)
        }) {
            let Location::Board { position } = unit.location else {
                continue;
            };
            let Some(tile) = state.board.get(position) else {
                continue;
            };
            if !capturable_by(state, friendly, position)
                || !tile.capture_points.is_some_and(|points| points < 20)
            {
                continue;
            }
            self.capture.push(CaptureMission {
                unit: unit.id,
                property: position,
                state: CaptureMissionState::Capturing,
                assigned_day: day,
                updated_day: day,
            });
        }
        let committed_units: HashSet<_> = self
            .capture
            .iter()
            .filter(|mission| mission.state.is_active())
            .map(|mission| mission.unit)
            .collect();
        let mut reserved_properties: HashSet<_> = self
            .capture
            .iter()
            .filter(|mission| mission.state.is_active())
            .map(|mission| mission.property)
            .collect();
        let properties: Vec<_> = state
            .board
            .iter()
            .filter_map(|(position, _)| {
                (capturable_by(state, friendly, position)
                    && !reserved_properties.contains(&position))
                .then_some(position)
            })
            .collect();
        let mut candidates = Vec::new();

        for unit in state.units.iter().filter(|unit| {
            unit.owner == friendly
                && ruleset::profile(unit.kind).can_capture
                && matches!(unit.location, Location::Board { .. })
                && !committed_units.contains(&unit.id)
        }) {
            let Location::Board { position: origin } = unit.location else {
                continue;
            };
            let profile = ruleset::profile(unit.kind);
            let allowance = commander::effective_move(state, unit, profile.movement, profile.domain)
                .min(u64::from(u16::MAX)) as u16;
            let Some(mut travel) = session.travel(friendly) else {
                continue;
            };
            let Some(origin_cell) = state.board.dimensions().cell_index(origin) else {
                continue;
            };
            let mut distances = Vec::new();
            for property in &properties {
                travel.points_to(
                    profile.movement_class,
                    allowance,
                    [*property],
                    &mut distances,
                );
                let Some(points) = distances
                    .get(usize::from(origin_cell.get()))
                    .copied()
                    .flatten()
                else {
                    continue;
                };
                let priority = property_priority(state, *property);
                let property_cell = state
                    .board
                    .dimensions()
                    .cell_index(*property)
                    .map_or(u16::MAX, |cell| cell.get());
                candidates.push((
                    std::cmp::Reverse(priority),
                    points,
                    property_cell,
                    unit.id,
                    *property,
                ));
            }
        }

        candidates.sort_unstable();
        let mut assigned_units = committed_units;
        for (_, _, _, unit, property) in candidates {
            if assigned_units.contains(&unit) || reserved_properties.contains(&property) {
                continue;
            }
            self.capture.push(CaptureMission {
                unit,
                property,
                state: CaptureMissionState::Approaching,
                assigned_day: day,
                updated_day: day,
            });
            assigned_units.insert(unit);
            reserved_properties.insert(property);
        }
    }
}

/// Classify every friendly unit visible at this root.
///
/// The roster order makes the result stable. Classification is exclusive:
/// the first applicable role in [`UnitRole`] precedence wins.
pub fn classify(view: &Observation) -> Vec<RoleAssignment> {
    let mut missions = MissionBook::new();
    classify_with_missions(view, &mut missions)
}

/// Classify units after updating the caller's persistent mission state.
pub fn classify_with_missions(
    view: &Observation,
    missions: &mut MissionBook,
) -> Vec<RoleAssignment> {
    missions.update(view);
    let Ok(session) = Session::from_observation(view) else {
        return Vec::new();
    };
    if !session.is_commandable() {
        return Vec::new();
    }
    let state = session.state();
    let Some(friendly) = state.players.seat(&state.turn.active_player) else {
        return Vec::new();
    };

    let mut orders = Vec::new();
    session.legal().orders(&mut orders);
    let orders_by_unit = orders_by_unit(&session, orders);
    let loaded = state.units.loaded_transports();
    let threatened = threatened_objectives(&session, friendly);
    let loading = loading_units(&session, &orders_by_unit);
    let assigned: HashSet<_> = missions
        .capture
        .iter()
        .filter(|mission| mission.state.is_active())
        .map(|mission| mission.unit)
        .collect();

    state
        .units
        .iter()
        .filter(|unit| unit.owner == friendly)
        .map(|unit| RoleAssignment {
            unit: unit.id,
            role: role_of(
                &session,
                unit,
                orders_by_unit
                    .get(&unit.id)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]),
                &loaded,
                &loading,
                &threatened,
                &assigned,
            ),
        })
        .collect()
}

fn role_of(
    session: &Session,
    unit: &awvm::semantic::Unit,
    orders: &[Order],
    loaded: &HashSet<UnitId, impl std::hash::BuildHasher>,
    loading: &HashSet<UnitId>,
    threatened: &HashSet<awvm::semantic::CellIdx>,
    assigned: &HashSet<UnitId>,
) -> UnitRole {
    let profile = ruleset::profile(unit.kind);
    let attacks = || {
        orders.iter().filter_map(|order| match order.kind() {
            OrderKind::Attack(target) => Some(target),
            _ => None,
        })
    };

    if attacks().any(|target| threatened.contains(&target)) {
        return UnitRole::EmergencyDefender;
    }
    if active_capturer(session, unit) {
        return UnitRole::ActiveCapturer;
    }
    if matches!(unit.location, Location::Cargo { .. })
        || loaded.contains(&unit.id)
        || loading.contains(&unit.id)
        || orders
            .iter()
            .any(|order| matches!(order.kind(), OrderKind::Load | OrderKind::Unload(_)))
    {
        return UnitRole::TransportMission;
    }
    if attacks().next().is_some() {
        return if profile.indirect_range.is_some() {
            UnitRole::ImmediateIndirectAttacker
        } else {
            UnitRole::ImmediateDirectTactical
        };
    }
    if profile.can_capture && assigned.contains(&unit.id) {
        return UnitRole::AssignedCapturer;
    }
    if profile.transport.is_some() || profile.supply.is_some() || profile.repair.is_some() {
        return UnitRole::RearProduction;
    }
    UnitRole::Flex
}

fn attacks_threat(orders: &[Order], threatened: &HashSet<awvm::semantic::CellIdx>) -> bool {
    orders.iter().any(
        |order| matches!(order.kind(), OrderKind::Attack(target) if threatened.contains(&target)),
    )
}

fn capturable_by(state: &awvm::semantic::State, friendly: PlayerIdx, position: Pos) -> bool {
    let Some(tile) = state.board.get(position) else {
        return false;
    };
    if !ruleset::terrain_has(tile.terrain, TerrainTrait::Capturable) {
        return false;
    }
    match tile.owner {
        TileOwner::Neutral => true,
        TileOwner::Owned(owner) => owner != friendly && hostile(state, friendly, owner),
        TileOwner::NotOwnable => false,
    }
}

fn hostile(state: &awvm::semantic::State, left: PlayerIdx, right: PlayerIdx) -> bool {
    crate::threat::hostile(state, left, right)
}

fn property_priority(state: &awvm::semantic::State, position: Pos) -> u8 {
    let Some(tile) = state.board.get(position) else {
        return 0;
    };
    let has = |value| ruleset::terrain_has(tile.terrain, value);
    if has(TerrainTrait::CaptureDefeatsOwner) {
        5
    } else if has(TerrainTrait::ProducesGround) {
        4
    } else if has(TerrainTrait::ProducesAir) {
        3
    } else if has(TerrainTrait::Income) {
        2
    } else {
        1
    }
}

fn orders_by_unit(session: &Session, orders: Vec<Order>) -> HashMap<UnitId, Vec<Order>> {
    let mut grouped = HashMap::<UnitId, Vec<Order>>::new();
    for order in orders {
        if let Some(unit) = session.unit_of(order) {
            grouped.entry(unit).or_default().push(order);
        }
    }
    grouped
}

/// Units participating in a legal load bind the cargo and carrier together.
fn loading_units(session: &Session, orders: &HashMap<UnitId, Vec<Order>>) -> HashSet<UnitId> {
    let mut loading = HashSet::new();
    for (cargo, candidates) in orders {
        for order in candidates {
            if order.kind() != OrderKind::Load {
                continue;
            }
            loading.insert(*cargo);
            let destination = order.destination();
            if let Some(transport) = unit_on_cell(session, destination) {
                loading.insert(transport);
            }
        }
    }
    loading
}

/// Cells occupied by enemy capturers that are taking one of our objectives.
fn threatened_objectives(
    session: &Session,
    friendly: PlayerIdx,
) -> HashSet<awvm::semantic::CellIdx> {
    let state = session.state();
    state
        .units
        .iter()
        .filter_map(|unit| {
            if unit.owner == friendly || !ruleset::profile(unit.kind).can_capture {
                return None;
            }
            let Location::Board { position } = unit.location else {
                return None;
            };
            let tile = state.board.get(position)?;
            let ours = matches!(tile.owner, TileOwner::Owned(owner) if owner == friendly);
            let valuable = ruleset::terrain_has(tile.terrain, TerrainTrait::CaptureDefeatsOwner)
                || ruleset::terrain_has(tile.terrain, TerrainTrait::ProducesGround)
                || ruleset::terrain_has(tile.terrain, TerrainTrait::ProducesAir)
                || ruleset::terrain_has(tile.terrain, TerrainTrait::ProducesSea)
                || ruleset::terrain_has(tile.terrain, TerrainTrait::Income);
            (ours && valuable && tile.capture_points.is_some_and(|points| points < 20))
                .then(|| state.board.dimensions().cell_index(position))?
        })
        .collect()
}

fn active_capturer(session: &Session, unit: &awvm::semantic::Unit) -> bool {
    if !ruleset::profile(unit.kind).can_capture {
        return false;
    }
    let Location::Board { position } = unit.location else {
        return false;
    };
    session
        .state()
        .board
        .get(position)
        .and_then(|tile| tile.capture_points)
        .is_some_and(|points| points < 20)
}

fn unit_on_cell(session: &Session, wanted: awvm::semantic::CellIdx) -> Option<UnitId> {
    let dimensions = session.state().board.dimensions();
    session.state().units.iter().find_map(|unit| {
        let Location::Board { position } = unit.location else {
            return None;
        };
        (dimensions.cell_index(position) == Some(wanted)).then_some(unit.id)
    })
}

#[cfg(test)]
mod tests {
    use awvm::ruleset::{Terrain, UnitKind};
    use awvm::semantic::{AwbwVisibility, Concealment, Unit, UnitAction, observe};
    use awvm::transition::{Command, ExecuteOutcome, execute};

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
    fn classification_is_complete_and_repeatable() {
        let view = view();
        let first = classify(&view);
        assert_eq!(first, classify(&view));
        assert!(!first.is_empty());

        let session = Session::from_observation(&view).expect("the view opens");
        let friendly = session
            .state()
            .players
            .seat(&session.state().turn.active_player)
            .expect("the active player has a seat");
        assert_eq!(
            first.len(),
            session
                .state()
                .units
                .iter()
                .filter(|unit| unit.owner == friendly)
                .count()
        );
    }

    #[test]
    fn every_unit_has_exactly_one_role() {
        let assignments = classify(&view());
        let unique: HashSet<_> = assignments
            .iter()
            .map(|assignment| assignment.unit)
            .collect();
        assert_eq!(assignments.len(), unique.len());
    }

    #[test]
    fn repeated_updates_preserve_assignments_without_duplicates() {
        let view = view();
        let mut missions = MissionBook::new();
        missions.update(&view);
        let first = missions.clone();
        missions.update(&view);

        assert_eq!(missions, first);
        let units: HashSet<_> = missions
            .capture_missions()
            .iter()
            .filter(|mission| mission.state.is_active())
            .map(|mission| mission.unit)
            .collect();
        let properties: HashSet<_> = missions
            .capture_missions()
            .iter()
            .filter(|mission| mission.state.is_active())
            .map(|mission| mission.property)
            .collect();
        let active = missions
            .capture_missions()
            .iter()
            .filter(|mission| mission.state.is_active())
            .count();
        assert_eq!(units.len(), active);
        assert_eq!(properties.len(), active);
    }

    #[test]
    fn a_friendly_property_removes_a_completed_mission() {
        let view = view();
        let session = Session::from_observation(&view).expect("the view opens");
        let state = session.state();
        let friendly = state
            .players
            .seat(&state.turn.active_player)
            .expect("the active player has a seat");
        let unit = state
            .units
            .iter()
            .find(|unit| unit.owner == friendly && ruleset::profile(unit.kind).can_capture)
            .expect("the active player has a capturer");
        let property = state
            .board
            .iter()
            .find_map(|(position, tile)| {
                matches!(tile.owner, TileOwner::Owned(owner) if owner == friendly)
                    .then_some(position)
            })
            .expect("the active player owns a property");
        let mut missions = MissionBook {
            capture: vec![CaptureMission {
                unit: unit.id,
                property,
                state: CaptureMissionState::Approaching,
                assigned_day: state.turn.day,
                updated_day: state.turn.day,
            }],
        };

        missions.update(&view);
        assert!(
            missions
                .capture_missions()
                .iter()
                .all(|mission| mission.property != property)
        );
    }

    #[test]
    fn an_active_capture_is_bound_before_new_assignments() {
        let initial = view();
        let session = Session::from_observation(&initial).expect("the view opens");
        let mut state = session.state().clone();
        let friendly = state
            .players
            .seat(&state.turn.active_player)
            .expect("the active player has a seat");
        let unit = state
            .units
            .iter()
            .find(|unit| unit.owner == friendly && ruleset::profile(unit.kind).can_capture)
            .copied()
            .expect("the active player has a capturer");
        let Location::Board { position } = unit.location else {
            panic!("the capturer is on the board");
        };
        let tile = state.board.get_mut(position).expect("the unit has a tile");
        tile.terrain = Terrain::City;
        tile.owner = TileOwner::Neutral;
        tile.capture_points = Some(10);
        let view = observe(&AwbwVisibility, &state, &state.turn.active_player)
            .expect("the active player observes the capture");
        let mut missions = MissionBook::new();

        missions.update(&view);
        let mission = missions
            .capture_mission(unit.id)
            .expect("the active capturer has a mission");
        assert_eq!(mission.property, position);
        assert_eq!(mission.state, CaptureMissionState::Capturing);
    }

    #[test]
    fn loaded_transport_and_cargo_share_the_transport_role() {
        let mut state = arena(false, 3);
        let owner = state
            .players
            .seat(&state.turn.active_player)
            .expect("the active player has a seat");
        let transport_id = UnitId::new(9_000);
        let cargo_id = UnitId::new(9_001);
        let position = (0..state.board.height())
            .flat_map(|y| (0..state.board.width()).map(move |x| Pos::new(x, y)))
            .find(|position| {
                state.units.iter().all(|unit| {
                    !matches!(unit.location, Location::Board { position: held } if held == *position)
                })
            })
            .expect("the arena has an empty tile");
        let apc = ruleset::profile(UnitKind::Apc);
        let infantry = ruleset::profile(UnitKind::Infantry);
        state.units.push(Unit {
            id: transport_id,
            kind: UnitKind::Apc,
            owner,
            hp: 100,
            fuel: apc.max_fuel,
            ammo: apc.max_ammo,
            action: UnitAction::Ready,
            concealment: Concealment::Exposed,
            location: Location::Board { position },
        });
        state.units.push(Unit {
            id: cargo_id,
            kind: UnitKind::Infantry,
            owner,
            hp: 100,
            fuel: infantry.max_fuel,
            ammo: infantry.max_ammo,
            action: UnitAction::Ready,
            concealment: Concealment::Exposed,
            location: Location::Cargo {
                transport: transport_id,
                slot: 0,
            },
        });
        let view = observe(&AwbwVisibility, &state, &state.turn.active_player)
            .expect("the active player observes the transport");
        let assignments = classify(&view);

        for unit in [transport_id, cargo_id] {
            assert_eq!(
                assignments
                    .iter()
                    .find(|assignment| assignment.unit == unit)
                    .map(|assignment| assignment.role),
                Some(UnitRole::TransportMission)
            );
        }
    }
}
