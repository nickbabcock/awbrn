//! Playing a turn without a pointer.
//!
//! The board already answers one question at a time — which unit, which tile,
//! which order — and the keyboard answers the same three. Nothing here decides
//! what is legal or what an order costs: a key names a tile, and the tile goes
//! through the same [`handle_tap`] a press goes through.
//!
//! The scheme:
//!
//! | Key | What it does |
//! | --- | --- |
//! | Arrows, WASD, numpad | Move the cursor one tile |
//! | Shift + the above | Draw the route by hand, tile by tile |
//! | Backspace | Take back the last hand-drawn tile |
//! | Tab, `E` / `Q` | Next / previous unit that can still act, then the bases |
//! | Enter, Space | Answer the tile under the cursor |
//! | Ctrl + Enter | Ask to end the turn |
//! | Escape | Step back one stage |
//!
//! A base the cycle stops on takes the cursor and nothing else: the build order
//! is a page element that takes the keyboard when it opens, so opening it on the
//! way past would end the walk. Enter opens it, the same key that answers every
//! other tile.
//!
//! Ending a turn is the one move that cannot be taken back, so no key here
//! ends one. The chord asks, and the page puts the question — with what is
//! still in hand named in it — before anything is sent.
//!
//! Escape and Backspace are read where the state they act on lives, in
//! [`super::clear_selection_on_escape`] and
//! [`super::update_proposed_move_path`]. The order
//! menu is a page element and keeps its own keyboard, so a menu that is open
//! holds the cursor and none of this runs.

use super::{
    BuildableSites, PendingAttackConfirmation, PlaySelectionState, PlayUnitSelectionParams,
    PointerPolicy, ProductionOptionsParams, TileAnswer, clear_selection_state,
    close_production_options, handle_tap, select_unit, selectable_unit_at, turn_readiness,
    unit_is_selectable,
};
use crate::core::coords::{TILE_SIZE, position_to_world_translation};
use crate::features::camera::FocusBoardOn;
use crate::features::event_bus::{EndTurnRequested, EventSink};
use crate::features::input::{BoardProjection, KeyboardCursor};
use awbrn_bevy::world::GameMap;
use awbrn_map::Pos;
use bevy::ecs::system::SystemParam;
use bevy::input::{ButtonState, keyboard::KeyboardInput};
use bevy::prelude::*;

/// How close to the edge of the view the cursor may come before the board
/// slides to keep up, in tiles.
///
/// The board follows rather than scrolls: a cursor walked off the edge is a
/// cursor the player cannot see, and a board that re-centres on every step is
/// one they cannot read.
const CURSOR_KEEP_IN_VIEW_TILES: f32 = 2.0;

/// What a key asks the board to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyIntent {
    /// Move the cursor by this many tiles right and down.
    Step(i16, i16),
    /// The next unit that can still act, or the one before it.
    Cycle(Cycle),
    /// Answer the tile the cursor is on.
    Confirm,
    /// Ask the page whether the turn should end.
    EndTurn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cycle {
    Next,
    Previous,
}

/// Which of the board's keys this is, if it is one of them.
///
/// Shift is a modifier the route drawing already owns, so it changes nothing
/// here — except on Tab, which the page needs to keep: Shift+Tab is the way out
/// of the board for anyone moving through the page by keyboard, and `Q` cycles
/// backwards for anyone who wanted the other thing.
///
/// Control is only read on Enter, and it makes the one key that answers a tile
/// into the one that asks about the turn. A chord, rather than a letter beside
/// the movement keys, because it is the only question here worth a two-handed
/// press.
fn key_intent(key: KeyCode, shift: bool, control: bool) -> Option<KeyIntent> {
    Some(match key {
        KeyCode::Enter | KeyCode::NumpadEnter if control => KeyIntent::EndTurn,
        KeyCode::ArrowUp | KeyCode::KeyW | KeyCode::Numpad8 => KeyIntent::Step(0, -1),
        KeyCode::ArrowDown | KeyCode::KeyS | KeyCode::Numpad2 => KeyIntent::Step(0, 1),
        KeyCode::ArrowLeft | KeyCode::KeyA | KeyCode::Numpad4 => KeyIntent::Step(-1, 0),
        KeyCode::ArrowRight | KeyCode::KeyD | KeyCode::Numpad6 => KeyIntent::Step(1, 0),
        KeyCode::Tab if !shift => KeyIntent::Cycle(Cycle::Next),
        KeyCode::KeyE => KeyIntent::Cycle(Cycle::Next),
        KeyCode::KeyQ => KeyIntent::Cycle(Cycle::Previous),
        KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => KeyIntent::Confirm,
        _ => return None,
    })
}

/// Whether the board holds this tile at all.
fn on_board(tile: Pos, game_map: &GameMap) -> bool {
    tile.x < game_map.width() && tile.y < game_map.height()
}

/// Where the cursor is, for a player who has not moved it yet.
///
/// The keyboard takes over from wherever the board was already pointing: the
/// mouse cursor, then the unit in hand, then the middle of the board. Starting
/// at the origin instead would send the first arrow key to a corner.
fn steering_tile(
    cursor: &KeyboardCursor,
    projection: &BoardProjection<'_, '_>,
    selection: &PlaySelectionState<'_>,
    game_map: &GameMap,
) -> Option<Pos> {
    cursor
        .0
        .or_else(|| projection.pointer_tile())
        .or_else(|| selection.selected.0.map(|selected| selected.origin))
        .or_else(|| {
            (game_map.width() > 0 && game_map.height() > 0)
                .then(|| Pos::new(game_map.width() / 2, game_map.height() / 2))
        })
        .filter(|tile| on_board(*tile, game_map))
}

/// The tile a walk is ordered by.
fn reading_order(position: &Pos) -> (u8, u8) {
    (position.y, position.x)
}

/// Everything this player may still act on: every unit first, in reading order,
/// and then every base, in reading order.
///
/// Building is the last thing a turn does — what the funds are best spent on is
/// what the rest of the board turned out to need — so the bases come after the
/// units rather than interleaved with them.
///
/// The number of units is returned with the list: it is the boundary between
/// the two halves, and the walk needs to know which side it landed on.
fn actionable_positions(
    unit_selection: &PlayUnitSelectionParams<'_, '_>,
    sites: &[Pos],
) -> (Vec<Pos>, usize) {
    let mut positions: Vec<Pos> = unit_selection
        .units
        .iter()
        .filter(|(_, faction, _, _, is_active, is_carried, has_cargo)| {
            unit_is_selectable(
                **faction,
                *is_active,
                *is_carried,
                *has_cargo,
                &unit_selection.friendly_factions,
            )
        })
        .map(|(_, _, map_position, _, _, _, _)| map_position.position())
        .collect();
    positions.sort_unstable_by_key(reading_order);
    positions.dedup();
    let unit_count = positions.len();
    positions.extend_from_slice(sites);
    (positions, unit_count)
}

/// Walk the units from where the cursor stands, and pick up the first one that
/// has an order to give.
///
/// A unit that can neither move, unload, nor be deleted is one the cycle would
/// stop on and answer nothing for, which is why the walk asks the rules rather
/// than trusting the cheap test that ordered the list.
fn cycle_units(
    direction: Cycle,
    cursor: &mut KeyboardCursor,
    sites: &BuildableSites,
    unit_selection: &PlayUnitSelectionParams<'_, '_>,
    production_options: &ProductionOptionsParams<'_>,
    selection: &mut PlaySelectionState<'_>,
    from: Option<Pos>,
) {
    let (positions, unit_count) = actionable_positions(unit_selection, &sites.tiles);
    if positions.is_empty() {
        return;
    }

    let count = positions.len();
    // A walk already under way carries on from the stop it is standing on. A
    // base holds the cursor and nothing else, so the cursor is the only record
    // that the walk was ever there; a cursor resting anywhere else means the
    // walk starts at the end it is walking from.
    let standing_at = from.and_then(|tile| positions.iter().position(|stop| *stop == tile));
    let first = match (standing_at, direction) {
        (Some(index), Cycle::Next) => (index + 1) % count,
        (Some(index), Cycle::Previous) => (index + count - 1) % count,
        (None, Cycle::Next) => 0,
        (None, Cycle::Previous) => count - 1,
    };

    for offset in 0..count {
        let index = match direction {
            Cycle::Next => (first + offset) % count,
            Cycle::Previous => (first + count - offset) % count,
        };
        let position = positions[index];
        if index < unit_count {
            if let Some(selectable) = selectable_unit_at(position, unit_selection) {
                let origin = selectable.origin;
                close_production_options(production_options.sink.as_deref());
                select_unit(selectable, unit_selection, selection);
                cursor.0 = Some(origin);
                return;
            }
            continue;
        }
        // The unit in hand goes: with a base under the cursor, Enter must read
        // as opening the build order, not as sending that unit there.
        close_production_options(production_options.sink.as_deref());
        clear_selection_state(selection);
        cursor.0 = Some(position);
        return;
    }
}

/// Everything a key needs to name a tile: where the cursors are, and what the
/// units on the board would answer.
#[derive(SystemParam)]
pub(crate) struct KeyboardBoard<'w, 's> {
    projection: BoardProjection<'w, 's>,
    cursor: ResMut<'w, KeyboardCursor>,
    sites: Res<'w, BuildableSites>,
    end_turn: Option<Res<'w, EventSink<EndTurnRequested>>>,
    unit_selection: PlayUnitSelectionParams<'w, 's>,
    production_options: ProductionOptionsParams<'w>,
}

/// Read the board's keys and act on them.
pub(crate) fn handle_play_keyboard(
    mut keyboard: MessageReader<KeyboardInput>,
    keys: Res<ButtonInput<KeyCode>>,
    mut board: KeyboardBoard<'_, '_>,
    mut policy: PointerPolicy<'_>,
    mut confirmation: ResMut<PendingAttackConfirmation>,
    mut selection: PlaySelectionState<'_>,
) {
    let KeyboardBoard {
        projection,
        cursor,
        sites,
        end_turn,
        unit_selection,
        production_options,
    } = &mut board;
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let control = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);

    for event in keyboard.read() {
        if event.state != ButtonState::Pressed {
            continue;
        }
        let Some(intent) = key_intent(event.key_code, shift, control) else {
            continue;
        };
        // A held key walks the cursor, because that is one answer being
        // adjusted. It never gives an order twice.
        if event.repeat && !matches!(intent, KeyIntent::Step(..)) {
            continue;
        }

        let standing_on = steering_tile(cursor, projection, &selection, &unit_selection.game_map);

        match intent {
            KeyIntent::Step(dx, dy) => {
                let Some(standing_on) = standing_on else {
                    continue;
                };
                let next = standing_on
                    .offset(dx, dy)
                    .filter(|tile| on_board(*tile, &unit_selection.game_map))
                    .unwrap_or(standing_on);
                cursor.set_if_neq(KeyboardCursor(Some(next)));
            }
            KeyIntent::Cycle(direction) => {
                // The walk continues from the unit in hand, or from the base
                // the last press of this key left the cursor on. A cursor
                // resting anywhere else means the first stop on the board.
                let from = selection
                    .selected
                    .0
                    .map(|selected| selected.origin)
                    .or(cursor.0);
                cycle_units(
                    direction,
                    cursor,
                    sites,
                    unit_selection,
                    production_options,
                    &mut selection,
                    from,
                );
            }
            KeyIntent::EndTurn => {
                if let Some(sink) = end_turn.as_deref() {
                    let readiness = turn_readiness(unit_selection, sites);
                    sink.emit(EndTurnRequested {
                        idle_units: readiness.idle_units,
                        free_sites: readiness.free_sites,
                    });
                }
            }
            KeyIntent::Confirm => {
                let Some(standing_on) = standing_on else {
                    continue;
                };
                cursor.0 = Some(standing_on);
                handle_tap(
                    TileAnswer {
                        tile: standing_on,
                        world: None,
                        coarse: false,
                    },
                    projection,
                    &mut confirmation,
                    &mut policy,
                    unit_selection,
                    production_options,
                    &mut selection,
                );
            }
        }
    }
}

/// Keep the tile the keyboard is on inside the view.
pub(crate) fn follow_keyboard_cursor(
    cursor: Res<KeyboardCursor>,
    game_map: Res<GameMap>,
    windows: Query<&Window>,
    cameras: Query<(&Projection, &GlobalTransform), With<Camera>>,
    mut focus: MessageWriter<FocusBoardOn>,
) {
    if !cursor.is_changed() {
        return;
    }
    let Some(tile) = cursor.0 else {
        return;
    };
    let (Ok(window), Ok((projection, camera_transform))) = (windows.single(), cameras.single())
    else {
        return;
    };
    let Projection::Orthographic(orthographic) = projection else {
        return;
    };

    let target =
        position_to_world_translation(&super::MOVE_RANGE_SPRITE_SIZE, tile, game_map.as_ref())
            .truncate();
    let half_view =
        Vec2::new(window.width(), window.height()) * 0.5 * orthographic.scale.max(f32::EPSILON);
    let margin = Vec2::splat(TILE_SIZE * CURSOR_KEEP_IN_VIEW_TILES);
    let comfortable = (half_view - margin).max(Vec2::ZERO);

    let offset = (target - camera_transform.translation().truncate()).abs();
    if offset.x > comfortable.x || offset.y > comfortable.y {
        focus.write(FocusBoardOn { world: target });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use awbrn_bevy::world::{BoardIndex, FriendlyFactions, UnitActive};
    use awbrn_map::Dimensions;
    use awbrn_types::PlayerFaction;
    use bevy::ecs::system::RunSystemOnce;

    /// The walk offers every unit before it offers the first base.
    ///
    /// Money is best spent on what the rest of the turn turned out to need, so
    /// a cycle that mixed the two would ask for the purchase halfway through
    /// the information that decides it.
    #[test]
    fn every_unit_comes_before_the_first_base() {
        let mut world = World::new();
        world.insert_resource(BoardIndex::new(Dimensions::new(4, 4)));
        world.init_resource::<GameMap>();
        let mut friendly = FriendlyFactions::default();
        friendly.0.insert(PlayerFaction::OrangeStar);
        world.insert_resource(friendly);
        for position in [Pos::new(3, 3), Pos::new(1, 0)] {
            world.spawn((
                awbrn_bevy::MapPosition::from(position),
                awbrn_bevy::world::Unit(awbrn_types::Unit::Infantry),
                awbrn_bevy::world::Faction(PlayerFaction::OrangeStar),
                UnitActive,
            ));
        }

        let sites = vec![Pos::new(0, 0), Pos::new(2, 1)];
        let (positions, unit_count) = world
            .run_system_once_with(
                |In(sites): In<Vec<Pos>>, unit_selection: PlayUnitSelectionParams<'_, '_>| {
                    actionable_positions(&unit_selection, &sites)
                },
                sites,
            )
            .unwrap();

        assert_eq!(unit_count, 2);
        assert_eq!(
            positions,
            vec![
                Pos::new(1, 0),
                Pos::new(3, 3),
                Pos::new(0, 0),
                Pos::new(2, 1),
            ],
            "units in reading order, then bases in reading order"
        );
    }
}
