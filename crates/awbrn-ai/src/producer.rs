//! Classify the next-turn usability of owned production properties.
//!
//! This module is a diagnostic feature. It does not select a command and it
//! does not read evaluator weights. A producer is classified from the board
//! state at the current turn boundary.
//!
//! The classes are applied in this order:
//!
//! 1. `disabled` means that a known rule or match state prevents production.
//! 2. `unknown` is used only for an occupation hidden by a fog observation.
//! 3. `open` means that no unit occupies the producer.
//! 4. `hostile-blocked` means that a hostile unit occupies the producer.
//! 5. `releasable` means that a friendly unit has a legal move to another
//!    resting tile. All other friendly occupants are `friendly-blocked`.
//!
//! A normal turn-start refresh makes `moved` and `spent` units ready again.
//! `immobilized` remains unable to act, so it remains a friendly blocker. The
//! movement query uses the current board and the rules engine's scratch
//! storage. It does not make a turn plan or clone the full state.

use awvm::commander;
use awvm::query::{MoveScratch, Sweep};
use awvm::ruleset::{self, Terrain, TerrainTrait, UnitKind};
use awvm::semantic::{
    Location, Match, Observation, ObservedPlayer, ObservedUnitRef, PlayerIdx, PlayerStatus, Pos,
    State, TeamId, UnitAction, UnitId,
};
use serde::{Deserialize, Serialize};

type ProducerExtraction = (
    Vec<ProducerUsabilityRecord>,
    Vec<ProducerUsabilityCounts>,
    Vec<TeamId>,
);

/// The usability of one owned production property.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProducerUsability {
    /// A unit can be produced at the next production opportunity.
    Open,
    /// A friendly unit blocks the property but can leave it.
    Releasable,
    /// A friendly unit blocks the property and cannot leave it.
    FriendlyBlocked,
    /// A hostile unit occupies the property.
    HostileBlocked,
    /// A known rule or match state prevents production.
    Disabled,
    /// Fog hides whether a unit occupies the property.
    Unknown,
}

/// The rule that selected a producer usability class.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProducerUsabilityRule {
    /// The match is finished or the owner is no longer active.
    MatchState,
    /// No legal unit domain remains at this site.
    NoProductionDomain,
    /// A unit is not on the producer.
    NoOccupation,
    /// A fog observation does not disclose occupation.
    OccupationNotObservable,
    /// A friendly unit has a legal move away from the producer.
    FriendlyUnitCanLeave,
    /// A friendly unit has no legal move away from the producer.
    FriendlyUnitCannotLeave,
    /// A hostile unit occupies the producer.
    HostileUnit,
}

/// One producer classification.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerUsabilityRecord {
    /// The row-major tile index. This is stable for one map.
    pub producer_tile: u32,
    /// The coordinate is useful when a scenario is inspected by hand.
    pub position: Pos,
    /// The owner seat in the state or observation roster.
    pub owner_seat: u8,
    /// The owner's team.
    pub owner_team: TeamId,
    /// The selected class.
    pub class: ProducerUsability,
    /// The rule that selected the class.
    pub rule: ProducerUsabilityRule,
}

/// Counts of producer classes for one seat.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerUsabilityCounts {
    pub open: u32,
    pub releasable: u32,
    pub friendly_blocked: u32,
    pub hostile_blocked: u32,
    pub disabled: u32,
    pub unknown: u32,
}

impl ProducerUsabilityCounts {
    /// Return known production capacity.
    pub const fn known_capacity(self) -> u32 {
        self.open + self.releasable
    }

    /// Add another set of counts.
    pub fn add_assign(&mut self, other: Self) {
        self.open += other.open;
        self.releasable += other.releasable;
        self.friendly_blocked += other.friendly_blocked;
        self.hostile_blocked += other.hostile_blocked;
        self.disabled += other.disabled;
        self.unknown += other.unknown;
    }

    fn add_class(&mut self, class: ProducerUsability) {
        match class {
            ProducerUsability::Open => self.open += 1,
            ProducerUsability::Releasable => self.releasable += 1,
            ProducerUsability::FriendlyBlocked => self.friendly_blocked += 1,
            ProducerUsability::HostileBlocked => self.hostile_blocked += 1,
            ProducerUsability::Disabled => self.disabled += 1,
            ProducerUsability::Unknown => self.unknown += 1,
        }
    }
}

/// The source of a producer classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProducerUsabilityMode {
    Authoritative,
    FogVisible,
}

/// Producer records and deterministic per-seat counts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerUsabilityReport {
    pub mode: ProducerUsabilityMode,
    pub records: Vec<ProducerUsabilityRecord>,
    pub counts_by_seat: Vec<ProducerUsabilityCounts>,
    pub teams: Vec<TeamId>,
    /// Number of movement queries used by this extraction.
    pub movement_queries: u64,
    /// Number of logical scratch-buffer allocations used by this extraction.
    pub scratch_allocations: u64,
    /// Number of full-state clones requested by this extraction.
    pub full_state_clones: u64,
}

/// Counts from one producer extraction without per-producer records.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerUsabilityCountsReport {
    pub mode: ProducerUsabilityMode,
    pub counts_by_seat: Vec<ProducerUsabilityCounts>,
    pub teams: Vec<TeamId>,
    /// Number of movement queries used by this extraction.
    pub movement_queries: u64,
    /// Number of logical scratch-buffer allocations used by this extraction.
    pub scratch_allocations: u64,
    /// Number of full-state clones requested by this extraction.
    pub full_state_clones: u64,
}

impl ProducerUsabilityReport {
    /// Return counts for one seat.
    pub fn counts(&self, seat: PlayerIdx) -> ProducerUsabilityCounts {
        self.counts_by_seat
            .get(seat.get())
            .copied()
            .unwrap_or_default()
    }

    /// Return the sum for one team.
    pub fn counts_for_team(&self, team: &TeamId) -> ProducerUsabilityCounts {
        self.teams
            .iter()
            .enumerate()
            .filter(|(_, candidate)| *candidate == team)
            .map(|(seat, _)| self.counts_by_seat[seat])
            .fold(ProducerUsabilityCounts::default(), |mut total, counts| {
                total.add_assign(counts);
                total
            })
    }
}

impl ProducerUsabilityCountsReport {
    /// Return counts for one seat.
    pub fn counts(&self, seat: PlayerIdx) -> ProducerUsabilityCounts {
        self.counts_by_seat
            .get(seat.get())
            .copied()
            .unwrap_or_default()
    }

    /// Return the sum for one team.
    pub fn counts_for_team(&self, team: &TeamId) -> ProducerUsabilityCounts {
        self.teams
            .iter()
            .enumerate()
            .filter(|(_, candidate)| *candidate == team)
            .map(|(seat, _)| self.counts_by_seat[seat])
            .fold(ProducerUsabilityCounts::default(), |mut total, counts| {
                total.add_assign(counts);
                total
            })
    }
}

/// Errors while classifying a fog observation.
#[derive(Debug, thiserror::Error)]
pub enum ProducerUsabilityError {
    #[error("producer usability observation cannot be reified: {0}")]
    Observation(String),
}

/// Reusable producer classifier scratch.
#[derive(Debug, Default)]
pub struct ProducerUsabilityExtractor {
    occupant: Vec<Option<UnitId>>,
    scratch: MoveScratch,
    movement_queries: u64,
    scratch_allocations: u64,
}

impl ProducerUsabilityExtractor {
    /// Create an empty extractor.
    pub const fn new() -> Self {
        Self {
            occupant: Vec::new(),
            scratch: MoveScratch::new(),
            movement_queries: 0,
            scratch_allocations: 0,
        }
    }

    /// Classify every owned producer in an authoritative state.
    pub fn state(&mut self, state: &State) -> ProducerUsabilityReport {
        let (records, counts_by_seat, teams) = self.state_inner(state, true);
        ProducerUsabilityReport {
            mode: ProducerUsabilityMode::Authoritative,
            records,
            counts_by_seat,
            teams,
            movement_queries: self.movement_queries,
            scratch_allocations: self.scratch_allocations,
            full_state_clones: 0,
        }
    }

    /// Count every owned producer without creating producer records.
    pub fn state_counts(&mut self, state: &State) -> ProducerUsabilityCountsReport {
        let (_, counts_by_seat, teams) = self.state_inner(state, false);
        ProducerUsabilityCountsReport {
            mode: ProducerUsabilityMode::Authoritative,
            counts_by_seat,
            teams,
            movement_queries: self.movement_queries,
            scratch_allocations: self.scratch_allocations,
            full_state_clones: 0,
        }
    }

    fn state_inner(&mut self, state: &State, include_records: bool) -> ProducerExtraction {
        self.movement_queries = 0;
        self.scratch_allocations = 0;
        self.fill_occupants(state);
        let mut records = include_records.then(Vec::new).unwrap_or_default();
        let mut counts_by_seat = vec![ProducerUsabilityCounts::default(); state.players.len()];
        let teams = state
            .players
            .iter()
            .map(|player| player.team.clone())
            .collect::<Vec<_>>();
        let sweeps = state
            .players
            .seats()
            .filter_map(|(seat, _)| Sweep::open(state, seat))
            .collect::<Vec<_>>();

        for (position, tile) in state.board.iter() {
            if !is_producer(tile.terrain) {
                continue;
            }
            let Some(owner) = tile.owner.player() else {
                continue;
            };
            let Some(owner_team) = state
                .players
                .get(owner.get())
                .map(|player| player.team.clone())
            else {
                continue;
            };
            let producer_tile = tile_id(state, position);
            let (class, rule) =
                self.classify_state_tile(state, owner, owner_team.clone(), position, &sweeps);
            if let Some(counts) = counts_by_seat.get_mut(owner.get()) {
                counts.add_class(class);
            }
            if include_records {
                records.push(ProducerUsabilityRecord {
                    producer_tile,
                    position,
                    owner_seat: u8::try_from(owner.get()).unwrap_or(u8::MAX),
                    owner_team,
                    class,
                    rule,
                });
            }
        }
        (records, counts_by_seat, teams)
    }

    fn classify_state_tile(
        &mut self,
        state: &State,
        owner: PlayerIdx,
        owner_team: TeamId,
        position: Pos,
        sweeps: &[Sweep<'_>],
    ) -> (ProducerUsability, ProducerUsabilityRule) {
        if !owner_can_produce(state, owner, position) {
            return (
                ProducerUsability::Disabled,
                if matches!(state.match_state, Match::Finished { .. })
                    || state.player(owner).status != PlayerStatus::Active
                {
                    ProducerUsabilityRule::MatchState
                } else {
                    ProducerUsabilityRule::NoProductionDomain
                },
            );
        }

        let occupant = self
            .occupant_at(state, position)
            .and_then(|unit| state.units.get(unit));
        let Some(occupant) = occupant else {
            return (ProducerUsability::Open, ProducerUsabilityRule::NoOccupation);
        };
        let Some(occupant_team) = state
            .players
            .get(occupant.owner.get())
            .map(|player| &player.team)
        else {
            return (
                ProducerUsability::Disabled,
                ProducerUsabilityRule::MatchState,
            );
        };
        if *occupant_team != owner_team {
            return (
                ProducerUsability::HostileBlocked,
                ProducerUsabilityRule::HostileUnit,
            );
        }
        if occupant.action == UnitAction::Immobilized {
            return (
                ProducerUsability::FriendlyBlocked,
                ProducerUsabilityRule::FriendlyUnitCannotLeave,
            );
        }
        let releasable = sweeps
            .iter()
            .find(|sweep| sweep.seat() == occupant.owner)
            .is_some_and(|sweep| self.can_leave(sweep, occupant.id));
        if releasable {
            (
                ProducerUsability::Releasable,
                ProducerUsabilityRule::FriendlyUnitCanLeave,
            )
        } else {
            (
                ProducerUsability::FriendlyBlocked,
                ProducerUsabilityRule::FriendlyUnitCannotLeave,
            )
        }
    }

    fn can_leave(&mut self, sweep: &Sweep<'_>, unit: UnitId) -> bool {
        self.movement_queries += 1;
        if self.movement_queries == 1 {
            self.scratch_allocations = 1;
        }
        sweep
            .can_leave_into(unit, &mut self.scratch)
            .unwrap_or(false)
    }

    fn fill_occupants(&mut self, state: &State) {
        self.occupant.clear();
        self.occupant.resize(state.board.dimensions().len(), None);
        for unit in state.units.iter() {
            let Location::Board { position } = unit.location else {
                continue;
            };
            if let Some(cell) = state.board.dimensions().cell_index(position) {
                self.occupant[usize::from(cell.get())] = Some(unit.id);
            }
        }
    }

    fn occupant_at(&self, state: &State, position: Pos) -> Option<UnitId> {
        state
            .board
            .dimensions()
            .cell_index(position)
            .and_then(|cell| {
                self.occupant
                    .get(usize::from(cell.get()))
                    .copied()
                    .flatten()
            })
    }
}

/// Classify every owned producer in an authoritative state.
pub fn classify_producers(state: &State) -> ProducerUsabilityReport {
    ProducerUsabilityExtractor::new().state(state)
}

/// Classify producers from one player's observation.
///
/// The function reads only the observation. It never uses an authoritative
/// state to decide whether a fogged producer is occupied. An opponent-owned
/// producer without a visible occupant is therefore `unknown`, because a
/// concealed unit may still occupy it.
pub fn classify_producers_in_observation(
    observation: &Observation,
) -> Result<ProducerUsabilityReport, ProducerUsabilityError> {
    let visible_state = awvm::session::Session::from_observation(observation)
        .map_err(|error| ProducerUsabilityError::Observation(error.to_string()))?;
    classify_producers_in_observation_with_session(observation, &visible_state)
}

/// Count producers from one player's observation.
pub fn classify_producer_counts_in_observation(
    observation: &Observation,
) -> Result<ProducerUsabilityCountsReport, ProducerUsabilityError> {
    let visible_state = awvm::session::Session::from_observation(observation)
        .map_err(|error| ProducerUsabilityError::Observation(error.to_string()))?;
    let mut extractor = ProducerUsabilityExtractor::new();
    extractor.observation_counts_with_session(observation, &visible_state)
}

/// Classify an observation while reusing its already reified session.
///
/// The session must come from [`awvm::session::Session::from_observation`]
/// with this observation. This entry point avoids a second visible-state
/// conversion during feature extraction. It does not read an authoritative
/// state.
pub fn classify_producers_in_observation_with_session(
    observation: &Observation,
    visible_session: &awvm::session::Session,
) -> Result<ProducerUsabilityReport, ProducerUsabilityError> {
    let mut extractor = ProducerUsabilityExtractor::new();
    extractor.observation_with_session(observation, visible_session)
}

impl ProducerUsabilityExtractor {
    /// Classify producers from an already reified observation.
    pub fn observation(
        &mut self,
        observation: &Observation,
    ) -> Result<ProducerUsabilityReport, ProducerUsabilityError> {
        let visible_session = awvm::session::Session::from_observation(observation)
            .map_err(|error| ProducerUsabilityError::Observation(error.to_string()))?;
        self.observation_with_session(observation, &visible_session)
    }

    /// Count producers from an already reified observation.
    pub fn observation_counts(
        &mut self,
        observation: &Observation,
    ) -> Result<ProducerUsabilityCountsReport, ProducerUsabilityError> {
        let visible_session = awvm::session::Session::from_observation(observation)
            .map_err(|error| ProducerUsabilityError::Observation(error.to_string()))?;
        self.observation_counts_with_session(observation, &visible_session)
    }

    /// Classify producers while reusing an existing observation session.
    pub fn observation_with_session(
        &mut self,
        observation: &Observation,
        visible_session: &awvm::session::Session,
    ) -> Result<ProducerUsabilityReport, ProducerUsabilityError> {
        let (records, counts_by_seat, teams) =
            self.observation_inner(observation, visible_session, true)?;
        Ok(ProducerUsabilityReport {
            mode: ProducerUsabilityMode::FogVisible,
            records,
            counts_by_seat,
            teams,
            movement_queries: self.movement_queries,
            scratch_allocations: self.scratch_allocations,
            full_state_clones: 0,
        })
    }

    /// Count producers while reusing an existing observation session.
    pub fn observation_counts_with_session(
        &mut self,
        observation: &Observation,
        visible_session: &awvm::session::Session,
    ) -> Result<ProducerUsabilityCountsReport, ProducerUsabilityError> {
        let (_, counts_by_seat, teams) =
            self.observation_inner(observation, visible_session, false)?;
        Ok(ProducerUsabilityCountsReport {
            mode: ProducerUsabilityMode::FogVisible,
            counts_by_seat,
            teams,
            movement_queries: self.movement_queries,
            scratch_allocations: self.scratch_allocations,
            full_state_clones: 0,
        })
    }

    fn observation_inner(
        &mut self,
        observation: &Observation,
        visible_session: &awvm::session::Session,
        include_records: bool,
    ) -> Result<ProducerExtraction, ProducerUsabilityError> {
        self.movement_queries = 0;
        self.scratch_allocations = 0;
        let visible_state = visible_session.state();
        let teams = observation
            .players
            .iter()
            .map(observed_player_team)
            .collect::<Vec<_>>();
        let mut counts_by_seat = vec![ProducerUsabilityCounts::default(); teams.len()];
        let mut records = include_records.then(Vec::new).unwrap_or_default();
        self.fill_occupants(visible_state);
        let sweeps = visible_state
            .players
            .seats()
            .filter_map(|(seat, _)| Sweep::open(visible_state, seat))
            .collect::<Vec<_>>();
        let recipient_team = observation
            .players
            .iter()
            .find(|player| observed_player_id(player) == &observation.recipient)
            .map(observed_player_team);

        for (index, (position, tile)) in observation.board.iter().enumerate() {
            if !is_producer(tile.terrain) {
                continue;
            }
            let Some(owner_id) = tile.owner.player() else {
                // A fogged tile does not disclose its owner. There is no honest
                // seat or team to attach to a record, so it is not a known owned
                // producer for this observation.
                continue;
            };
            let Some(owner_index) = observation
                .players
                .iter()
                .position(|player| observed_player_id(player) == owner_id)
            else {
                continue;
            };
            let owner_seat = u8::try_from(owner_index).unwrap_or(u8::MAX);
            let owner_team = teams[owner_index].clone();
            let producer_tile = u32::try_from(index).unwrap_or(u32::MAX);

            let (class, rule) =
                if !observation_owner_can_produce(visible_state, observation, owner_id, position) {
                    (
                        ProducerUsability::Disabled,
                        if matches!(
                            observation.match_state,
                            awvm::semantic::ObservedMatch::Finished { .. }
                        ) {
                            ProducerUsabilityRule::MatchState
                        } else {
                            ProducerUsabilityRule::NoProductionDomain
                        },
                    )
                } else if let Some(occupant) = observation_occupant(observation, position) {
                    let occupant_team = observation
                        .players
                        .iter()
                        .find(|player| observed_player_id(player) == &occupant.owner)
                        .map(observed_player_team);
                    if occupant_team.as_ref() != Some(&owner_team) {
                        (
                            ProducerUsability::HostileBlocked,
                            ProducerUsabilityRule::HostileUnit,
                        )
                    } else if occupant.action == UnitAction::Immobilized {
                        (
                            ProducerUsability::FriendlyBlocked,
                            ProducerUsabilityRule::FriendlyUnitCannotLeave,
                        )
                    } else {
                        let unit_id = match occupant.reference {
                            ObservedUnitRef::Friendly { unit } => Some(unit),
                            ObservedUnitRef::Enemy { .. } => visible_state
                                .units
                                .iter()
                                .find(|unit| unit.location == Location::Board { position })
                                .map(|unit| unit.id),
                        };
                        let releasable = unit_id.is_some_and(|unit| {
                            let Some(owner) = visible_state
                                .units
                                .iter()
                                .find(|candidate| candidate.id == unit)
                                .map(|candidate| candidate.owner)
                            else {
                                return false;
                            };
                            sweeps
                                .iter()
                                .find(|sweep| sweep.seat() == owner)
                                .is_some_and(|sweep| self.can_leave(sweep, unit))
                        });
                        if releasable {
                            (
                                ProducerUsability::Releasable,
                                ProducerUsabilityRule::FriendlyUnitCanLeave,
                            )
                        } else {
                            (
                                ProducerUsability::FriendlyBlocked,
                                ProducerUsabilityRule::FriendlyUnitCannotLeave,
                            )
                        }
                    }
                } else if observation.settings.fog && recipient_team.as_ref() != Some(&owner_team) {
                    // A visible hostile tile can still contain a concealed hostile
                    // unit. Keep this conservative because the observation has no
                    // negative fact that rules one out.
                    (
                        ProducerUsability::Unknown,
                        ProducerUsabilityRule::OccupationNotObservable,
                    )
                } else {
                    (ProducerUsability::Open, ProducerUsabilityRule::NoOccupation)
                };

            if let Some(counts) = counts_by_seat.get_mut(owner_index) {
                counts.add_class(class);
            }
            if include_records {
                records.push(ProducerUsabilityRecord {
                    producer_tile,
                    position,
                    owner_seat,
                    owner_team,
                    class,
                    rule,
                });
            }
        }
        Ok((records, counts_by_seat, teams))
    }
}

fn is_producer(terrain: Terrain) -> bool {
    ruleset::terrain_has(terrain, TerrainTrait::ProducesGround)
        || ruleset::terrain_has(terrain, TerrainTrait::ProducesAir)
        || ruleset::terrain_has(terrain, TerrainTrait::ProducesSea)
}

fn tile_id(state: &State, position: Pos) -> u32 {
    state
        .board
        .dimensions()
        .cell_index(position)
        .map(|cell| u32::from(cell.get()))
        .unwrap_or_default()
}

fn owner_can_produce(state: &State, owner: PlayerIdx, position: Pos) -> bool {
    if !ruleset::supports(&state.ruleset)
        || matches!(state.match_state, Match::Finished { .. })
        || state.player(owner).status != PlayerStatus::Active
    {
        return false;
    }
    if state.settings.unit_limit.is_some_and(|limit| {
        state
            .units
            .iter()
            .filter(|unit| unit.owner == owner)
            .count() as u64
            >= limit
    }) {
        return false;
    }
    let owns_lab = state
        .board
        .iter()
        .any(|(_, tile)| tile.terrain == Terrain::Lab && tile.owner.is_owned_by(owner));
    UnitKind::ALL.iter().copied().any(|kind| {
        let profile = ruleset::profile(kind);
        !state.settings.unit_bans.contains(&kind)
            && (!state.settings.lab_units.contains(&kind) || owns_lab)
            && commander::production_site(
                state,
                owner,
                state.board.tile(position).terrain,
                profile.domain,
            )
    })
}

fn observed_player_id(player: &ObservedPlayer) -> &awvm::semantic::PlayerId {
    match player {
        ObservedPlayer::Private { id, .. } | ObservedPlayer::Public { id, .. } => id,
    }
}

fn observed_player_team(player: &ObservedPlayer) -> TeamId {
    match player {
        ObservedPlayer::Private { team, .. } | ObservedPlayer::Public { team, .. } => team.clone(),
    }
}

fn observation_owner_can_produce(
    visible_state: &State,
    observation: &Observation,
    owner: &awvm::semantic::PlayerId,
    position: Pos,
) -> bool {
    let Some(owner_index) = observation
        .players
        .iter()
        .position(|player| observed_player_id(player) == owner)
    else {
        return false;
    };
    if matches!(
        observation.match_state,
        awvm::semantic::ObservedMatch::Finished { .. }
    ) {
        return false;
    }
    let owner_status = match &observation.players[owner_index] {
        ObservedPlayer::Private { status, .. } | ObservedPlayer::Public { status, .. } => *status,
    };
    if owner_status != PlayerStatus::Active {
        return false;
    }
    if observation.settings.unit_bans.len() >= UnitKind::COUNT {
        return false;
    }
    let Some(owner_seat) = visible_state.player_index(owner) else {
        return false;
    };
    if observation.settings.unit_limit.is_some_and(|limit| {
        visible_state
            .units
            .iter()
            .filter(|unit| unit.owner == owner_seat)
            .count() as u64
            >= limit
    }) {
        return false;
    }
    let owns_lab = visible_state
        .board
        .iter()
        .any(|(_, tile)| tile.terrain == Terrain::Lab && tile.owner.is_owned_by(owner_seat));
    let terrain = observation.board.tile(position).terrain;
    UnitKind::ALL.iter().copied().any(|kind| {
        !observation.settings.unit_bans.contains(&kind)
            && (!observation.settings.lab_units.contains(&kind) || owns_lab)
            && commander::production_site(
                visible_state,
                owner_seat,
                terrain,
                ruleset::profile(kind).domain,
            )
    })
}

fn observation_occupant(
    observation: &Observation,
    position: Pos,
) -> Option<&awvm::semantic::ObservedUnit> {
    observation
        .units
        .iter()
        .find(|unit| unit.location == Location::Board { position })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::arena;
    use awvm::semantic::{Concealment, Outcome, Unit};

    fn seats(state: &State) -> (PlayerIdx, PlayerIdx) {
        let mut seats = state.players.seats().map(|(seat, _)| seat);
        (
            seats.next().expect("the fixture has a first seat"),
            seats.next().expect("the fixture has a second seat"),
        )
    }

    fn producer(state: &State, owner: PlayerIdx) -> Pos {
        state
            .board
            .iter()
            .find(|(_, tile)| tile.owner.is_owned_by(owner) && is_producer(tile.terrain))
            .map(|(position, _)| position)
            .expect("the fixture has an owned producer")
    }

    fn add_unit(state: &mut State, owner: PlayerIdx, position: Pos, action: UnitAction) {
        state.units.push(Unit {
            id: UnitId::new(50_000 + u32::try_from(state.units.len()).unwrap_or_default()),
            kind: UnitKind::Infantry,
            owner,
            hp: 100,
            fuel: ruleset::profile(UnitKind::Infantry).max_fuel,
            ammo: ruleset::profile(UnitKind::Infantry).max_ammo,
            action,
            concealment: Concealment::Exposed,
            location: Location::Board { position },
        });
    }

    fn class_at(state: &State, position: Pos) -> ProducerUsability {
        classify_producers(state)
            .records
            .into_iter()
            .find(|record| record.position == position)
            .expect("the producer is classified")
            .class
    }

    #[test]
    fn empty_producer_is_open() {
        let state = arena(false, 1);
        let (first, _) = seats(&state);
        assert_eq!(
            class_at(&state, producer(&state, first)),
            ProducerUsability::Open
        );
    }

    #[test]
    fn a_ready_or_already_moved_friendly_unit_is_releasable() {
        for action in [UnitAction::Ready, UnitAction::Moved, UnitAction::Spent] {
            let mut state = arena(false, 1);
            let (first, _) = seats(&state);
            let position = producer(&state, first);
            add_unit(&mut state, first, position, action);
            assert_eq!(class_at(&state, position), ProducerUsability::Releasable);
        }
    }

    #[test]
    fn an_immobilized_friendly_unit_is_blocked() {
        let mut state = arena(false, 1);
        let (first, _) = seats(&state);
        let position = producer(&state, first);
        add_unit(&mut state, first, position, UnitAction::Immobilized);
        assert_eq!(
            class_at(&state, position),
            ProducerUsability::FriendlyBlocked
        );
    }

    #[test]
    fn a_hostile_unit_is_blocked() {
        let mut state = arena(false, 1);
        let (first, second) = seats(&state);
        let position = producer(&state, first);
        add_unit(&mut state, second, position, UnitAction::Ready);
        assert_eq!(
            class_at(&state, position),
            ProducerUsability::HostileBlocked
        );
    }

    #[test]
    fn a_unit_on_the_owner_team_is_friendly() {
        let mut state = arena(false, 1);
        let (first, second) = seats(&state);
        let first_team = state.player(first).team.clone();
        state.player_mut(second).team = first_team;
        let position = producer(&state, first);
        add_unit(&mut state, second, position, UnitAction::Ready);
        assert_eq!(class_at(&state, position), ProducerUsability::Releasable);
    }

    #[test]
    fn finished_state_has_disabled_producers_before_occupation() {
        let mut state = arena(false, 1);
        let (first, _) = seats(&state);
        let position = producer(&state, first);
        state.match_state = Match::Finished {
            outcome: Outcome::Draw {
                teams: Vec::new(),
                reason: awvm::semantic::DrawReason::DayLimit,
            },
        };
        add_unit(&mut state, first, position, UnitAction::Ready);
        assert_eq!(class_at(&state, position), ProducerUsability::Disabled);
    }

    #[test]
    fn a_producer_with_no_allowed_unit_domain_is_disabled() {
        let mut state = arena(false, 1);
        let (first, _) = seats(&state);
        let position = producer(&state, first);
        state.settings.unit_bans = UnitKind::ALL.to_vec();
        add_unit(&mut state, first, position, UnitAction::Ready);
        assert_eq!(class_at(&state, position), ProducerUsability::Disabled);
    }

    #[test]
    fn repeated_extraction_is_deterministic_and_does_not_change_the_state() {
        let state = arena(false, 1);
        let before = state.clone();
        let first = classify_producers(&state);
        let second = classify_producers(&state);
        assert_eq!(first, second);
        assert_eq!(state, before);
    }

    #[test]
    fn counts_only_extraction_matches_detailed_counts_without_records() {
        let state = arena(false, 1);
        let mut extractor = ProducerUsabilityExtractor::new();
        let detailed = extractor.state(&state);
        let counts = extractor.state_counts(&state);
        assert_eq!(detailed.counts_by_seat, counts.counts_by_seat);
        assert!(!detailed.records.is_empty());
    }

    #[test]
    fn unknown_capacity_is_not_included_in_the_known_delta() {
        let counts = ProducerUsabilityCounts {
            open: 2,
            releasable: 1,
            unknown: 4,
            ..ProducerUsabilityCounts::default()
        };
        assert_eq!(counts.known_capacity(), 3);
    }

    #[test]
    fn fog_observation_does_not_infer_a_hidden_hostile_occupant() {
        let mut state = arena(false, 1);
        let (first, second) = seats(&state);
        let position = producer(&state, second);
        add_unit(&mut state, first, position, UnitAction::Ready);
        let mut observation = awvm::semantic::observe(
            &awvm::semantic::AwbwVisibility,
            &state,
            state.player_id(first),
        )
        .expect("the observation is valid");
        observation.settings.fog = true;
        observation
            .units
            .retain(|unit| unit.location != Location::Board { position });
        let report =
            classify_producers_in_observation(&observation).expect("the observation reads");
        let record = report
            .records
            .iter()
            .find(|record| record.position == position)
            .expect("the owner and producer remain visible");
        assert_eq!(record.class, ProducerUsability::Unknown);
        assert_eq!(record.rule, ProducerUsabilityRule::OccupationNotObservable);
    }
}
