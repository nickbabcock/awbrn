//! Compatibility views projected from AWVM observations.
//!
//! These values remain the server's public wire contract while AWVM is the
//! sole rules engine. Visibility and event disclosure come from
//! `observe`/`observe_events`; this module only translates vocabulary.

use awbrn_map::Position;
use awbrn_types::{
    BridgeType, Faction, GraphicalTerrain, MissileSiloStatus, PipeSeamType, PipeType,
    PlayerFaction, Property, RiverType, RoadType, SeaDirection, ShoalDirection, Unit as ServerUnit,
};
use awvm::event::{AttackTarget, Event};
use awvm::ruleset::{KnownReason, Terrain};
use awvm::semantic::{
    AwbwVisibility, Concealment, Location, Match, ObservedEvent, ObservedTransition, ObservedUnit,
    ObservedUnitRef, Outcome, Phase, PlayerId as VmPlayerId, Pos, Reason, State, TileOwner, UnitId,
    Viewpoint, Visibility, observe_events, observe_transition,
};

use crate::awvm_adapter::{AcceptedTransition, Authority, semantic_terrain};
use crate::player::PlayerId;
use crate::state::TurnPhase;
use crate::unit_id::ServerUnitId;

/// Exact HP-point deltas from a combat engagement on the 0-100 HP scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CombatOutcome {
    pub attacker_damage_pts: u8,
    pub defender_damage_pts: Option<u8>,
}

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
    pub position: Position,
    pub hp: u8,
    pub fuel: Option<u32>,
    pub ammo: Option<u32>,
    pub capturing: bool,
    pub capture_progress: Option<u8>,
    pub hiding: bool,
}

/// A terrain tile as visible to a specific player.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct VisibleTerrain {
    pub position: Position,
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
    pub path: Vec<Position>,
    pub from: Position,
    pub to: Position,
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
        tile: Position,
        unit_id: ServerUnitId,
        progress: u8,
    },
    PropertyCaptured {
        tile: Position,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combat_outcome: Option<CombatOutcome>,
}

pub(crate) fn build_player_view(authority: &Authority, player: PlayerId) -> Option<PlayerView> {
    let state = authority.state();
    let recipient = authority.player(player);
    let recipient_state = state.find_player(&recipient)?;
    let visibility = AwbwVisibility.view(state, &recipient_state.team);
    Some(PlayerView {
        state: game_state_header(state),
        my_funds: narrow_u32(recipient_state.funds),
        players: public_player_states(state),
        units: state
            .units
            .iter()
            .filter(|unit| visibility.unit(unit))
            .filter_map(|unit| {
                visible_unit(
                    authority,
                    state,
                    unit,
                    Some(same_team(state, &unit.owner, &recipient)),
                )
            })
            .collect(),
        terrain: state
            .board
            .iter()
            .filter(|(position, _)| visibility.position(*position))
            .map(|(position, tile)| VisibleTerrain {
                position: server_pos(position),
                terrain: graphical_terrain(
                    authority,
                    position,
                    tile.terrain,
                    &tile.owner,
                    tile.silo,
                ),
            })
            .collect(),
    })
}

pub(crate) fn build_spectator_view(authority: &Authority) -> SpectatorView {
    let state = authority.state();
    SpectatorView {
        state: game_state_header(state),
        players: public_player_states(state),
        units: state
            .units
            .iter()
            .filter_map(|unit| visible_unit(authority, state, unit, None))
            .collect(),
        terrain: state
            .board
            .iter()
            .map(|(position, tile)| VisibleTerrain {
                position: server_pos(position),
                terrain: graphical_terrain(
                    authority,
                    position,
                    tile.terrain,
                    &tile.owner,
                    tile.silo,
                ),
            })
            .collect(),
    }
}

pub(crate) fn build_command_result(
    authority: &Authority,
    transition: &AcceptedTransition,
) -> CommandResult {
    let combat_outcome = combat_outcome(&transition.prior, authority.state(), &transition.events);
    let players = authority.players().collect::<Vec<_>>();
    let observed_transitions = players
        .iter()
        .map(|player| {
            let recipient = authority.player(*player);
            let transition = observe_transition(
                &AwbwVisibility,
                &transition.prior,
                authority.state(),
                &transition.events,
                &recipient,
            )
            .expect("an authoritative transition projects for every server player");
            (*player, transition)
        })
        .collect();
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
        let observed = observe_events(
            &AwbwVisibility,
            &transition.prior,
            authority.state(),
            &transition.events,
            &recipient,
        )
        .expect("an authoritative transition projects for every server team");
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
            updates.push((
                teammate,
                player_update(
                    authority,
                    &transition.prior,
                    &transition.events,
                    teammate,
                    TeamUpdate {
                        observed: &observed,
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
    CommandResult {
        updates,
        observed_transitions,
        combat_outcome,
    }
}

struct TeamUpdate<'a, V> {
    observed: &'a [ObservedEvent],
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
    team: TeamUpdate<'_, V>,
) -> PlayerUpdate {
    let recipient = authority.player(player);
    let mut units_revealed = Vec::new();
    let mut units_removed = Vec::new();
    let mut units_moved = Vec::new();

    for event in team.observed {
        match event {
            ObservedEvent::UnitMoved {
                unit,
                from,
                to,
                path,
            } => {
                if let Some(id) = observed_ref_id(unit, prior, authority.state()) {
                    units_moved.push(UnitMoved {
                        id: server_unit_id(id),
                        path: path.iter().copied().map(server_pos).collect(),
                        from: server_pos(*from),
                        to: server_pos(*to),
                    });
                }
            }
            ObservedEvent::UnitAppeared { unit, .. }
            | ObservedEvent::UnitChanged { state: unit, .. } => {
                if let Some(visible) =
                    visible_observed_unit(authority, authority.state(), unit, &recipient)
                {
                    push_unique_unit(&mut units_revealed, visible);
                }
            }
            ObservedEvent::UnitDisappeared { unit, .. }
            | ObservedEvent::UnitRemoved { unit, .. } => {
                if let Some(id) = observed_ref_id(unit, prior, authority.state()) {
                    push_unique_id(&mut units_removed, server_unit_id(id));
                }
            }
            _ => {}
        }
    }

    // Transport and join events change presentation state even when the
    // observation projects them through a generic unit-change event.
    for event in events {
        let changed = match event {
            Event::UnitUnloaded {
                unit, transport, ..
            } => [Some(*unit), Some(*transport)],
            Event::UnitsJoined { target, .. } => [Some(*target), None],
            Event::ConcealmentChanged { unit, .. } => [Some(*unit), None],
            Event::UnitResourced {
                unit,
                reason: Reason::Known(KnownReason::UnitSupply | KnownReason::UnitProduction),
                ..
            } => [Some(*unit), None],
            _ => [None, None],
        };
        for id in changed.into_iter().flatten() {
            if let Some(unit) = authority.state().units.get(id)
                && team.post_visibility.unit(unit)
                && let Some(unit) = visible_unit(
                    authority,
                    authority.state(),
                    unit,
                    Some(same_team(authority.state(), &unit.owner, &recipient)),
                )
            {
                push_unique_unit(&mut units_revealed, unit);
            }
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

    PlayerUpdate {
        units_revealed,
        units_moved,
        units_removed,
        terrain_revealed: team.terrain_revealed.to_vec(),
        terrain_changed: team.terrain_changed.to_vec(),
        turn_change,
        combat_event: combat_event(authority.state(), events, prior, team.post_visibility),
        capture_event: capture_event(authority, events, team.pre_visibility, team.post_visibility),
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
    let tile = authority.state().board.tile(position);
    VisibleTerrain {
        position: server_pos(position),
        terrain: graphical_terrain(authority, position, tile.terrain, &tile.owner, tile.silo),
    }
}

fn combat_event(
    post: &State,
    events: &[Event],
    prior: &State,
    visibility: &impl Viewpoint,
) -> Option<UnitCombatEvent> {
    let (attacker, defender) = events.iter().find_map(|event| match event {
        Event::AttackResolved {
            attacker,
            target: AttackTarget::Unit { unit },
            ..
        } => Some((*attacker, *unit)),
        _ => None,
    })?;
    let visible = |id: UnitId| {
        if let Some(unit) = post.units.get(id) {
            return visibility.unit(unit);
        }
        prior.units.get(id).is_some_and(|unit| {
            let Location::Board { position } = unit.location else {
                return false;
            };
            visibility.unit_at(unit, position)
        })
    };
    if !visible(attacker) || !visible(defender) {
        return None;
    }
    Some(UnitCombatEvent {
        attacker_id: server_unit_id(attacker),
        defender_id: server_unit_id(defender),
        attacker_hp_after: awbrn_game::world::GraphicalHp(visual_hp(post, attacker)),
        defender_hp_after: awbrn_game::world::GraphicalHp(visual_hp(post, defender)),
    })
}

fn combat_outcome(prior: &State, post: &State, events: &[Event]) -> Option<CombatOutcome> {
    let (attacker, defender) = events.iter().find_map(|event| match event {
        Event::AttackResolved {
            attacker,
            target: AttackTarget::Unit { unit },
            ..
        } => Some((*attacker, *unit)),
        _ => None,
    })?;
    let defender_before = prior.units.get(defender)?.hp;
    let defender_after = post.units.get(defender).map_or(0, |unit| unit.hp);
    let attacker_before = prior.units.get(attacker)?.hp;
    let attacker_after = post.units.get(attacker).map_or(0, |unit| unit.hp);
    let countered = events.iter().any(|event| {
        matches!(
            event,
            Event::AttackResolved {
                attacker: counter,
                target: AttackTarget::Unit { unit },
                ..
            } if *counter == defender && *unit == attacker
        )
    });
    Some(CombatOutcome {
        attacker_damage_pts: defender_before.saturating_sub(defender_after),
        defender_damage_pts: countered.then(|| attacker_before.saturating_sub(attacker_after)),
    })
}

fn capture_event(
    authority: &Authority,
    events: &[Event],
    pre: &impl Viewpoint,
    post: &impl Viewpoint,
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
                tile: server_pos(position),
                new_faction: authority
                    .player_faction(owner)
                    .expect("a tile owner has a faction"),
            });
        }
    }
    events.iter().find_map(|event| match event {
        Event::CaptureChanged { position, to, .. } if *to < 20 && post.position(*position) => {
            let unit = authority.state().units.iter().find(
                |unit| matches!(unit.location, Location::Board { position: p } if p == *position),
            )?;
            Some(CaptureEvent::CaptureContinued {
                tile: server_pos(*position),
                unit_id: server_unit_id(unit.id),
                progress: 20 - *to,
            })
        }
        _ => None,
    })
}

fn visible_observed_unit(
    authority: &Authority,
    state: &State,
    observed: &ObservedUnit,
    recipient: &VmPlayerId,
) -> Option<VisibleUnit> {
    let id = observed_unit_id(observed, state)?;
    let unit = state.units.get(id)?;
    let recipient_team = &state.find_player(recipient)?.team;
    let owner_team = &state.find_player(&unit.owner)?.team;
    visible_unit(authority, state, unit, Some(owner_team == recipient_team))
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
            .player_faction(&unit.owner)
            .expect("every unit owner has a faction"),
        position: server_pos(position),
        hp: unit.hp.div_ceil(10),
        fuel: friendly.unwrap_or(true).then(|| narrow_u32(unit.fuel)),
        ammo: friendly.unwrap_or(true).then(|| narrow_u32(unit.ammo)),
        capturing: capture_progress.is_some(),
        capture_progress,
        hiding: unit.concealment == Concealment::Hidden,
    })
}

fn graphical_terrain(
    authority: &Authority,
    position: Pos,
    terrain: Terrain,
    owner: &TileOwner,
    silo: Option<awvm::semantic::Silo>,
) -> GraphicalTerrain {
    let template = authority.map().terrain_at(server_pos(position));
    let mut graphical = template
        .filter(|candidate| semantic_terrain(candidate.as_terrain()) == terrain)
        .unwrap_or_else(|| fallback_terrain(terrain, silo));
    if let GraphicalTerrain::Property(property) = graphical {
        let faction = owner
            .player()
            .and_then(|owner| authority.player_faction(owner))
            .map_or(Faction::Neutral, Faction::Player);
        graphical = GraphicalTerrain::Property(property.with_owner(faction));
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
        .map(|player| server_player_id(&player.id))
}

fn public_player_states(state: &State) -> Vec<PublicPlayerState> {
    state
        .players
        .iter()
        .map(|player| PublicPlayerState {
            slot_index: server_player_id(&player.id).0,
            funds: narrow_u32(player.funds),
        })
        .collect()
}

fn observed_unit_id(unit: &ObservedUnit, state: &State) -> Option<UnitId> {
    match unit.reference {
        ObservedUnitRef::Friendly { unit } => Some(unit),
        ObservedUnitRef::Enemy { position } => state
            .units
            .iter()
            .find(|candidate| {
                candidate.kind == unit.kind
                    && candidate.owner == unit.owner
                    && matches!(candidate.location, Location::Board { position: p } if p == position)
            })
            .map(|unit| unit.id),
    }
}

fn observed_ref_id(reference: &ObservedUnitRef, prior: &State, post: &State) -> Option<UnitId> {
    match reference {
        ObservedUnitRef::Friendly { unit } => Some(*unit),
        ObservedUnitRef::Enemy { position } => [post, prior]
            .into_iter()
            .flat_map(|state| state.units.iter())
            .find(|unit| matches!(unit.location, Location::Board { position: p } if p == *position))
            .map(|unit| unit.id),
    }
}

fn visual_hp(state: &State, unit: UnitId) -> u8 {
    state.units.get(unit).map_or(0, |unit| unit.hp.div_ceil(10))
}

fn same_team(state: &State, left: &VmPlayerId, right: &VmPlayerId) -> bool {
    state
        .find_player(left)
        .zip(state.find_player(right))
        .is_some_and(|(left, right)| left.team == right.team)
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

fn server_pos(position: Pos) -> Position {
    Position::new(usize::from(position.x), usize::from(position.y))
}

fn narrow_u32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
