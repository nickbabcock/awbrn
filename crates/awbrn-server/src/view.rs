//! Compatibility views projected from AWVM observations.
//!
//! These values remain the server's public wire contract while AWVM is the
//! sole rules engine. Visibility and event disclosure come from
//! `observe`/`observe_events`; this module only translates vocabulary.

use std::cell::OnceCell;
use std::collections::{BTreeMap, HashMap, HashSet};

use awbrn_types::{
    BridgeType, Faction, GraphicalTerrain, MissileSiloStatus, PipeSeamType, PipeType,
    PlayerFaction, Property, RiverType, RoadType, SeaDirection, ShoalDirection, Unit as ServerUnit,
};
use awvm::event::Event;
use awvm::ruleset::Terrain;
use awvm::semantic::{
    AwbwVisibility, Concealment, Location, Match, Observation, ObservedEvent, ObservedTransition,
    ObservedUnit, ObservedUnitHp, ObservedUnitRef, Outcome, Phase, PlayerId as VmPlayerId, Pos,
    State, TileVisibility, UnitId, Viewpoint, Visibility, observe, observe_transition,
};

use awbrn_map::semantic_terrain;

use crate::awvm_adapter::{AcceptedTransition, Authority};
use crate::player::PlayerId;
use crate::state::TurnPhase;
use crate::unit_id::ServerUnitId;

/// Header with the current game state, included in every response.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GameStateHeader {
    pub day: u32,
    pub active_player: PlayerId,
    pub phase: TurnPhase,
}

/// A unit as visible to a specific player.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct VisibleUnit {
    pub id: ServerUnitId,
    pub unit_type: ServerUnit,
    pub faction: PlayerFaction,
    pub position: Pos,
    pub hp: Option<u8>,
    pub fuel: Option<u32>,
    pub ammo: Option<u32>,
    pub capturing: bool,
    pub capture_progress: Option<u8>,
    pub hiding: bool,
}

/// A terrain tile as visible to a specific player.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct VisibleTerrain {
    pub position: Pos,
    pub terrain: GraphicalTerrain,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PlayerView {
    pub state: GameStateHeader,
    pub my_funds: u32,
    pub players: Vec<PublicPlayerState>,
    pub units: Vec<VisibleUnit>,
    pub terrain: Vec<VisibleTerrain>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicPlayerState {
    pub slot_index: u8,
    pub funds: u32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SpectatorView {
    pub state: GameStateHeader,
    pub players: Vec<PublicPlayerState>,
    pub units: Vec<VisibleUnit>,
    pub terrain: Vec<VisibleTerrain>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UnitMoved {
    pub id: ServerUnitId,
    pub path: Vec<Pos>,
    pub from: Pos,
    pub to: Pos,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TurnChange {
    pub new_active_player: PlayerId,
    pub new_day: Option<u32>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UnitCombatEvent {
    pub attacker_id: ServerUnitId,
    pub defender_id: ServerUnitId,
    pub attacker_hp_after: awbrn_game::world::GraphicalHp,
    pub defender_hp_after: awbrn_game::world::GraphicalHp,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CaptureEvent {
    CaptureContinued {
        tile: Pos,
        unit_id: ServerUnitId,
        progress: u8,
    },
    PropertyCaptured {
        tile: Pos,
        new_faction: PlayerFaction,
    },
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PlayerUpdate {
    pub units_revealed: Vec<VisibleUnit>,
    pub units_moved: Vec<UnitMoved>,
    pub units_removed: Vec<ServerUnitId>,
    pub terrain_revealed: Vec<VisibleTerrain>,
    pub terrain_changed: Vec<VisibleTerrain>,
    pub turn_change: Option<TurnChange>,
    pub combat_event: Option<UnitCombatEvent>,
    pub capture_event: Option<CaptureEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub my_funds: Option<u32>,
    pub state: GameStateHeader,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CommandResult {
    pub updates: Vec<(PlayerId, PlayerUpdate)>,
    pub observed_transitions: Vec<(PlayerId, ObservedTransition)>,
}

#[derive(Default)]
pub(crate) struct RecipientUnitIds {
    recipients: HashMap<PlayerId, EnemyUnitIds>,
}

struct EnemyUnitIds {
    next: u64,
    positions: HashMap<Pos, ServerUnitId>,
}

impl Default for EnemyUnitIds {
    fn default() -> Self {
        Self {
            next: u64::MAX,
            positions: HashMap::new(),
        }
    }
}

impl RecipientUnitIds {
    fn for_player(&mut self, player: PlayerId) -> &mut EnemyUnitIds {
        self.recipients.entry(player).or_default()
    }
}

impl EnemyUnitIds {
    fn resolve(&mut self, reference: &ObservedUnitRef) -> ServerUnitId {
        match reference {
            ObservedUnitRef::Friendly { unit } => server_unit_id(*unit),
            ObservedUnitRef::Enemy { position } => self.enemy_at(*position),
        }
    }

    fn tracked(&self, reference: &ObservedUnitRef) -> Option<ServerUnitId> {
        match reference {
            ObservedUnitRef::Friendly { unit } => Some(server_unit_id(*unit)),
            ObservedUnitRef::Enemy { position } => self.positions.get(position).copied(),
        }
    }

    fn move_unit(&mut self, reference: &ObservedUnitRef, from: Pos, to: Pos) -> ServerUnitId {
        match reference {
            ObservedUnitRef::Friendly { unit } => server_unit_id(*unit),
            ObservedUnitRef::Enemy { .. } => {
                let id = self
                    .positions
                    .remove(&from)
                    .or_else(|| self.positions.remove(&to))
                    .unwrap_or_else(|| self.allocate());
                self.positions.insert(to, id);
                id
            }
        }
    }

    fn drop_reference(&mut self, reference: &ObservedUnitRef) {
        if let ObservedUnitRef::Enemy { position } = reference {
            self.positions.remove(position);
        }
    }

    fn unobserved_enemies(
        &self,
        observation: &Observation,
    ) -> Vec<(ObservedUnitRef, ServerUnitId)> {
        let observed = observation
            .units
            .iter()
            .filter_map(|unit| match unit.reference {
                ObservedUnitRef::Friendly { .. } => None,
                ObservedUnitRef::Enemy { position } => Some(position),
            })
            .collect::<HashSet<_>>();
        self.positions
            .iter()
            .filter(|(position, _)| !observed.contains(position))
            .map(|(position, id)| {
                (
                    ObservedUnitRef::Enemy {
                        position: *position,
                    },
                    *id,
                )
            })
            .collect()
    }

    fn enemy_at(&mut self, position: Pos) -> ServerUnitId {
        if let Some(id) = self.positions.get(&position) {
            return *id;
        }
        let id = self.allocate();
        self.positions.insert(position, id);
        id
    }

    fn allocate(&mut self) -> ServerUnitId {
        let id = ServerUnitId(self.next);
        self.next = self
            .next
            .checked_sub(1)
            .expect("recipient-local unit identifiers are exhausted");
        id
    }
}

pub(crate) fn build_player_view(
    authority: &Authority,
    ids: &mut RecipientUnitIds,
    player: PlayerId,
) -> Option<PlayerView> {
    let state = authority.state();
    let recipient = authority.player(player);
    let recipient_state = state.find_player(&recipient)?;
    let observation = match observe(&AwbwVisibility, state, &recipient) {
        Ok(observation) => observation,
        Err(error) => {
            bevy::log::error!("failed to project player {player:?} view: {error}");
            return None;
        }
    };
    let ids = ids.for_player(player);
    Some(PlayerView {
        state: game_state_header(state),
        my_funds: narrow_u32(recipient_state.funds),
        players: public_player_states(state),
        units: observation
            .units
            .iter()
            .filter_map(|unit| {
                let unit = BoardObservedUnit::try_from(unit).ok()?;
                Some(visible_observed_unit(authority, &observation, unit, ids))
            })
            .collect(),
        terrain: observation
            .board
            .iter()
            .filter(|(_, tile)| tile.visibility == TileVisibility::Visible)
            .map(|(position, tile)| VisibleTerrain {
                position,
                terrain: graphical_terrain_at(
                    authority,
                    position,
                    tile.terrain,
                    tile.owner.player(),
                    tile.silo,
                ),
            })
            .collect(),
    })
}

pub(crate) fn build_spectator_view(authority: &Authority) -> SpectatorView {
    let state = authority.state();

    // A spectator sees the whole board, so the tile count is known before the
    // walk starts and the vector is sized once rather than grown.
    let mut terrain = Vec::with_capacity(state.board.dimensions().len());
    for (position, tile) in state.board.iter() {
        terrain.push(VisibleTerrain {
            position,
            terrain: graphical_terrain_at(
                authority,
                position,
                tile.terrain,
                state.tile_owner_id(&tile.owner),
                tile.silo,
            ),
        });
    }

    SpectatorView {
        state: game_state_header(state),
        players: public_player_states(state),
        units: state
            .units
            .iter()
            .filter_map(|unit| visible_unit(authority, state, unit, None))
            .collect(),
        terrain,
    }
}

pub(crate) fn build_command_result(
    authority: &Authority,
    transition: &AcceptedTransition,
    ids: &mut RecipientUnitIds,
) -> CommandResult {
    let players = authority.players().collect::<Vec<_>>();
    let projections = players
        .iter()
        .map(|player| {
            let recipient = authority.player(*player);
            let observed_transition = observe_transition(
                &AwbwVisibility,
                &transition.prior,
                authority.state(),
                &transition.events,
                &recipient,
            )
            .expect("an authoritative transition projects for every server player");
            (
                *player,
                RecipientProjection {
                    observed_transition,
                    prior: &transition.prior,
                    recipient,
                },
            )
        })
        .collect::<HashMap<_, _>>();
    let mut projected_teams = Vec::new();
    let mut updates = Vec::with_capacity(players.len());
    for player in &players {
        let recipient = authority.player(*player);
        let team = &transition
            .prior
            .find_player(&recipient)
            .expect("server player exists")
            .team;
        if projected_teams.contains(team) {
            continue;
        }
        projected_teams.push(team.clone());

        // Visibility and observed unit events are team-scoped. Player-change
        // snapshots can distinguish self from ally, but this compatibility
        // projection reads funds directly and ignores those snapshots.
        let pre_visibility = AwbwVisibility.view(&transition.prior, team);
        let post_visibility = AwbwVisibility.view(authority.state(), team);
        let (terrain_revealed, terrain_changed) = team_terrain_updates(
            authority,
            &transition.events,
            &pre_visibility,
            &post_visibility,
        );

        for teammate in players.iter().copied().filter(|candidate| {
            let candidate = authority.player(*candidate);
            transition
                .prior
                .find_player(&candidate)
                .is_some_and(|player| player.team == *team)
        }) {
            let projection = &projections[&teammate];
            updates.push((
                teammate,
                player_update(
                    authority,
                    &transition.prior,
                    &transition.events,
                    teammate,
                    ids,
                    projection,
                    TeamUpdate {
                        pre_visibility: &pre_visibility,
                        post_visibility: &post_visibility,
                        terrain_revealed: &terrain_revealed,
                        terrain_changed: &terrain_changed,
                    },
                ),
            ));
        }
    }
    updates.sort_by_key(|(player, _)| player.0);
    let mut observed_transitions = projections
        .into_iter()
        .map(|(player, projection)| (player, projection.observed_transition))
        .collect::<Vec<_>>();
    observed_transitions.sort_by_key(|(player, _)| player.0);
    CommandResult {
        updates,
        observed_transitions,
    }
}

struct RecipientProjection<'a> {
    observed_transition: ObservedTransition,
    prior: &'a State,
    recipient: VmPlayerId,
}

struct TeamUpdate<'a, V> {
    pre_visibility: &'a V,
    post_visibility: &'a V,
    terrain_revealed: &'a [VisibleTerrain],
    terrain_changed: &'a [VisibleTerrain],
}

fn player_update<V: Viewpoint>(
    authority: &Authority,
    prior: &State,
    events: &[Event],
    player: PlayerId,
    recipient_ids: &mut RecipientUnitIds,
    projection: &RecipientProjection<'_>,
    team: TeamUpdate<'_, V>,
) -> PlayerUpdate {
    let recipient = authority.player(player);
    let ids = recipient_ids.for_player(player);
    let mut units_revealed = Vec::new();
    let mut units_removed = Vec::new();
    let mut units_moved = Vec::new();

    for event in &projection.observed_transition.events {
        let ObservedEvent::UnitMoved {
            unit,
            from,
            to,
            path,
        } = event
        else {
            continue;
        };
        units_moved.push(UnitMoved {
            id: ids.move_unit(unit, *from, *to),
            path: path.to_vec(),
            from: *from,
            to: *to,
        });
    }

    let mut dropped_ids = BTreeMap::new();
    for (reference, id) in ids.unobserved_enemies(&projection.observed_transition.post) {
        push_unique_id(&mut units_removed, id);
        ids.drop_reference(&reference);
        dropped_ids.insert(reference, id);
    }

    for event in &projection.observed_transition.events {
        match event {
            ObservedEvent::UnitMoved { .. } => {}
            ObservedEvent::UnitAppeared { unit, .. }
            | ObservedEvent::UnitChanged { state: unit, .. } => {
                if let Ok(unit) = BoardObservedUnit::try_from(unit) {
                    push_unique_unit(
                        &mut units_revealed,
                        visible_observed_unit(
                            authority,
                            &projection.observed_transition.post,
                            unit,
                            ids,
                        ),
                    );
                }
            }
            ObservedEvent::UnitDisappeared { unit, .. }
            | ObservedEvent::UnitRemoved { unit, .. } => {
                if let Some(id) = dropped_ids.get(unit).copied().or_else(|| ids.tracked(unit)) {
                    push_unique_id(&mut units_removed, id);
                    ids.drop_reference(unit);
                    dropped_ids.insert(*unit, id);
                }
            }
            ObservedEvent::MovementStopped { .. }
            | ObservedEvent::CombatEngaged { .. }
            | ObservedEvent::TileChanged { .. }
            | ObservedEvent::PlayerChanged { .. }
            | ObservedEvent::AreaStrikeResolved { .. }
            | ObservedEvent::PublicEvent { .. } => {}
        }
    }

    let turn_change = (prior.turn.active_player != authority.state().turn.active_player
        || prior.turn.day != authority.state().turn.day)
        .then(|| TurnChange {
            new_active_player: server_player_id(&authority.state().turn.active_player),
            new_day: (prior.turn.day != authority.state().turn.day)
                .then(|| narrow_u32(authority.state().turn.day)),
        });
    let my_funds = funds(authority.state(), &recipient)
        .filter(|after| funds(prior, &recipient) != Some(*after));
    let combat_event = combat_event(projection, ids, &dropped_ids);
    let capture_event = capture_event(
        authority,
        events,
        team.pre_visibility,
        team.post_visibility,
        &projection.observed_transition.post,
        ids,
    );
    PlayerUpdate {
        units_revealed,
        units_moved,
        units_removed,
        terrain_revealed: team.terrain_revealed.to_vec(),
        terrain_changed: team.terrain_changed.to_vec(),
        turn_change,
        combat_event,
        capture_event,
        my_funds,
        state: game_state_header(authority.state()),
    }
}

fn team_terrain_updates(
    authority: &Authority,
    events: &[Event],
    pre_visibility: &impl Viewpoint,
    post_visibility: &impl Viewpoint,
) -> (Vec<VisibleTerrain>, Vec<VisibleTerrain>) {
    let terrain_revealed = if authority.state().settings.fog {
        authority
            .state()
            .board
            .positions()
            .filter(|position| {
                !pre_visibility.position(*position) && post_visibility.position(*position)
            })
            .map(|position| visible_authoritative_terrain(authority, position))
            .collect()
    } else {
        Vec::new()
    };
    let mut changed_positions = events
        .iter()
        .filter_map(|event| match event {
            Event::TileOwnerChanged { position, .. }
            | Event::TileTerrainChanged { position, .. }
            | Event::SiloChanged { position, .. } => Some(*position),
            _ => None,
        })
        .filter(|position| pre_visibility.position(*position))
        .collect::<Vec<_>>();
    changed_positions.sort_unstable();
    changed_positions.dedup();
    let terrain_changed = changed_positions
        .into_iter()
        .map(|position| visible_authoritative_terrain(authority, position))
        .collect();
    (terrain_revealed, terrain_changed)
}

fn visible_authoritative_terrain(authority: &Authority, position: Pos) -> VisibleTerrain {
    let state = authority.state();
    let tile = state.board.tile(position);
    VisibleTerrain {
        position,
        terrain: graphical_terrain_at(
            authority,
            position,
            tile.terrain,
            state.tile_owner_id(&tile.owner),
            tile.silo,
        ),
    }
}

fn combat_event(
    projection: &RecipientProjection<'_>,
    ids: &mut EnemyUnitIds,
    dropped_ids: &BTreeMap<ObservedUnitRef, ServerUnitId>,
) -> Option<UnitCombatEvent> {
    let observed = &projection.observed_transition;
    let (attacker, defender) = observed.events.iter().find_map(|event| match event {
        ObservedEvent::CombatEngaged { attacker, defender } => Some((*attacker, *defender)),
        _ => None,
    })?;
    let prior_observation = OnceCell::new();
    let attacker_hp_after =
        combat_graphical_hp(&attacker, observed, projection, &prior_observation)?;
    let defender_hp_after =
        combat_graphical_hp(&defender, observed, projection, &prior_observation)?;
    Some(UnitCombatEvent {
        attacker_id: dropped_ids
            .get(&attacker)
            .copied()
            .unwrap_or_else(|| ids.resolve(&attacker)),
        defender_id: dropped_ids
            .get(&defender)
            .copied()
            .unwrap_or_else(|| ids.resolve(&defender)),
        attacker_hp_after,
        defender_hp_after,
    })
}

fn combat_graphical_hp(
    reference: &ObservedUnitRef,
    observed: &ObservedTransition,
    projection: &RecipientProjection<'_>,
    prior_observation: &OnceCell<Observation>,
) -> Option<awbrn_game::world::GraphicalHp> {
    if let Some(unit) = observed
        .events
        .iter()
        .find_map(|event| match event {
            ObservedEvent::UnitChanged { state, .. }
            | ObservedEvent::UnitAppeared { unit: state, .. }
                if state.reference == *reference =>
            {
                Some(state)
            }
            _ => None,
        })
        .or_else(|| {
            observed
                .post
                .units
                .iter()
                .find(|unit| unit.reference == *reference)
        })
    {
        return Some(unit.hp.into());
    }
    let removed = observed
        .events
        .iter()
        .any(|event| matches!(event, ObservedEvent::UnitRemoved { unit, .. } if unit == reference));
    if !removed {
        return None;
    }
    let prior = prior_observation.get_or_init(|| {
        observe(&AwbwVisibility, projection.prior, &projection.recipient)
            .expect("the prior state projects for every server player")
    });
    let prior_reference = match reference {
        ObservedUnitRef::Friendly { .. } => *reference,
        ObservedUnitRef::Enemy { position } => {
            observed.events.iter().find_map(|event| match event {
                ObservedEvent::UnitMoved {
                    unit: ObservedUnitRef::Enemy { .. },
                    from,
                    to,
                    ..
                } if to == position => Some(ObservedUnitRef::Enemy { position: *from }),
                _ => None,
            })?
        }
    };
    let hp = prior
        .units
        .iter()
        .find(|unit| unit.reference == prior_reference)?
        .hp;
    Some(match hp {
        ObservedUnitHp::Exact(_) => ObservedUnitHp::Exact(0).into(),
        ObservedUnitHp::Hidden(hidden) => ObservedUnitHp::Hidden(hidden).into(),
    })
}

fn capture_event(
    authority: &Authority,
    events: &[Event],
    pre: &impl Viewpoint,
    post: &impl Viewpoint,
    observation: &Observation,
    ids: &mut EnemyUnitIds,
) -> Option<CaptureEvent> {
    if let Some((position, owner)) = events.iter().find_map(|event| match event {
        Event::TileOwnerChanged {
            position,
            to: Some(owner),
            ..
        } => Some((*position, owner)),
        _ => None,
    }) {
        let visible = pre.position(position) || post.position(position);
        if visible {
            return Some(CaptureEvent::PropertyCaptured {
                tile: position,
                new_faction: authority
                    .player_faction(owner)
                    .expect("a tile owner has a faction"),
            });
        }
    }
    events.iter().find_map(|event| match event {
        Event::CaptureChanged { position, to, .. } if *to < 20 && post.position(*position) => {
            let unit = observation.units.iter().find(
                |unit| matches!(unit.location, Location::Board { position: p } if p == *position),
            )?;
            Some(CaptureEvent::CaptureContinued {
                tile: *position,
                unit_id: ids.resolve(&unit.reference),
                progress: 20 - *to,
            })
        }
        _ => None,
    })
}

struct BoardObservedUnit<'a> {
    unit: &'a ObservedUnit,
    position: Pos,
}

impl<'a> TryFrom<&'a ObservedUnit> for BoardObservedUnit<'a> {
    type Error = ();

    fn try_from(unit: &'a ObservedUnit) -> Result<Self, Self::Error> {
        let Location::Board { position } = unit.location else {
            return Err(());
        };
        Ok(Self { unit, position })
    }
}

fn visible_observed_unit(
    authority: &Authority,
    observation: &Observation,
    observed: BoardObservedUnit<'_>,
    ids: &mut EnemyUnitIds,
) -> VisibleUnit {
    let BoardObservedUnit { unit, position } = observed;
    let capture_progress = observation
        .board
        .tile(position)
        .capture_points
        .filter(|points| *points < 20)
        .map(|remaining| 20 - remaining);
    let friendly = matches!(unit.reference, ObservedUnitRef::Friendly { .. });
    VisibleUnit {
        id: ids.resolve(&unit.reference),
        unit_type: unit.kind,
        faction: authority
            .player_faction(&unit.owner)
            .expect("every unit owner has a faction"),
        position,
        hp: awbrn_game::world::GraphicalHp::from(unit.hp)
            .visible()
            .map(awbrn_types::VisualHp::get),
        fuel: friendly.then(|| narrow_u32(unit.fuel)),
        ammo: friendly.then(|| narrow_u32(unit.ammo)),
        capturing: capture_progress.is_some(),
        capture_progress,
        hiding: unit.concealment == Concealment::Hidden,
    }
}

fn visible_unit(
    authority: &Authority,
    state: &State,
    unit: &awvm::semantic::Unit,
    friendly: Option<bool>,
) -> Option<VisibleUnit> {
    let Location::Board { position } = unit.location else {
        return None;
    };
    let tile = state.board.tile(position);
    let capture_progress = tile
        .capture_points
        .filter(|points| *points < 20)
        .map(|remaining| 20 - remaining);
    Some(VisibleUnit {
        id: server_unit_id(unit.id),
        unit_type: unit.kind,
        faction: authority
            .player_faction(state.player_id(unit.owner))
            .expect("every unit owner has a faction"),
        position,
        hp: awbrn_game::world::GraphicalHp::from(awbrn_types::ExactHp::new(unit.hp))
            .visible()
            .map(awbrn_types::VisualHp::get),
        fuel: friendly.unwrap_or(true).then(|| narrow_u32(unit.fuel)),
        ammo: friendly.unwrap_or(true).then(|| narrow_u32(unit.ammo)),
        capturing: capture_progress.is_some(),
        capture_progress,
        hiding: unit.concealment == Concealment::Hidden,
    })
}

/// The graphical terrain the view reports for one tile.
///
/// The map's own tile is the template: the board says what kind of terrain is
/// there, and the map says which art that kind is drawn in. Where the two
/// disagree, which a board edited after loading can do, the board wins and the
/// art falls back.
fn graphical_terrain_at(
    authority: &Authority,
    position: Pos,
    terrain: Terrain,
    owner: Option<&awvm::semantic::PlayerId>,
    silo: Option<awvm::semantic::Silo>,
) -> GraphicalTerrain {
    let mut graphical = authority
        .map()
        .terrain_at(position)
        .filter(|candidate| semantic_terrain(candidate.as_terrain()) == terrain)
        .unwrap_or_else(|| fallback_terrain(terrain, silo));
    match graphical {
        GraphicalTerrain::Property(property) => {
            let faction = owner
                .and_then(|owner| authority.player_faction(owner))
                .map_or(Faction::Neutral, Faction::Player);
            graphical = GraphicalTerrain::Property(property.with_owner(faction));
        }
        GraphicalTerrain::MissileSilo(_) => {
            graphical = GraphicalTerrain::MissileSilo(match silo {
                Some(awvm::semantic::Silo::Spent) => MissileSiloStatus::Unloaded,
                _ => MissileSiloStatus::Loaded,
            });
        }
        _ => {}
    }
    graphical
}

fn fallback_terrain(terrain: Terrain, silo: Option<awvm::semantic::Silo>) -> GraphicalTerrain {
    match terrain {
        Terrain::Airport => GraphicalTerrain::Property(Property::Airport(Faction::Neutral)),
        Terrain::Base => GraphicalTerrain::Property(Property::Base(Faction::Neutral)),
        Terrain::Bridge => GraphicalTerrain::Bridge(BridgeType::Horizontal),
        Terrain::City => GraphicalTerrain::Property(Property::City(Faction::Neutral)),
        Terrain::ComTower => GraphicalTerrain::Property(Property::ComTower(Faction::Neutral)),
        Terrain::Hq => GraphicalTerrain::Property(Property::HQ(PlayerFaction::OrangeStar)),
        Terrain::Lab => GraphicalTerrain::Property(Property::Lab(Faction::Neutral)),
        Terrain::MissileSilo => GraphicalTerrain::MissileSilo(match silo {
            Some(awvm::semantic::Silo::Spent) => MissileSiloStatus::Unloaded,
            _ => MissileSiloStatus::Loaded,
        }),
        Terrain::Mountain => GraphicalTerrain::Mountain,
        Terrain::Pipe => GraphicalTerrain::Pipe(PipeType::Horizontal),
        Terrain::PipeSeam => GraphicalTerrain::PipeSeam(PipeSeamType::Horizontal),
        Terrain::Plain => GraphicalTerrain::Plain,
        Terrain::Port => GraphicalTerrain::Property(Property::Port(Faction::Neutral)),
        Terrain::Reef => GraphicalTerrain::Reef,
        Terrain::River => GraphicalTerrain::River(RiverType::Horizontal),
        Terrain::Road => GraphicalTerrain::Road(RoadType::Horizontal),
        Terrain::Sea => GraphicalTerrain::Sea(SeaDirection::Sea),
        Terrain::Shoal => GraphicalTerrain::Shoal(ShoalDirection::S),
        Terrain::Teleporter => GraphicalTerrain::Teleporter,
        Terrain::Wood => GraphicalTerrain::Wood,
    }
}

fn game_state_header(state: &State) -> GameStateHeader {
    GameStateHeader {
        day: narrow_u32(state.turn.day),
        active_player: server_player_id(&state.turn.active_player),
        phase: if state.turn.phase == Phase::Finished {
            TurnPhase::GameOver {
                winner: winner(state),
            }
        } else {
            TurnPhase::PlayerTurn
        },
    }
}

fn winner(state: &State) -> Option<PlayerId> {
    let Match::Finished {
        outcome: Outcome::Victory { winners, .. },
    } = &state.match_state
    else {
        return None;
    };
    state
        .players
        .iter()
        .find(|player| winners.contains(&player.team))
        .map(|player| server_player_id(player.id()))
}

fn public_player_states(state: &State) -> Vec<PublicPlayerState> {
    state
        .players
        .iter()
        .map(|player| PublicPlayerState {
            slot_index: server_player_id(player.id()).0,
            funds: narrow_u32(player.funds),
        })
        .collect()
}

fn funds(state: &State, player: &VmPlayerId) -> Option<u32> {
    state
        .find_player(player)
        .map(|player| narrow_u32(player.funds))
}

fn push_unique_unit(units: &mut Vec<VisibleUnit>, unit: VisibleUnit) {
    if let Some(existing) = units.iter_mut().find(|existing| existing.id == unit.id) {
        *existing = unit;
    } else {
        units.push(unit);
    }
}

fn push_unique_id(ids: &mut Vec<ServerUnitId>, id: ServerUnitId) {
    if !ids.contains(&id) {
        ids.push(id);
    }
}

fn server_player_id(player: &VmPlayerId) -> PlayerId {
    PlayerId(
        player
            .as_str()
            .parse()
            .expect("server player ids are numeric slots"),
    )
}

fn server_unit_id(unit: UnitId) -> ServerUnitId {
    ServerUnitId(u64::from(unit.get()))
}

fn narrow_u32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
