//! A move planner board: a turn planned with the actions of the game, and
//! where an area-strike power lands.
//!
//! A move planner opens in play mode on a board that this module holds. The
//! board is a local game. The army whose turn it is gives its orders with the
//! play mode controls, and the host sends each order back as a
//! [`PlannerEdit::Order`]. [`PlannerBoard`] runs the order through the AWVM
//! reducer, at the [`PlannerLuck`] that the host sets, so a power changes the
//! move ranges after it and an attack charges the power meters. After each
//! order, the viewpoint goes to the army whose turn it is, so each army can
//! play its turn in order.
//!
//! The player can also change the board outside the rules: set the HP of a
//! unit, or remove it. Undo and redo apply to each edit, orders included.
//!
//! When the previewed player's super power fires area strikes, the board also
//! shows where they would land now, and reports the strikes to the host. The
//! board draws the strikes in three parts. A dark shade under the units gets
//! darker for each strike that hits a tile. Each strike area has a cased
//! outline in the color of its missile. A numbered badge and corner brackets
//! mark the center tile of each strike, above the units. The host can also
//! show the HP that each unit keeps after all strikes.

mod board;
#[cfg(test)]
mod fixture;

use awbrn_bevy::replay::{AwbwUnitId, ReplayViewpoint};
use awbrn_bevy::world::GameMap;
use awbrn_map::Pos;
use awbrn_types::{AwbwGamePlayerId, ExactHp, GraphicalHp, UnitExt};
use awbw_replay::AwbwReplay;
use awvm::commander::{AreaStrikePolicy, PowerLevel};
use awvm::event::Event;
use awvm::query::{AreaStrike, area_strikes};
use awvm::semantic::{
    AwbwVisibility, Location, ObservedTransition, PlayerId, PowerState, State, UnitId,
};
use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

pub use board::{EditOutcome, EditRefused, PlannerBoard, PlannerEdit, PlannerLuck};

use crate::core::coords::{TILE_SIZE, position_to_world_translation};
use crate::core::{AppState, GameMode, LoadingState, RenderLayer, SpriteSize};
use crate::features::event_bus::EventSink;
use crate::loading::LiveMatchPlayer;
use crate::modes::play::PendingCommandRejected;
use crate::modes::play::inspect::InspectedUnit;
use crate::modes::replay::presentation::{CompleteTransitionCommand, ReplayAdvanceLock};
use crate::render::UiAtlasResource;
use crate::render::outline::{OUTLINE_SPRITE_SIZE, boundary_edges, inset_segment, spawn_segment};
use crate::render::units::HealthOverlayOverride;

/// A move planner board to open when its map is ready.
#[derive(Debug, Resource)]
pub(crate) struct PlannerToOpen {
    /// A replay with one game entry, which holds the board, and no turns.
    pub replay: AwbwReplay,
    pub previewed_player: Option<AwbwGamePlayerId>,
    /// How many powers each player's CO used before.
    pub power_uses: Vec<(AwbwGamePlayerId, u64)>,
}

/// The player whose area-strike power the board shows, if there is one.
///
/// This player does not have to be the one whose turn it is. While another
/// army plays, the strikes aim again after each order, so the board shows
/// where they would land if this player's super power started now.
#[derive(Debug, Resource, Clone, Copy, PartialEq, Eq, Default)]
pub struct AreaStrikePreview {
    pub player: Option<AwbwGamePlayerId>,
}

/// Whether each unit's health overlay shows the HP it keeps after all the
/// strikes, in place of its current HP.
#[derive(Debug, Resource, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlannerAfterStrikes(pub bool);

/// The preview and the view that the strikes on the screen were drawn for.
#[derive(Debug, Resource, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct ShownStrikeView {
    preview: AreaStrikePreview,
    after_strikes: PlannerAfterStrikes,
}

/// The board changes that the screen does not show yet, oldest first.
///
/// A move animates before the board shows its result, so the changes wait
/// until the animation is done.
#[derive(Debug, Resource, Default)]
pub(crate) struct PlannerPresentations(VecDeque<Vec<ObservedTransition>>);

/// Every sprite that the strike overlay draws.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AreaStrikeHighlight;

/// The shade on a tile that `hits` strikes hit.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AreaStrikeShade {
    pub hits: u8,
}

/// A segment of the outline around one strike area.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AreaStrikeOutline;

/// The firing number on the center tile of a strike, from 1.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AreaStrikeBadge {
    pub number: u8,
}

/// The state of the planner board, and the strikes of the previewed power, in
/// firing order.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[serde(rename_all = "camelCase")]
pub struct AreaStrikesPreviewed {
    /// The previewed player, if there is one.
    pub player_id: Option<u32>,
    /// Empty without a previewed player, when the previewed power fires no
    /// area strikes, or when the previewed player's power is active on its
    /// own turn.
    pub strikes: Vec<PreviewedAreaStrike>,
    /// Why there is no board or no strikes, if something failed.
    pub error: Option<String>,
    /// Whether the planner board has an edit to undo.
    pub can_undo: bool,
    /// Whether the planner board has an undone edit to redo.
    pub can_redo: bool,
}

/// The unit selected on the planner board, if there is one.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[serde(rename_all = "camelCase")]
pub struct PlannerSelectionChanged {
    pub unit: Option<PlannerSelectedUnit>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[serde(rename_all = "camelCase")]
pub struct PlannerSelectedUnit {
    pub unit_id: u32,
    pub unit_name: String,
    pub player_id: u32,
    /// Visible HP, from 1 to 10.
    pub hp: u8,
    pub x: u8,
    pub y: u8,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[serde(rename_all = "camelCase")]
pub struct PreviewedAreaStrike {
    #[cfg_attr(
        target_family = "wasm",
        tsify(type = "\"infantry-hp\" | \"unit-value\" | \"unit-hp\"")
    )]
    pub policy: AreaStrikePolicy,
    /// The center tile.
    pub x: u8,
    pub y: u8,
    pub radius: u32,
    /// Exact HP that the strike removes from each unit, in tenths of a
    /// visible HP.
    pub damage: u32,
    pub units: Vec<StruckUnit>,
}

/// A unit in a strike area, with its HP before and after that strike.
///
/// The HP is exact, in tenths of a visible HP. Strikes apply in order, so a
/// unit in two areas has the first strike's result as its HP before the
/// second.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[serde(rename_all = "camelCase")]
pub struct StruckUnit {
    pub player_id: u32,
    pub unit_name: String,
    pub x: u8,
    pub y: u8,
    pub hp_before: u32,
    pub hp_after: u32,
}

#[derive(Debug)]
pub struct PlannerPlugin;

impl Plugin for PlannerPlugin {
    fn build(&self, app: &mut App) {
        let on_planner_board = in_state(GameMode::Game)
            .and_then(in_state(AppState::InGame))
            .and_then(resource_exists::<PlannerBoard>);
        app.add_message::<PlannerEdit>()
            .init_resource::<PlannerPresentations>()
            .add_systems(
                OnEnter(LoadingState::Complete),
                open_planner_board
                    .after(crate::modes::play::initialize_live_semantic_world)
                    .run_if(in_state(GameMode::Game))
                    .run_if(resource_exists::<PlannerToOpen>),
            )
            .add_systems(OnExit(AppState::InGame), close_planner_board)
            // Orders and edits arrive between frames from the host, so the
            // board takes them before it reads new input.
            .add_systems(
                Update,
                (apply_planner_edits, refresh_strike_view)
                    .chain()
                    .before(crate::features::input::PointerSet::Recognize)
                    .run_if(on_planner_board.clone()),
            )
            .add_systems(
                Update,
                (
                    emit_planner_selection
                        .after(crate::modes::play::inspect::update_inspection_fields),
                    paint_badge_digits,
                )
                    .in_set(crate::features::input::PointerSet::Consume)
                    .run_if(on_planner_board),
            );
    }
}

const STRIKE_SHADE_SPRITE_SIZE: SpriteSize = SpriteSize {
    width: TILE_SIZE,
    height: TILE_SIZE,
    z_index: RenderLayer::MOVE_RANGE_OVERLAY,
};

const STRIKE_MARK_SPRITE_SIZE: SpriteSize = SpriteSize {
    width: TILE_SIZE,
    height: TILE_SIZE,
    z_index: RenderLayer::STRIKE_MARK,
};

/// The dark ink of the shade, the badges and the casings.
const STRIKE_INK: Color = Color::srgb(0.086, 0.094, 0.114);

/// The alpha of the shade on a tile that one, two or three strikes hit.
///
/// The shade has no hue, so overlapping areas do not mix into a new color.
/// More strikes give a darker tile, and the outlines say which strikes. The
/// alpha stays at 0.40 or less, so the terrain under the shade stays clear.
const STRIKE_SHADE_ALPHA: [f32; 3] = [0.14, 0.27, 0.40];

/// How far inside the tile edge each strike's outline runs, in firing order.
///
/// Where two areas share an edge, the two lines show side by side and do not
/// cover each other. The lines stay near the tile edge, because the unit
/// sprites cover the middle of a tile.
const STRIKE_OUTLINE_INSET: [f32; 3] = [0.5, 2.0, 3.5];

/// The width and height of the badge ring. The digit fills the inside.
const STRIKE_BADGE_SIZE: Vec2 = Vec2::new(10.0, 9.0);
/// The size of a UI atlas digit.
const STRIKE_DIGIT_SIZE: Vec2 = Vec2::new(8.0, 7.0);
/// How long each arm of a corner bracket is.
const STRIKE_BRACKET_ARM: f32 = 4.0;

/// The color of each missile, by policy.
///
/// The three colors are bright, and they get their contrast from the dark
/// casing. No army, terrain or inspection field uses them. The firing number
/// on the badge, not the color, is the main identity of a missile.
fn strike_color(policy: AreaStrikePolicy) -> Color {
    match policy {
        AreaStrikePolicy::InfantryHp => Color::srgb(1.0, 0.88, 0.30),
        AreaStrikePolicy::UnitValue => Color::srgb(1.0, 0.18, 0.82),
        AreaStrikePolicy::UnitHp => Color::WHITE,
    }
}

/// Make the planner board from the planner file and its map, and show it.
pub(crate) fn open_planner_board(world: &mut World) {
    let Some(planner) = world.remove_resource::<PlannerToOpen>() else {
        return;
    };
    world.insert_resource(AreaStrikePreview {
        player: planner.previewed_player,
    });
    world.init_resource::<PlannerAfterStrikes>();
    world.remove_resource::<PlannerBoard>();
    world.resource_mut::<PlannerPresentations>().0.clear();
    let opened = crate::loading::loaded_awbw_map(world)
        .ok_or_else(|| "the map of the planner board is not loaded".to_string())
        .and_then(|map| {
            awvm_awbw::initial_state(&planner.replay, map).map_err(|error| error.to_string())
        })
        .and_then(|state| with_power_uses(state, &planner.power_uses))
        .and_then(|state| {
            seed_presentation(world, &planner.replay, &state)?;
            Ok(state)
        });
    match opened {
        Ok(state) => {
            world.insert_resource(PlannerBoard::new(state));
            present_strikes(world);
        }
        Err(error) => {
            error!("Could not open the planner board: {error}");
            if let Some(sink) = world.get_resource::<EventSink<AreaStrikesPreviewed>>() {
                sink.emit(AreaStrikesPreviewed {
                    player_id: planner.previewed_player.map(|player| player.as_u32()),
                    strikes: Vec::new(),
                    error: Some(error),
                    can_undo: false,
                    can_redo: false,
                });
            }
        }
    }
}

/// `state` with the power use count of each player's active CO.
///
/// An AWBW game records the count only as the raised power costs, so the
/// planner sets it after the board is made.
fn with_power_uses(
    mut state: State,
    power_uses: &[(AwbwGamePlayerId, u64)],
) -> Result<State, String> {
    for (player, uses) in power_uses {
        let seat = state
            .player_index(&PlayerId::from(player.as_u32().to_string()))
            .ok_or_else(|| format!("player {} is not on the board", player.as_u32()))?;
        for commander in &mut state.player_mut(seat).commanders {
            if commander.active {
                commander.power_uses = *uses;
            }
        }
    }
    Ok(state)
}

/// Put the roster and the units of `state` on the screen, as the army whose
/// turn it is sees them.
fn seed_presentation(world: &mut World, replay: &AwbwReplay, state: &State) -> Result<(), String> {
    let mut players = replay
        .games
        .first()
        .map(|game| game.players.clone())
        .unwrap_or_default();
    players.sort_by_key(|player| player.order);
    let players = players
        .iter()
        .map(|player| LiveMatchPlayer {
            player_id: player.id.as_u32(),
            faction_id: player.faction.awbw_id().as_u8(),
        })
        .collect::<Vec<_>>();
    let transitions = observed(None, state, &[])?;
    let active = transitions
        .iter()
        .find(|transition| transition.post.recipient == state.turn.active_player)
        .ok_or_else(|| "the active player is not on the board".to_string())?;
    crate::modes::play::seed_live_presentation(world, &players, &active.post);
    CompleteTransitionCommand { transitions }.apply(world);
    Ok(())
}

/// The transition from `prior` to `state` for each player. Without a prior
/// state, each player sees `state` with no events.
fn observed(
    prior: Option<&State>,
    state: &State,
    events: &[Event],
) -> Result<Vec<ObservedTransition>, String> {
    state
        .players
        .iter()
        .map(|player| {
            match prior {
                Some(prior) => awvm::semantic::observe_transition(
                    &AwbwVisibility,
                    prior,
                    state,
                    events,
                    player.id(),
                ),
                None => awvm::semantic::observe(&AwbwVisibility, state, player.id()).map(|post| {
                    ObservedTransition {
                        post,
                        events: Vec::new(),
                    }
                }),
            }
            .map_err(|error| error.to_string())
        })
        .collect()
}

/// Apply the edits that arrived since the last frame, then show the next
/// board change when the last animation is done.
pub(crate) fn apply_planner_edits(
    world: &mut World,
    mut cursor: Local<MessageCursor<PlannerEdit>>,
) {
    let edits = cursor
        .read(world.resource::<Messages<PlannerEdit>>())
        .cloned()
        .collect::<Vec<_>>();
    if !edits.is_empty() {
        let luck = world
            .get_resource::<PlannerLuck>()
            .copied()
            .unwrap_or_default();
        let mut refused_order = false;
        world.resource_scope(|world, mut board: Mut<PlannerBoard>| {
            for edit in edits {
                match board.apply(&edit, luck) {
                    Ok(outcome) => {
                        let transitions = match outcome {
                            EditOutcome::Played { prior, events } => {
                                observed(Some(&prior), board.current(), &events)
                            }
                            EditOutcome::Replaced => observed(None, board.current(), &[]),
                        };
                        match transitions {
                            Ok(transitions) => world
                                .resource_mut::<PlannerPresentations>()
                                .0
                                .push_back(transitions),
                            Err(error) => error!("Could not show the planner board: {error}"),
                        }
                    }
                    Err(error) => {
                        warn!("Planner edit refused: {error}");
                        refused_order |= matches!(edit, PlannerEdit::Order(_));
                    }
                }
            }
        });
        // The play mode waits for the order that it sent. A refusal gives
        // the player the unit back, with its route.
        if refused_order {
            world.write_message(PendingCommandRejected);
        }
    }
    present_next_change(world);
}

/// Show the oldest board change that the screen does not show yet.
fn present_next_change(world: &mut World) {
    if world
        .get_resource::<ReplayAdvanceLock>()
        .is_some_and(ReplayAdvanceLock::is_active)
    {
        return;
    }
    let Some(transitions) = world.resource_mut::<PlannerPresentations>().0.pop_front() else {
        return;
    };
    // The army whose turn it is plays the board.
    if let Some(active) = transitions.first().and_then(|transition| {
        transition
            .post
            .turn
            .active_player
            .as_str()
            .parse::<u32>()
            .ok()
    }) {
        world.insert_resource(ReplayViewpoint::Player(AwbwGamePlayerId::new(active)));
    }
    CompleteTransitionCommand { transitions }.apply(world);
    if let Err(error) = world.run_system_once(crate::modes::play::settle_committed_unload) {
        error!("Could not settle the planner selection: {error}");
    }
    // The strikes aim at the newest board, so they wait until the screen shows
    // it.
    if world.resource::<PlannerPresentations>().0.is_empty() {
        present_strikes(world);
    }
}

/// Show the strikes again when the host picks another previewed player, or
/// turns the after-strikes view on or off.
pub(crate) fn refresh_strike_view(world: &mut World) {
    // A board change that the screen does not show yet aims the strikes again
    // when it is shown.
    if !world.resource::<PlannerPresentations>().0.is_empty() {
        return;
    }
    if world.get_resource::<ShownStrikeView>().copied() != Some(strike_view(world)) {
        present_strikes(world);
    }
}

fn strike_view(world: &World) -> ShownStrikeView {
    ShownStrikeView {
        preview: world
            .get_resource::<AreaStrikePreview>()
            .copied()
            .unwrap_or_default(),
        after_strikes: world
            .get_resource::<PlannerAfterStrikes>()
            .copied()
            .unwrap_or_default(),
    }
}

/// Aim the previewed power on the current board, draw its strikes and report
/// them.
fn present_strikes(world: &mut World) {
    let painted = world
        .query_filtered::<Entity, With<AreaStrikeHighlight>>()
        .iter(world)
        .collect::<Vec<_>>();
    for entity in painted {
        world.despawn(entity);
    }
    let overridden = world
        .query_filtered::<Entity, With<HealthOverlayOverride>>()
        .iter(world)
        .collect::<Vec<_>>();
    for entity in overridden {
        world.entity_mut(entity).remove::<HealthOverlayOverride>();
    }
    let view = strike_view(world);
    world.insert_resource(view);
    let preview = view.preview;

    let (strikes, can_undo, can_redo) = match world.get_resource::<PlannerBoard>() {
        Some(board) => (
            preview.player.map_or(Ok(Default::default()), |player| {
                preview_strikes(board.current(), player)
            }),
            board.can_undo(),
            board.can_redo(),
        ),
        None => (
            Err("the board has no authoritative state".into()),
            false,
            false,
        ),
    };
    let (strikes, error) = match strikes {
        Ok((strikes, after)) => {
            world.resource_scope(|world, game_map: Mut<GameMap>| {
                let mut commands = world.commands();
                spawn_strikes(&mut commands, &strikes, &game_map);
            });
            world.flush();
            if view.after_strikes.0 {
                show_hp_after_strikes(world, &after);
            }
            (strikes, None)
        }
        Err(error) => {
            error!("Could not preview area strikes: {error}");
            (Vec::new(), Some(error))
        }
    };
    if let Some(sink) = world.get_resource::<EventSink<AreaStrikesPreviewed>>() {
        sink.emit(AreaStrikesPreviewed {
            player_id: preview.player.map(|player| player.as_u32()),
            strikes,
            error,
            can_undo,
            can_redo,
        });
    }
}

/// Make each struck unit's health overlay show the exact HP in `after`.
fn show_hp_after_strikes(world: &mut World, after: &HashMap<UnitId, u8>) {
    let struck = world
        .query::<(Entity, &AwbwUnitId)>()
        .iter(world)
        .filter_map(|(entity, id)| {
            let hp = after.get(&UnitId::new(id.0.as_u32()))?;
            Some((entity, GraphicalHp::from(ExactHp::new(*hp))))
        })
        .collect::<Vec<_>>();
    for (entity, hp) in struck {
        world.entity_mut(entity).insert(HealthOverlayOverride(hp));
    }
}

/// Give each strike badge its firing number, in the UI atlas digits that a
/// unit's HP uses.
///
/// The board can open before the UI atlas is ready, so a badge gets its digit
/// here, on the first frame that the atlas is ready.
pub(crate) fn paint_badge_digits(
    mut commands: Commands,
    badges: Query<(Entity, &AreaStrikeBadge), Without<Sprite>>,
    atlas: Option<Res<UiAtlasResource>>,
    atlas_assets: Option<Res<Assets<crate::UiAtlasAsset>>>,
) {
    if badges.is_empty() {
        return;
    }
    let (Some(atlas), Some(atlas_assets)) = (atlas, atlas_assets) else {
        return;
    };
    let Some(index_map) = atlas_assets.get(&atlas.handle).map(|ui| ui.index_map()) else {
        return;
    };
    for (entity, badge) in &badges {
        let Some(index) = index_map.get(&format!("Healthv2/{}.png", badge.number)) else {
            continue;
        };
        commands.entity(entity).insert(Sprite::from_atlas_image(
            atlas.texture.clone(),
            TextureAtlas {
                layout: atlas.layout.clone(),
                index: *index,
            },
        ));
    }
}

/// Tell the host which unit is selected, and its state on the board.
pub(crate) fn emit_planner_selection(
    inspected: Res<InspectedUnit>,
    board: Res<PlannerBoard>,
    unit_ids: Query<&AwbwUnitId>,
    sink: Option<Res<EventSink<PlannerSelectionChanged>>>,
    mut emitted: Local<Option<PlannerSelectionChanged>>,
) {
    if !inspected.is_changed() && !board.is_changed() {
        return;
    }
    let state = board.current();
    let unit = inspected
        .0
        .and_then(|entity| unit_ids.get(entity).ok())
        .and_then(|id| state.units.get(UnitId::new(id.0.as_u32())))
        .and_then(|unit| {
            let Location::Board { position } = unit.location else {
                return None;
            };
            let player_id = state
                .players
                .get(unit.owner.get())
                .and_then(|owner| owner.id().as_str().parse().ok())?;
            Some(PlannerSelectedUnit {
                unit_id: unit.id.get(),
                unit_name: unit.kind.name().to_string(),
                player_id,
                hp: unit.hp.div_ceil(10),
                x: position.x,
                y: position.y,
            })
        });
    let event = PlannerSelectionChanged { unit };
    if emitted.as_ref() == Some(&event) {
        return;
    }
    *emitted = Some(event.clone());
    if let Some(sink) = sink {
        sink.emit(event);
    }
}

/// The strikes of the previewed player's super power, and the exact HP that
/// each struck unit keeps after all of them. A power that fires no area
/// strikes has none.
fn preview_strikes(
    state: &State,
    player: AwbwGamePlayerId,
) -> Result<(Vec<PreviewedAreaStrike>, HashMap<UnitId, u8>), String> {
    let player = PlayerId::from(player.as_u32().to_string());
    let seat = state
        .player_index(&player)
        .ok_or_else(|| "the previewed player is not on the board".to_string())?;
    // On its own turn, a player whose power is on already fired its strikes,
    // and cannot start a power again this turn. On another army's turn, the
    // power ends before this player moves again, so the strikes show where
    // they land if the super power starts now.
    if state.turn.active_player == player && state.player(seat).power_state != PowerState::None {
        return Ok((Vec::new(), HashMap::new()));
    }
    let strikes = area_strikes(state, &player, PowerLevel::Scop).map_err(|e| e.to_string())?;
    let (units, after) = struck_units(state, &strikes);
    let previewed = strikes
        .iter()
        .zip(units)
        .map(|(strike, units)| PreviewedAreaStrike {
            policy: strike.policy,
            x: strike.center.x,
            y: strike.center.y,
            radius: strike.radius as u32,
            damage: u32::from(strike.damage),
            units,
        })
        .collect();
    Ok((previewed, after))
}

/// The board units in each strike area, with the HP that each strike leaves,
/// and the exact HP that each struck unit keeps after all strikes.
fn struck_units(
    state: &State,
    strikes: &[AreaStrike],
) -> (Vec<Vec<StruckUnit>>, HashMap<UnitId, u8>) {
    let mut hp = state
        .units
        .iter()
        .map(|unit| (unit.id, unit.hp))
        .collect::<HashMap<_, _>>();
    let mut after = HashMap::new();
    let units = strikes
        .iter()
        .map(|strike| {
            state
                .units
                .iter()
                .filter_map(|unit| {
                    let Location::Board { position } = unit.location else {
                        return None;
                    };
                    if !strike.covers(position) {
                        return None;
                    }
                    let player_id = state
                        .players
                        .get(unit.owner.get())
                        .and_then(|owner| owner.id().as_str().parse().ok())?;
                    let current = hp.get_mut(&unit.id)?;
                    let before = *current;
                    *current = before.saturating_sub(strike.damage).max(1);
                    after.insert(unit.id, *current);
                    Some(StruckUnit {
                        player_id,
                        unit_name: unit.kind.name().to_string(),
                        x: position.x,
                        y: position.y,
                        hp_before: u32::from(before),
                        hp_after: u32::from(*current),
                    })
                })
                .collect()
        })
        .collect();
    (units, after)
}

/// The tiles of a strike area that are on the board.
fn strike_area(strike: &PreviewedAreaStrike, game_map: &GameMap) -> HashSet<Pos> {
    let center = Pos::new(strike.x, strike.y);
    let radius = strike.radius as i32;
    let mut area = HashSet::new();
    for dy in -radius..=radius {
        for dx in -(radius - dy.abs())..=(radius - dy.abs()) {
            let (Ok(x), Ok(y)) = (
                u8::try_from(i32::from(center.x) + dx),
                u8::try_from(i32::from(center.y) + dy),
            ) else {
                continue;
            };
            if x < game_map.width() && y < game_map.height() {
                area.insert(Pos::new(x, y));
            }
        }
    }
    area
}

/// Draw the shade, the outlines and the center marks of the strikes.
fn spawn_strikes(commands: &mut Commands, strikes: &[PreviewedAreaStrike], game_map: &GameMap) {
    let areas = strikes
        .iter()
        .map(|strike| strike_area(strike, game_map))
        .collect::<Vec<_>>();

    // One shade per tile, darker for each strike that hits it.
    let mut hits = HashMap::<Pos, u8>::new();
    for tile in areas.iter().flatten() {
        *hits.entry(*tile).or_default() += 1;
    }
    for (tile, count) in hits {
        let alpha = STRIKE_SHADE_ALPHA[usize::from(count).clamp(1, 3) - 1];
        commands.spawn((
            AreaStrikeHighlight,
            AreaStrikeShade { hits: count },
            Sprite::from_color(STRIKE_INK.with_alpha(alpha), Vec2::splat(TILE_SIZE)),
            STRIKE_SHADE_SPRITE_SIZE,
            Transform::from_translation(position_to_world_translation(
                &STRIKE_SHADE_SPRITE_SIZE,
                tile,
                game_map,
            )),
        ));
    }

    for (order, (strike, area)) in strikes.iter().zip(&areas).enumerate() {
        let color = strike_color(strike.policy);
        let inset = STRIKE_OUTLINE_INSET[order.min(STRIKE_OUTLINE_INSET.len() - 1)];
        let mut tiles = area.iter().copied().collect::<Vec<_>>();
        tiles.sort();
        for tile in tiles {
            let center = position_to_world_translation(&OUTLINE_SPRITE_SIZE, tile, game_map);
            for index in boundary_edges(tile, area) {
                let (size, offset) = inset_segment(tile, index, area, inset);
                spawn_segment(
                    commands,
                    (AreaStrikeHighlight, AreaStrikeOutline),
                    color,
                    size,
                    center + offset,
                );
            }
        }
    }

    // Strikes can share a center tile, so the marks of each center tile are
    // drawn together.
    let mut centers = Vec::<(Pos, Vec<(usize, Color)>)>::new();
    for (order, strike) in strikes.iter().enumerate() {
        let center = Pos::new(strike.x, strike.y);
        let mark = (order, strike_color(strike.policy));
        match centers.iter_mut().find(|(tile, _)| *tile == center) {
            Some((_, marks)) => marks.push(mark),
            None => centers.push((center, vec![mark])),
        }
    }
    for (center, marks) in centers {
        spawn_center_marks(commands, center, &marks, game_map);
    }
}

/// The corners of a tile, as `(x, y)` signs, in the order that the badges of
/// the strikes on one center tile take them.
const STRIKE_MARK_CORNERS: [(f32, f32); 4] = [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)];

/// Mark a center tile of the strikes in `marks`, as `(firing order, color)`,
/// above the units.
///
/// Each strike has a badge in its own corner of the tile, which holds its
/// firing number in the digits of the unit HP. The first strike takes the
/// top-right corner, and the next ones go around the tile, so badges on one
/// tile never cover each other. Brackets in the first strike's color fill the
/// other corners, and make the tile read as an aim point and not as a
/// selected tile.
fn spawn_center_marks(
    commands: &mut Commands,
    center: Pos,
    marks: &[(usize, Color)],
    game_map: &GameMap,
) {
    let middle = position_to_world_translation(&STRIKE_MARK_SPRITE_SIZE, center, game_map);
    let half = TILE_SIZE * 0.5;
    let Some(&(_, bracket_color)) = marks.first() else {
        return;
    };

    for &(sx, sy) in STRIKE_MARK_CORNERS.iter().skip(marks.len()) {
        let arm = STRIKE_BRACKET_ARM;
        for (size, offset) in [
            (
                Vec2::new(arm, 1.0),
                Vec2::new(sx * (half - arm * 0.5), sy * (half - 0.5)),
            ),
            (
                Vec2::new(1.0, arm),
                Vec2::new(sx * (half - 0.5), sy * (half - arm * 0.5)),
            ),
        ] {
            spawn_segment(
                commands,
                AreaStrikeHighlight,
                bracket_color,
                size,
                middle + offset.extend(0.0),
            );
        }
    }

    for (&(order, color), &(sx, sy)) in marks.iter().zip(&STRIKE_MARK_CORNERS) {
        // The ring is flush with its corner of the tile. The ink square and
        // the digit fill the inside of the ring.
        let corner = Vec2::new(sx, sy) * (Vec2::splat(half) - STRIKE_BADGE_SIZE * 0.5);
        let badge = middle + corner.extend(0.0);
        commands.spawn((
            AreaStrikeHighlight,
            Sprite::from_color(color, STRIKE_BADGE_SIZE),
            STRIKE_MARK_SPRITE_SIZE,
            Transform::from_translation(badge),
        ));
        commands.spawn((
            AreaStrikeHighlight,
            Sprite::from_color(STRIKE_INK, STRIKE_DIGIT_SIZE),
            STRIKE_MARK_SPRITE_SIZE,
            Transform::from_translation(badge + Vec3::Z * 0.0002),
        ));
        // `paint_badge_digits` gives this entity its digit sprite.
        commands.spawn((
            AreaStrikeHighlight,
            AreaStrikeBadge {
                number: (order + 1) as u8,
            },
            STRIKE_MARK_SPRITE_SIZE,
            Transform::from_translation(badge + Vec3::Z * 0.0004),
        ));
    }
}

/// Close the planner board when the board leaves the screen.
pub(crate) fn close_planner_board(
    mut commands: Commands,
    mut presentations: ResMut<PlannerPresentations>,
    highlights: Query<Entity, With<AreaStrikeHighlight>>,
    overridden: Query<Entity, With<HealthOverlayOverride>>,
) {
    commands.remove_resource::<PlannerBoard>();
    commands.remove_resource::<AreaStrikePreview>();
    commands.remove_resource::<ShownStrikeView>();
    presentations.0.clear();
    for entity in &highlights {
        commands.entity(entity).try_despawn();
    }
    for entity in &overridden {
        commands
            .entity(entity)
            .try_remove::<HealthOverlayOverride>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::CorePlugin;
    use crate::features::player_roster::PlayerPowerMeters;
    use crate::features::visibility::VisibilityPlugin;
    use crate::modes::planner::fixture::{ADDER, Cowboy, JESS, RACHEL, cowboy, cowboy_seated};
    use crate::modes::play::PlayPlugin;
    use crate::modes::replay::ReplayPlugin;
    use crate::modes::replay::presentation::ReplayFollowupCommand;
    use awbrn_bevy::MapPosition;
    use awbrn_bevy::world::{CarriedBy, UnitActive};
    use awbrn_game::{GameCommand, PostMoveAction, ServerUnitId};
    use awbrn_map::{AwbrnMap, AwbwMap};
    use awbrn_types::{AwbwCoId, PlayerFaction};
    use bevy::state::app::StatesPlugin;
    use std::sync::{Arc, Mutex};

    struct PlannerApp {
        app: App,
        reports: Arc<Mutex<Vec<AreaStrikesPreviewed>>>,
        player: AwbwGamePlayerId,
    }

    /// A play app with the cowboy planner board open, on Blue Moon's turn,
    /// with Blue Moon as `co_id` and `stars` power stars. The board previews
    /// Blue Moon's strikes.
    fn planner_app_as(co_id: AwbwCoId, stars: u64) -> PlannerApp {
        let fixture = cowboy(co_id, stars);
        let player = fixture.player;
        planner_app_with(fixture, Some(player), Vec::new())
    }

    /// A play app with `fixture` open, which previews the strikes of
    /// `previewed`, with the power use counts of `power_uses`.
    fn planner_app_with(
        fixture: Cowboy,
        previewed: Option<AwbwGamePlayerId>,
        power_uses: Vec<(AwbwGamePlayerId, u64)>,
    ) -> PlannerApp {
        let mut app = App::new();
        app.add_plugins((
            StatesPlugin,
            CorePlugin,
            ReplayPlugin,
            PlayPlugin,
            VisibilityPlugin,
        ));
        app.init_resource::<InspectedUnit>()
            .add_message::<PlannerEdit>()
            .init_resource::<PlannerPresentations>();
        app.add_systems(
            Update,
            (apply_planner_edits, refresh_strike_view, paint_badge_digits).chain(),
        );
        insert_test_atlas(&mut app);
        let reports = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&reports);
        app.insert_resource(EventSink::new(move |report: AreaStrikesPreviewed| {
            sink.lock().unwrap().push(report);
        }));
        app.world_mut()
            .resource_mut::<GameMap>()
            .set(AwbrnMap::from_map(
                &AwbwMap::try_from(&fixture.map).unwrap(),
            ));
        crate::loading::insert_loaded_awbw_map(app.world_mut(), fixture.map);
        awbrn_bevy::world::initialize_terrain_semantic_world(app.world_mut());
        app.insert_resource(PlannerToOpen {
            replay: fixture.replay,
            previewed_player: previewed,
            power_uses,
        });
        open_planner_board(app.world_mut());
        PlannerApp {
            app,
            reports,
            player: fixture.player,
        }
    }

    fn planner_app() -> PlannerApp {
        planner_app_as(RACHEL, 0)
    }

    /// A UI atlas that holds only the digits of the strike badges and the
    /// course arrows of a move.
    fn insert_test_atlas(app: &mut App) {
        let mut atlas_assets = Assets::<crate::UiAtlasAsset>::default();
        let digits = (1..=3).map(|digit| format!("Healthv2/{digit}.png"));
        let arrows = ["Arrow_Body.png", "Arrow_Curved.png", "Arrow_Tip.png"].map(String::from);
        let handle = atlas_assets.add(crate::UiAtlasAsset(awbrn_content::UiAtlasManifest {
            size: crate::UiAtlasSize {
                width: 64,
                height: 8,
            },
            sprites: digits
                .chain(arrows)
                .enumerate()
                .map(|(index, name)| crate::UiAtlasSprite {
                    name,
                    x: index as u32 * 8,
                    y: 0,
                    width: 8,
                    height: 7,
                })
                .collect(),
        }));
        app.insert_resource(UiAtlasResource {
            handle,
            texture: Handle::default(),
            layout: Handle::default(),
        });
        app.insert_resource(atlas_assets);
    }

    fn atlas_index(app: &App, sprite_name: &str) -> usize {
        let atlas = app.world().resource::<UiAtlasResource>();
        let assets = app.world().resource::<Assets<crate::UiAtlasAsset>>();
        assets.get(&atlas.handle).unwrap().index_map()[sprite_name]
    }

    /// The hit count and the shade alpha of each shaded tile.
    fn shades(app: &mut App) -> Vec<(u8, f32)> {
        app.world_mut()
            .query::<(&AreaStrikeShade, &Sprite)>()
            .iter(app.world())
            .map(|(shade, sprite)| (shade.hits, sprite.color.alpha()))
            .collect()
    }

    fn outline_sprites(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<(), With<AreaStrikeOutline>>()
            .iter(app.world())
            .count()
    }

    /// The firing number and the atlas digit of each badge, in firing order.
    fn badges(app: &mut App) -> Vec<(u8, usize)> {
        let mut badges = app
            .world_mut()
            .query::<(&AreaStrikeBadge, Option<&Sprite>)>()
            .iter(app.world())
            .map(|(badge, sprite)| {
                let digit = sprite.and_then(|sprite| sprite.texture_atlas.as_ref());
                (badge.number, digit.map_or(usize::MAX, |atlas| atlas.index))
            })
            .collect::<Vec<_>>();
        badges.sort();
        badges
    }

    /// The tiles that the reported strikes hit, with the number of hits.
    fn reported_hits(app: &App, report: &AreaStrikesPreviewed) -> HashMap<Pos, u8> {
        let game_map = app.world().resource::<GameMap>();
        let mut hits = HashMap::new();
        for strike in &report.strikes {
            for tile in strike_area(strike, game_map) {
                *hits.entry(tile).or_default() += 1;
            }
        }
        hits
    }

    /// Check that each unit's health overlay override is the HP that the
    /// reported strikes leave it, and that units out of the strikes have none.
    fn assert_overrides_follow(app: &mut App, report: &AreaStrikesPreviewed) {
        let mut after = HashMap::new();
        for unit in report.strikes.iter().flat_map(|strike| &strike.units) {
            after.insert(Pos::new(unit.x, unit.y), unit.hp_after);
        }
        let mut overridden = 0;
        for (position, shown) in app
            .world_mut()
            .query_filtered::<
                (&MapPosition, Option<&HealthOverlayOverride>),
                (With<AwbwUnitId>, Without<CarriedBy>),
            >()
            .iter(app.world())
        {
            let expected = after
                .get(&position.position())
                .map(|hp| HealthOverlayOverride(GraphicalHp::from(ExactHp::new(*hp as u8))));
            assert_eq!(shown.copied(), expected, "at {:?}", position.position());
            overridden += usize::from(shown.is_some());
        }
        assert_eq!(overridden, after.len());
    }

    fn overrides(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<(), With<HealthOverlayOverride>>()
            .iter(app.world())
            .count()
    }

    fn unit_entity(app: &mut App, unit: UnitId) -> Option<Entity> {
        app.world_mut()
            .query::<(Entity, &AwbwUnitId)>()
            .iter(app.world())
            .find(|(_, id)| id.0.as_u32() == unit.get())
            .map(|(entity, _)| entity)
    }

    fn unit_tile(app: &App, entity: Entity) -> Pos {
        app.world().get::<MapPosition>(entity).unwrap().position()
    }

    fn unit_at(state: &State, position: Pos) -> UnitId {
        state
            .units
            .iter()
            .find(|unit| unit.location == Location::Board { position })
            .unwrap()
            .id
    }

    fn board(app: &App) -> &State {
        app.world().resource::<PlannerBoard>().current()
    }

    fn viewpoint(app: &App) -> ReplayViewpoint {
        app.world().resource::<ReplayViewpoint>().clone()
    }

    /// End the movement animation of `entity`, as the animation system does
    /// at the end of the path.
    fn finish_animation(app: &mut App, entity: Entity) {
        let followup = app
            .world_mut()
            .resource_mut::<ReplayAdvanceLock>()
            .release_for(entity)
            .expect("the move animates");
        ReplayFollowupCommand {
            transitions: followup.transitions,
        }
        .apply(app.world_mut());
    }

    /// A Blue Moon tank.
    const TANK: Pos = Pos::new(15, 10);

    #[test]
    fn a_move_planner_board_shows_where_covering_fire_lands() {
        let PlannerApp {
            mut app,
            reports,
            player,
        } = planner_app();

        // Blue Moon has the turn, and the board shows it as Blue Moon sees it.
        assert_eq!(viewpoint(&app), ReplayViewpoint::Player(player));
        let reports = reports.lock().unwrap();
        let [report] = reports.as_slice() else {
            panic!("expected one report, got {reports:?}");
        };
        assert_eq!(report.error, None);
        assert!(!report.can_undo && !report.can_redo);
        let centers = report
            .strikes
            .iter()
            .map(|strike| (strike.policy, Pos::new(strike.x, strike.y)))
            .collect::<Vec<_>>();
        assert_eq!(
            centers,
            [
                (AreaStrikePolicy::InfantryHp, Pos::new(11, 12)),
                (AreaStrikePolicy::UnitValue, Pos::new(12, 14)),
                (AreaStrikePolicy::UnitHp, Pos::new(12, 13)),
            ]
        );
        // Each strike lowers a unit by three visible HP, to no less than 0.1.
        for unit in report.strikes.iter().flat_map(|strike| &strike.units) {
            assert_eq!(unit.hp_after, unit.hp_before.saturating_sub(30).max(1));
        }

        // One shade per struck tile, darker for each hit.
        let shades = shades(&mut app);
        assert_eq!(shades.len(), 23);
        assert_eq!(shades.iter().filter(|(hits, _)| *hits >= 2).count(), 12);
        assert_eq!(shades.iter().filter(|(hits, _)| *hits == 3).count(), 4);
        for (hits, alpha) in shades {
            assert_eq!(alpha, STRIKE_SHADE_ALPHA[usize::from(hits) - 1]);
        }

        // Each area of thirteen tiles has twenty outer edges. Each edge is a
        // line on a casing.
        assert_eq!(outline_sprites(&mut app), 3 * 20 * 2);

        // One badge per strike, with its firing number. The badge gets its
        // digit on the next frame.
        app.update();
        let expected = (1..=3)
            .map(|number| (number, atlas_index(&app, &format!("Healthv2/{number}.png"))))
            .collect::<Vec<_>>();
        assert_eq!(badges(&mut app), expected);

        // The view of the current HP has no overrides.
        assert_eq!(overrides(&mut app), 0);
    }

    #[test]
    fn a_move_order_animates_then_the_board_shows_it_and_aims_again() {
        let PlannerApp {
            mut app, reports, ..
        } = planner_app();
        let state = board(&app).clone();
        let tank = unit_at(&state, TANK);
        let field = awvm::query::reachable(&state, tank).unwrap();
        let to = field
            .destinations()
            .map(|(position, _)| position)
            .max_by_key(|position| (position.distance(TANK), *position))
            .unwrap();
        let entity = unit_entity(&mut app, tank).unwrap();

        app.world_mut()
            .write_message(PlannerEdit::Order(GameCommand::MoveUnit {
                unit_id: ServerUnitId(u64::from(tank.get())),
                path: field.path_to(to).unwrap(),
                action: Some(PostMoveAction::Wait),
            }));
        app.update();

        // The board has the move at once. The screen shows it after the
        // animation.
        assert_eq!(unit_at(board(&app), to), tank);
        assert!(app.world().resource::<ReplayAdvanceLock>().is_active());
        finish_animation(&mut app, entity);
        assert_eq!(unit_tile(&app, entity), to);
        assert!(!app.world().entity(entity).contains::<UnitActive>());

        let moved = reports.lock().unwrap().last().cloned().unwrap();
        assert_eq!(reports.lock().unwrap().len(), 2);
        assert!(moved.can_undo && !moved.can_redo);
        // The old overlay is replaced, not added to.
        assert_eq!(shades(&mut app).len(), reported_hits(&app, &moved).len());
        assert_eq!(outline_sprites(&mut app), 3 * 20 * 2);
        assert_eq!(badges(&mut app).len(), 3);

        // An undo puts the unit back, with no animation.
        app.world_mut().write_message(PlannerEdit::Undo);
        app.update();
        assert_eq!(unit_tile(&app, entity), TANK);
        assert!(app.world().entity(entity).contains::<UnitActive>());
        let reports = reports.lock().unwrap();
        let report = reports.last().unwrap();
        assert!(!report.can_undo && report.can_redo);
        assert_eq!(report.strikes, reports[0].strikes);
    }

    #[test]
    fn the_strikes_wait_for_the_screen_to_show_each_queued_change() {
        let PlannerApp {
            mut app, reports, ..
        } = planner_app();
        let state = board(&app).clone();
        let tank = unit_at(&state, TANK);
        let field = awvm::query::reachable(&state, tank).unwrap();
        let to = field
            .destinations()
            .map(|(position, _)| position)
            .max_by_key(|position| (position.distance(TANK), *position))
            .unwrap();
        let other = state
            .units
            .iter()
            .find(|unit| unit.id != tank && matches!(unit.location, Location::Board { .. }))
            .unwrap()
            .id;
        let entity = unit_entity(&mut app, tank).unwrap();

        // The delete arrives while the move animates.
        app.world_mut()
            .write_message(PlannerEdit::Order(GameCommand::MoveUnit {
                unit_id: ServerUnitId(u64::from(tank.get())),
                path: field.path_to(to).unwrap(),
                action: Some(PostMoveAction::Wait),
            }));
        app.world_mut()
            .write_message(PlannerEdit::Delete { unit: other });
        app.update();
        assert!(app.world().resource::<ReplayAdvanceLock>().is_active());
        assert_eq!(reports.lock().unwrap().len(), 1);

        // The strikes aim again when the screen shows the delete too.
        finish_animation(&mut app, entity);
        app.update();
        assert!(unit_entity(&mut app, other).is_none());
        let reports = reports.lock().unwrap();
        assert_eq!(reports.len(), 2);
        assert!(reports[1].can_undo);
    }

    #[test]
    fn a_power_order_shows_on_the_meter_and_end_turn_hands_over_the_board() {
        let cost = awvm::commander::power_activation_cost(
            awvm::ruleset::CommanderKind::Adder,
            PowerLevel::Cop,
            0,
        )
        .unwrap()
        .unwrap();
        let star = awvm::commander::power_star_charge(0).unwrap();
        let PlannerApp {
            mut app,
            reports,
            player,
        } = planner_app_as(ADDER, cost / star);
        let meter = |app: &App| {
            app.world()
                .resource::<PlayerPowerMeters>()
                .0
                .get(&player)
                .cloned()
                .unwrap()
        };
        assert_eq!(u64::from(meter(&app).charge), cost);
        assert_eq!(meter(&app).active_power, None);
        // Adder's super power fires no area strikes, so there is nothing to
        // aim.
        assert!(reports.lock().unwrap()[0].strikes.is_empty());
        assert_eq!(reports.lock().unwrap()[0].error, None);

        app.world_mut()
            .write_message(PlannerEdit::Order(GameCommand::ActivatePower {
                level: PowerLevel::Cop,
            }));
        app.update();

        assert_eq!(meter(&app).active_power, Some(PowerLevel::Cop));
        assert_eq!(meter(&app).charge, 0);
        assert_eq!(viewpoint(&app), ReplayViewpoint::Player(player));

        app.world_mut()
            .write_message(PlannerEdit::Order(GameCommand::EndTurn));
        app.update();

        let next = board(&app).turn.active_player.as_str().parse().unwrap();
        assert_ne!(next, player.as_u32());
        assert_eq!(
            viewpoint(&app),
            ReplayViewpoint::Player(AwbwGamePlayerId::new(next))
        );
        // A power lasts until its army's next turn.
        assert_eq!(meter(&app).active_power, Some(PowerLevel::Cop));
    }

    #[test]
    fn a_refused_order_gives_the_unit_back_to_the_play_mode() {
        let PlannerApp { mut app, .. } = planner_app();
        let initial = board(&app).clone();

        app.world_mut()
            .write_message(PlannerEdit::Order(GameCommand::ActivatePower {
                level: PowerLevel::Scop,
            }));
        app.update();

        assert_eq!(board(&app), &initial);
        let rejected = app
            .world()
            .resource::<Messages<PendingCommandRejected>>()
            .iter_current_update_messages()
            .count();
        assert_eq!(rejected, 1);
    }

    #[test]
    fn the_after_strikes_view_shows_the_hp_each_unit_keeps() {
        let PlannerApp {
            mut app, reports, ..
        } = planner_app();
        let state = board(&app).clone();
        // All three missiles hit the Orange Star artillery at full HP.
        let artillery = unit_entity(&mut app, unit_at(&state, Pos::new(11, 13))).unwrap();
        assert_eq!(app.world().get::<HealthOverlayOverride>(artillery), None);

        app.insert_resource(PlannerAfterStrikes(true));
        app.update();
        assert_eq!(
            app.world().get::<HealthOverlayOverride>(artillery),
            Some(&HealthOverlayOverride(GraphicalHp::from(ExactHp::new(1))))
        );
        let first = reports.lock().unwrap().last().cloned().unwrap();
        assert_overrides_follow(&mut app, &first);

        app.insert_resource(PlannerAfterStrikes(false));
        app.update();
        assert_eq!(overrides(&mut app), 0);

        // With the view on, an edit changes the strikes and the HP shown.
        app.insert_resource(PlannerAfterStrikes(true));
        let tank = unit_at(&state, Pos::new(11, 14));
        app.world_mut()
            .write_message(PlannerEdit::Delete { unit: tank });
        app.update();
        assert_eq!(unit_entity(&mut app, tank), None);
        let edited = reports.lock().unwrap().last().cloned().unwrap();
        assert_ne!(edited.strikes, first.strikes);
        assert_overrides_follow(&mut app, &edited);

        app.world_mut().write_message(PlannerEdit::Undo);
        app.update();
        let undone = reports.lock().unwrap().last().cloned().unwrap();
        assert_eq!(undone.strikes, first.strikes);
        assert_overrides_follow(&mut app, &undone);

        app.world_mut().write_message(PlannerEdit::Redo);
        app.update();
        let redone = reports.lock().unwrap().last().cloned().unwrap();
        assert_eq!(redone.strikes, edited.strikes);
        assert_overrides_follow(&mut app, &redone);

        app.world_mut().write_message(PlannerEdit::Reset);
        app.update();
        let reset = reports.lock().unwrap().last().cloned().unwrap();
        assert_eq!(reset.strikes, first.strikes);
        assert_overrides_follow(&mut app, &reset);
    }

    #[test]
    fn rachels_missiles_aim_again_while_jess_plays_and_land_where_previewed() {
        // Jess and Orange Star play first. Rachel and Blue Moon hold enough
        // charge for Covering Fire.
        let scop = awvm::commander::power_activation_cost(
            awvm::ruleset::CommanderKind::Rachel,
            PowerLevel::Scop,
            0,
        )
        .unwrap()
        .unwrap();
        let star = awvm::commander::power_star_charge(0).unwrap();
        let fixture = cowboy_seated(
            &[
                (PlayerFaction::OrangeStar, JESS, 0),
                (PlayerFaction::BlueMoon, RACHEL, scop / star),
            ],
            PlayerFaction::OrangeStar,
        );
        let (jess, rachel) = (fixture.id(PlayerFaction::OrangeStar), fixture.player);
        let PlannerApp {
            mut app, reports, ..
        } = planner_app_with(fixture, Some(rachel), Vec::new());
        let last = |reports: &Arc<Mutex<Vec<AreaStrikesPreviewed>>>| {
            reports.lock().unwrap().last().cloned().unwrap()
        };

        assert_eq!(viewpoint(&app), ReplayViewpoint::Player(jess));
        let opening = last(&reports);
        assert_eq!(opening.player_id, Some(rachel.as_u32()));
        assert_eq!(opening.strikes.len(), 3);
        let struck = |report: &AreaStrikesPreviewed, tile: Pos| {
            report
                .strikes
                .iter()
                .flat_map(|strike| &strike.units)
                .any(|unit| Pos::new(unit.x, unit.y) == tile)
        };
        // The Orange Star tank in the funds missile's area.
        let tank_tile = Pos::new(11, 14);
        assert!(struck(&opening, tank_tile));

        // Jess moves the tank away, and the missiles aim again on her board.
        let state = board(&app).clone();
        let tank = unit_at(&state, tank_tile);
        let field = awvm::query::reachable(&state, tank).unwrap();
        let to = field
            .destinations()
            .map(|(position, _)| position)
            .max_by_key(|position| (position.distance(tank_tile), *position))
            .unwrap();
        let entity = unit_entity(&mut app, tank).unwrap();
        app.world_mut()
            .write_message(PlannerEdit::Order(GameCommand::MoveUnit {
                unit_id: ServerUnitId(u64::from(tank.get())),
                path: field.path_to(to).unwrap(),
                action: Some(PostMoveAction::Wait),
            }));
        app.update();
        finish_animation(&mut app, entity);
        let moved = last(&reports);
        assert_eq!(moved.player_id, Some(rachel.as_u32()));
        assert!(!struck(&moved, tank_tile));
        assert_eq!(shades(&mut app).len(), reported_hits(&app, &moved).len());

        // Rachel's turn. The missiles land where the preview said.
        app.world_mut()
            .write_message(PlannerEdit::Order(GameCommand::EndTurn));
        app.update();
        assert_eq!(viewpoint(&app), ReplayViewpoint::Player(rachel));
        let before_power = last(&reports);
        assert_eq!(before_power.strikes.len(), 3);
        let mut predicted = HashMap::new();
        for unit in before_power.strikes.iter().flat_map(|strike| &strike.units) {
            predicted.insert(Pos::new(unit.x, unit.y), unit.hp_after);
        }
        app.world_mut()
            .write_message(PlannerEdit::Order(GameCommand::ActivatePower {
                level: PowerLevel::Scop,
            }));
        app.update();
        for (tile, hp) in predicted {
            let unit = board(&app).units.get(unit_at(board(&app), tile)).unwrap();
            assert_eq!(u32::from(unit.hp), hp, "at {tile:?}");
        }
        // Her power is on, so she has no missiles left to preview this turn.
        assert!(last(&reports).strikes.is_empty());
        assert_eq!(shades(&mut app).len(), 0);

        // On the next army's turn, the preview is back: her power ends before
        // she moves again.
        app.world_mut()
            .write_message(PlannerEdit::Order(GameCommand::EndTurn));
        app.update();
        assert_eq!(last(&reports).strikes.len(), 3);
    }

    #[test]
    fn the_previewed_army_can_change_on_an_open_board() {
        let fixture = cowboy(RACHEL, 0);
        let rachel = fixture.player;
        let PlannerApp {
            mut app, reports, ..
        } = planner_app_with(fixture, None, Vec::new());
        let last = || reports.lock().unwrap().last().cloned().unwrap();

        // Without a previewed army, the host still gets the undo state.
        assert_eq!(last().player_id, None);
        assert!(last().strikes.is_empty());
        assert_eq!(shades(&mut app).len(), 0);

        app.insert_resource(AreaStrikePreview {
            player: Some(rachel),
        });
        app.update();
        assert_eq!(last().player_id, Some(rachel.as_u32()));
        assert_eq!(last().strikes.len(), 3);
        let count = reports.lock().unwrap().len();

        // Nothing changed, so nothing is drawn again.
        app.update();
        assert_eq!(reports.lock().unwrap().len(), count);

        app.insert_resource(AreaStrikePreview { player: None });
        app.update();
        assert!(last().strikes.is_empty());
        assert_eq!(shades(&mut app).len(), 0);
    }

    #[test]
    fn a_used_power_raises_the_cost_on_the_meter() {
        let fixture = cowboy(RACHEL, 0);
        let rachel = fixture.player;
        let PlannerApp { app, .. } = planner_app_with(fixture, Some(rachel), vec![(rachel, 1)]);

        let cost = |uses| {
            awvm::commander::power_activation_cost(
                awvm::ruleset::CommanderKind::Rachel,
                PowerLevel::Scop,
                uses,
            )
            .unwrap()
            .unwrap()
        };
        assert!(cost(1) > cost(0));
        let meter = app
            .world()
            .resource::<PlayerPowerMeters>()
            .0
            .get(&rachel)
            .cloned()
            .unwrap();
        assert_eq!(meter.scop_cost.map(u64::from), Some(cost(1)));
    }

    #[test]
    fn badges_on_one_center_tile_take_their_own_corners() {
        let PlannerApp { mut app, .. } = planner_app();
        let painted = app
            .world_mut()
            .query_filtered::<Entity, With<AreaStrikeHighlight>>()
            .iter(app.world())
            .collect::<Vec<_>>();
        for entity in painted {
            app.world_mut().despawn(entity);
        }
        let strike = |policy| PreviewedAreaStrike {
            policy,
            x: 12,
            y: 13,
            radius: 2,
            damage: 30,
            units: Vec::new(),
        };
        let strikes = [
            strike(AreaStrikePolicy::InfantryHp),
            strike(AreaStrikePolicy::UnitValue),
            strike(AreaStrikePolicy::UnitHp),
        ];

        app.world_mut()
            .resource_scope(|world, game_map: Mut<GameMap>| {
                let mut commands = world.commands();
                spawn_strikes(&mut commands, &strikes, &game_map);
            });
        app.world_mut().flush();

        let middle = position_to_world_translation(
            &STRIKE_MARK_SPRITE_SIZE,
            Pos::new(12, 13),
            app.world().resource::<GameMap>(),
        );
        let mut badges = app
            .world_mut()
            .query::<(&AreaStrikeBadge, &Transform)>()
            .iter(app.world())
            .map(|(badge, transform)| (badge.number, transform.translation - middle))
            .collect::<Vec<_>>();
        badges.sort_by_key(|(number, _)| *number);
        let corners = badges
            .iter()
            .map(|(_, offset)| (offset.x.signum(), offset.y.signum()))
            .collect::<Vec<_>>();
        // Top-right, top-left and bottom-left, inside the center tile.
        assert_eq!(corners, [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0)]);
        for (_, offset) in badges {
            assert!(offset.x.abs() + STRIKE_BADGE_SIZE.x * 0.5 <= TILE_SIZE * 0.5);
            assert!(offset.y.abs() + STRIKE_BADGE_SIZE.y * 0.5 <= TILE_SIZE * 0.5);
        }
    }
}
