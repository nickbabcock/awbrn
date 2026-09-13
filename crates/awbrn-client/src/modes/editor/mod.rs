//! The map editor: a board that is drawn on rather than played.
//!
//! The rules of an edit live in [`awbrn_map::editor`]. This mode is what puts
//! them on screen: it holds the open map, turns a pointer into a stroke, and
//! gives the board the tiles an edit changed. Nothing here decides what a road
//! looks like or where a mirror lands.
//!
//! The board is the same board every other mode draws. Terrain tiles, units,
//! the camera and the tile cursor are the ones the client already has, so a map
//! is edited against exactly the picture it will be played on.

use crate::core::coords::position_to_world_translation;
use crate::core::{AppState, GameMode, LoadingState};
use crate::features::event_bus::EventSink;
use crate::features::input::{
    BoardCursor, DragOwner, PointerGesture, PointerGestureKind, PointerSet, TILE_CORE_SPRITE_SIZE,
};
use crate::render::UiAtlas;
use awbrn_bevy::MapPosition;
use awbrn_bevy::world::{
    BoardIndex, BoardOf, Faction, GameMap, GraphicalHp, TerrainTile, Unit, UnitActive, board_root,
    initialize_terrain_semantic_world,
};
use awbrn_map::editor::{ArmyRoster, BoardChanges, Brush, MapEditor, ResizeAnchor, Symmetry};
use awbrn_map::rules::{DEFAULT_INCOME_PER_PROPERTY, pays_income};
use awbrn_map::{AwbrnMapMetadata, AwbwMap, Dimensions, Pos};
use awbrn_types::{AwbwTerrain, FactionCode, PlayerFaction, Property, VisualHp};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// The map that is open, and the brush that is loaded.
#[derive(Debug, Resource)]
pub struct EditorSession {
    editor: MapEditor,
    brush: Brush,
    /// Counts the edits, so the browser can tell one report from the next.
    revision: u32,
}

impl EditorSession {
    fn new(map: AwbwMap) -> EditorSession {
        EditorSession {
            editor: MapEditor::open(map),
            brush: Brush::Terrain {
                terrain: AwbwTerrain::Plain,
            },
            revision: 0,
        }
    }

    /// The map as it now stands, for saving.
    pub fn document(&self, metadata: AwbrnMapMetadata) -> awbrn_map::AwbrnMapDocument {
        self.editor.document(metadata)
    }
}

/// What the browser asks the editor to do.
///
/// A stroke is normally made with the pointer over the board, which the mode
/// turns into these same commands. The browser sends them directly for the
/// things a pointer cannot say: which brush is loaded, how the board is
/// folded, and what shape it is.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum EditorCommand {
    /// Load a brush. It stays loaded until another one replaces it.
    SetBrush {
        brush: Brush,
    },
    /// Change how an edit is repeated. A mode the board refuses is not taken.
    SetSymmetry {
        symmetry: Symmetry,
    },
    /// Change which armies the images of an edit are shared out to.
    SetRoster {
        #[cfg_attr(target_family = "wasm", tsify(type = "string[]"))]
        factions: Vec<FactionCode>,
    },
    /// Open a stroke, so that everything painted until it ends is one undo
    /// step. A drag across twenty tiles is one act to whoever drew it.
    BeginStroke,
    /// Close the stroke in flight. A stroke that changed nothing records
    /// nothing.
    EndStroke,
    /// Load the brush that would draw what is already on one tile.
    ///
    /// `groundOnly` asks for the tile under a unit rather than the unit, which
    /// is the only way to reach ground somebody is standing on.
    // The outer rename reaches the variants, not the fields inside one, and
    // this is the only variant here with a field of two words.
    #[serde(rename_all = "camelCase")]
    PickBrush {
        #[cfg_attr(target_family = "wasm", tsify(type = "{ x: number; y: number }"))]
        #[serde(with = "awbrn_map::xy")]
        position: Pos,
        #[serde(default)]
        ground_only: bool,
    },
    /// Put the loaded brush on one tile, without the pointer.
    Paint {
        #[cfg_attr(target_family = "wasm", tsify(type = "{ x: number; y: number }"))]
        #[serde(with = "awbrn_map::xy")]
        position: Pos,
    },
    Undo,
    Redo,
    /// Cover the whole board with one terrain, and take every unit off it.
    /// The terrain is named by its AWBW terrain id.
    Fill {
        #[cfg_attr(target_family = "wasm", tsify(type = "number"))]
        terrain: AwbwTerrain,
    },
    /// Change the shape of the board.
    Resize {
        width: u8,
        height: u8,
        anchor: ResizeAnchor,
    },
}

/// The commands that arrived since the last frame.
#[derive(Debug, Resource, Default)]
pub struct EditorCommandQueue(VecDeque<EditorCommand>);

impl EditorCommandQueue {
    pub fn push(&mut self, command: EditorCommand) {
        self.0.push_back(command);
    }
}

/// One army on the board, and what it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[serde(rename_all = "camelCase")]
pub struct EditorArmy {
    #[cfg_attr(target_family = "wasm", tsify(type = "string"))]
    pub faction: FactionCode,
    /// How many headquarters the army holds. A playable army holds one.
    pub headquarters: u32,
    /// Every building the army holds, headquarters included.
    pub properties: u32,
    /// How many bases, airports and ports the army can build from.
    pub production: u32,
    pub units: u32,
}

/// One tile, named the way the board names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[serde(rename_all = "camelCase")]
pub struct EditorTile {
    pub x: u8,
    pub y: u8,
}

/// What the board holds, as the editor reads it.
///
/// This is the map maker's readout. It is derived after every edit rather than
/// asked for, because the answer to "is this map fair yet" is the one thing a
/// symmetry mode cannot promise on its own: a mirror keeps the ground equal,
/// and says nothing about a headquarters that was never placed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[serde(rename_all = "camelCase")]
pub struct EditorStateChanged {
    pub revision: u32,
    pub width: u8,
    pub height: u8,
    /// The armies that hold anything, in the order the game lists them.
    pub armies: Vec<EditorArmy>,
    pub neutral_properties: u32,
    pub units: u32,
    pub symmetry: Symmetry,
    /// The modes this board can be folded with. A board that is not square
    /// refuses the quarter turn and both diagonals.
    pub available_symmetries: Vec<Symmetry>,
    /// Whether the ground as it stands already reads the same under its mode.
    pub symmetric: bool,
    /// How many tiles do not read the same under the mode in use.
    pub uneven_tiles: u32,
    /// The first of those tiles in board order. A count with no coordinate is
    /// a fault report a map maker cannot act on.
    pub first_uneven_tile: Option<EditorTile>,
    /// Funds per turn every income-paying building on the board is worth,
    /// whoever holds it. A com tower and a lab are held like any other
    /// building and pay nothing, so this is not the building count.
    pub board_income: u32,
    /// That figure shared evenly between the armies the fold seats. It is what
    /// a half of this map is worth when the halves are equal, which is the
    /// number a map maker is drawing towards.
    pub income_per_army: u32,
    pub can_undo: bool,
    pub can_redo: bool,
    pub brush: Brush,
    #[cfg_attr(target_family = "wasm", tsify(type = "string[]"))]
    pub roster: Vec<FactionCode>,
}

/// The board a stroke is being drawn on, while a stroke is in flight.
#[derive(Debug, Resource, Default)]
pub(crate) struct StrokeInFlight(bool);

/// Drops what the last edit left behind when the editor closes.
///
/// The commands run only in editor mode, so a command that arrives as the mode
/// closes would otherwise wait in the queue and land on the next board. A
/// stroke that was open when the mode closed would do the same, and the next
/// drag would carry on from it.
pub(crate) fn cleanup_editor_input(
    mut queue: ResMut<EditorCommandQueue>,
    mut stroke: ResMut<StrokeInFlight>,
) {
    queue.0.clear();
    stroke.0 = false;
}

/// Puts the open map on the board once the assets are ready.
pub(crate) fn initialize_editor_world(world: &mut World) {
    let Some(pending) = world.remove_resource::<crate::loading::LoadedEditorMap>() else {
        return;
    };

    let session = EditorSession::new(pending.0);
    world
        .resource_mut::<GameMap>()
        .set(session.editor.drawn().clone());
    world.insert_resource(session);

    rebuild_board(world);
    report_state(world);
}

/// Rebuilds every tile and unit from the open map.
///
/// This is what a resize needs, and it is also how the board is first drawn.
/// Everything else changes tiles one at a time.
fn rebuild_board(world: &mut World) {
    let root = initialize_terrain_semantic_world(world);

    let units: Vec<(Pos, awbrn_types::Unit, PlayerFaction, VisualHp)> = {
        let session = world.resource::<EditorSession>();
        session
            .editor
            .map()
            .deployments()
            .iter()
            .map(|(position, deployment)| {
                (position, deployment.unit, deployment.faction, deployment.hp)
            })
            .collect()
    };

    for (position, unit, faction, hp) in units {
        spawn_editor_unit(world, root, position, unit, faction, hp);
    }

    // The backdrop is sized from the board, so a board of a new shape needs a
    // new one. Taking it away is what asks for it to be drawn again.
    let backdrops: Vec<Entity> = world
        .query_filtered::<Entity, With<crate::render::map::MapBackdrop>>()
        .iter(world)
        .collect();
    for backdrop in backdrops {
        world.entity_mut(backdrop).despawn();
    }
}

fn spawn_editor_unit(
    world: &mut World,
    root: Entity,
    position: Pos,
    unit: awbrn_types::Unit,
    faction: PlayerFaction,
    hp: VisualHp,
) {
    world.spawn((
        MapPosition::from(position),
        Unit(unit),
        Faction(faction),
        GraphicalHp::Visible(hp),
        // Nothing on an edited board has spent its turn, so every unit is drawn
        // at full strength rather than greyed out.
        UnitActive,
        BoardOf(root),
    ));
}

/// Runs everything the browser and the pointer asked for this frame.
///
/// It is one exclusive system because an edit can change the shape of the
/// board, and rebuilding the board is not something a queue of deferred
/// commands can be trusted to interleave with a stroke.
pub(crate) fn run_editor_commands(world: &mut World) {
    let commands: Vec<EditorCommand> = {
        let mut queue = world.resource_mut::<EditorCommandQueue>();
        queue.0.drain(..).collect()
    };
    if commands.is_empty() {
        return;
    }

    for command in commands {
        let changes = apply_command(world, command);
        if !changes.is_empty() {
            apply_changes(world, &changes);
        }
    }

    // Every command reports, whether or not it moved a tile: loading a brush
    // and setting a fold both change what the panels have to say.
    world.resource_mut::<EditorSession>().revision += 1;
    report_state(world);
}

fn apply_command(world: &mut World, command: EditorCommand) -> BoardChanges {
    let mut session = world.resource_mut::<EditorSession>();
    match command {
        EditorCommand::SetBrush { brush } => {
            session.brush = brush;
            BoardChanges::default()
        }
        EditorCommand::SetSymmetry { symmetry } => {
            session.editor.set_symmetry(symmetry);
            BoardChanges::default()
        }
        EditorCommand::SetRoster { factions } => {
            session.editor.set_roster(ArmyRoster::new(
                factions.into_iter().map(FactionCode::faction),
            ));
            BoardChanges::default()
        }
        EditorCommand::BeginStroke => {
            session.editor.begin_stroke();
            BoardChanges::default()
        }
        EditorCommand::EndStroke => {
            session.editor.end_stroke();
            BoardChanges::default()
        }
        EditorCommand::PickBrush {
            position,
            ground_only,
        } => {
            // A pick off an empty tile is not an error; it is a pointer that
            // ended up past the edge of the board, and the loaded brush stays.
            if let Some(brush) = session.editor.brush_at(position, ground_only) {
                session.brush = brush;
            }
            BoardChanges::default()
        }
        EditorCommand::Paint { position } => {
            let brush = session.brush;
            session.editor.paint(position, brush)
        }
        EditorCommand::Undo => session.editor.undo(),
        EditorCommand::Redo => session.editor.redo(),
        EditorCommand::Fill { terrain } => session.editor.fill(terrain),
        EditorCommand::Resize {
            width,
            height,
            anchor,
        } => {
            if width == 0 || height == 0 {
                BoardChanges::default()
            } else {
                session
                    .editor
                    .resize(Dimensions::new(width, height), anchor)
            }
        }
    }
}

/// Gives the board the tiles an edit changed, and nothing else.
fn apply_changes(world: &mut World, changes: &BoardChanges) {
    if changes.dimensions.is_some() {
        let drawn = world.resource::<EditorSession>().editor.drawn().clone();
        world.resource_mut::<GameMap>().set(drawn);
        rebuild_board(world);
        return;
    }

    for change in &changes.terrain {
        world
            .resource_mut::<GameMap>()
            .set_terrain(change.position, change.terrain);
        let Ok(entity) = world
            .resource::<BoardIndex>()
            .terrain_entity(change.position)
        else {
            continue;
        };
        // The component is immutable, so the tile takes a new one rather than
        // being written through.
        world.entity_mut(entity).insert(TerrainTile {
            terrain: change.terrain,
        });
    }

    if changes.units.is_empty() {
        return;
    }

    let Some(root) = board_root(world) else {
        return;
    };
    for change in &changes.units {
        if let Ok(Some(held)) = world.resource::<BoardIndex>().unit_entity(change.position) {
            world.entity_mut(held).despawn();
        }
        let Some(deployment) = change.deployment else {
            continue;
        };
        spawn_editor_unit(
            world,
            root,
            change.position,
            deployment.unit,
            deployment.faction,
            VisualHp::new(deployment.hp),
        );
    }
}

/// Tells the browser what the board now holds.
fn report_state(world: &mut World) {
    let Some(sink) = world.get_resource::<EventSink<EditorStateChanged>>() else {
        return;
    };
    let session = world.resource::<EditorSession>();
    sink.emit(read_state(session));
}

/// Reads the board, army by army.
fn read_state(session: &EditorSession) -> EditorStateChanged {
    let editor = &session.editor;
    let map = editor.map();
    let dimensions = map.dimensions();

    let mut armies: Vec<EditorArmy> = Vec::new();
    let mut neutral_properties = 0;
    fn seat_of(armies: &mut Vec<EditorArmy>, faction: PlayerFaction) -> usize {
        let faction = FactionCode::from(faction);
        match armies.iter().position(|held| held.faction == faction) {
            Some(seat) => seat,
            None => {
                armies.push(EditorArmy {
                    faction,
                    headquarters: 0,
                    properties: 0,
                    production: 0,
                    units: 0,
                });
                armies.len() - 1
            }
        }
    }

    // What the board is worth, whoever ends up holding it. Which buildings pay
    // is the ruleset's answer rather than a list kept here — a com tower and a
    // lab are held like any other building and pay nothing — and the rate is
    // the one a real match is set up with.
    let mut board_income: u32 = 0;

    for (_, terrain) in map.iter() {
        let AwbwTerrain::Property(property) = terrain else {
            continue;
        };
        if pays_income(terrain) {
            board_income = board_income
                .saturating_add(u32::try_from(DEFAULT_INCOME_PER_PROPERTY).unwrap_or(u32::MAX));
        }
        let awbrn_types::Faction::Player(faction) = property.faction() else {
            neutral_properties += 1;
            continue;
        };
        let seat = seat_of(&mut armies, faction);
        armies[seat].properties += 1;
        if matches!(property, Property::HQ(_)) {
            armies[seat].headquarters += 1;
        }
        if matches!(
            property,
            Property::Base(_) | Property::Airport(_) | Property::Port(_)
        ) {
            armies[seat].production += 1;
        }
    }

    let mut units = 0;
    for (_, deployment) in map.deployments().iter() {
        units += 1;
        let seat = seat_of(&mut armies, deployment.faction);
        armies[seat].units += 1;
    }

    armies.sort_by_key(|held| held.faction.faction());

    let uneven = editor.asymmetries(editor.symmetry());
    // Divided between the armies the fold seats rather than the armies that
    // happen to hold something: a board drawn for four is drawn towards a
    // quarter each, whether or not the fourth has been given anything yet.
    let seats = editor.roster().seats().len();

    EditorStateChanged {
        revision: session.revision,
        width: dimensions.width(),
        height: dimensions.height(),
        armies,
        neutral_properties,
        units,
        symmetry: editor.symmetry(),
        available_symmetries: Symmetry::ALL
            .into_iter()
            .filter(|symmetry| symmetry.fits(dimensions))
            .collect(),
        symmetric: uneven.is_empty(),
        uneven_tiles: u32::try_from(uneven.len()).unwrap_or(u32::MAX),
        first_uneven_tile: uneven.first().map(|position| EditorTile {
            x: position.x,
            y: position.y,
        }),
        board_income,
        income_per_army: board_income / u32::try_from(seats).unwrap_or(1).max(1),
        can_undo: editor.can_undo(),
        can_redo: editor.can_redo(),
        brush: session.brush,
        roster: editor
            .roster()
            .seats()
            .iter()
            .copied()
            .map(FactionCode::from)
            .collect(),
    }
}

/// One of the cursors drawn where a stroke would also land.
///
/// A fold means a stroke is never one tile, and until this was drawn the only
/// way to find out where the other tiles were was to make the stroke and look.
/// The images are the answer to "where is this going to land", given before it
/// lands rather than in a sentence in the settings column.
#[derive(Component, Debug)]
pub struct FoldCursor;

/// How many images a fold can add beside the tile under the pointer.
///
/// The widest fold is the quarter turn, which takes four tiles: the one the
/// pointer is on and three more. The cursors are spawned once and moved,
/// because a cursor that is spawned and despawned as the pointer travels is a
/// cursor that flickers.
const FOLD_CURSOR_COUNT: usize = 3;

/// How much of the cursor an image keeps.
///
/// The same orange the board's own cursor wears, at half strength: an image is
/// a consequence of where the pointer is, and it must never be mistaken for
/// where the pointer is.
const FOLD_CURSOR_ALPHA: f32 = 0.5;

pub(crate) fn spawn_fold_cursors(mut commands: Commands, ui_atlas: UiAtlas) {
    for _ in 0..FOLD_CURSOR_COUNT {
        let mut sprite = ui_atlas.sprite_for("Effects/TileCursor.png");
        sprite.color = Color::srgba(1.0, 1.0, 1.0, FOLD_CURSOR_ALPHA);
        commands.spawn((
            sprite,
            // Under the cursor the pointer is on, so that the two never argue
            // about which one is in front where a board is small enough for
            // them to meet.
            Transform::from_translation(Vec3::new(
                0.0,
                0.0,
                TILE_CORE_SPRITE_SIZE.z_index as f32 - 0.5,
            )),
            Visibility::Hidden,
            FoldCursor,
        ));
    }
}

/// Where else the stroke under the pointer would land.
///
/// The editor works the images out, because it is the one that would paint
/// them; the screen only asks. Nothing is drawn while the fold is free, and
/// nothing is drawn while the eyedropper is held, because a pick lands on the
/// one tile it is pointing at.
fn fold_images(
    cursor: &BoardCursor<'_, '_>,
    keys: &ButtonInput<KeyCode>,
    session: &EditorSession,
) -> Vec<Pos> {
    if session.editor.symmetry() == Symmetry::None || eyedropper(keys).is_some() {
        return Vec::new();
    }

    let Some(tile) = cursor.tile() else {
        return Vec::new();
    };

    let mut images = session.editor.images(tile);
    images.retain(|image| *image != tile);
    images
}

pub(crate) fn update_fold_cursors(
    cursor: BoardCursor<'_, '_>,
    keys: Res<ButtonInput<KeyCode>>,
    game_map: Res<GameMap>,
    session: Res<EditorSession>,
    mut cursors: Query<(&mut Transform, &mut Visibility), With<FoldCursor>>,
) {
    let mut images = fold_images(&cursor, &keys, &session).into_iter();

    for (mut transform, mut visibility) in &mut cursors {
        let Some(image) = images.next() else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let center =
            position_to_world_translation(&TILE_CORE_SPRITE_SIZE, image, game_map.as_ref());
        transform.translation.x = center.x;
        transform.translation.y = center.y;
        *visibility = Visibility::Visible;
    }
}

/// Claims a drag for the brush, so that the camera does not pan under it.
///
/// Holding space gives the drag back to the camera, which is the gesture every
/// drawing tool uses and the only way to reach a board larger than the window
/// without putting the brush down.
pub(crate) fn claim_drag_for_brush(
    mut gestures: MessageReader<PointerGesture>,
    keys: Res<ButtonInput<KeyCode>>,
    mut owner: ResMut<DragOwner>,
) {
    for gesture in gestures.read() {
        match gesture.kind {
            PointerGestureKind::DragStart => {
                if !keys.pressed(KeyCode::Space) && gesture.tile.is_some() {
                    *owner = DragOwner::Brush;
                }
            }
            PointerGestureKind::DragEnd | PointerGestureKind::DragCancel => {
                *owner = DragOwner::Camera;
            }
            _ => {}
        }
    }
}

/// Whether the pointer is picking a brush up rather than putting one down.
///
/// Alt is the eyedropper everywhere a picture is drawn, and it is read here
/// the same way space is read for the pan: off the keyboard rather than off
/// the pointer, because the board is where both gestures are made. Shift with
/// it asks for the ground rather than the unit standing on it.
fn eyedropper(keys: &ButtonInput<KeyCode>) -> Option<bool> {
    if !keys.pressed(KeyCode::AltLeft) && !keys.pressed(KeyCode::AltRight) {
        return None;
    }
    Some(keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight))
}

/// Turns a gesture over the board into a stroke.
pub(crate) fn paint_from_pointer(
    mut gestures: MessageReader<PointerGesture>,
    keys: Res<ButtonInput<KeyCode>>,
    owner: Res<DragOwner>,
    mut queue: ResMut<EditorCommandQueue>,
    mut stroke: ResMut<StrokeInFlight>,
) {
    let picking = eyedropper(&keys);

    for gesture in gestures.read() {
        match gesture.kind {
            PointerGestureKind::Tap => {
                let Some(position) = gesture.tile else {
                    continue;
                };
                queue.push(match picking {
                    Some(ground_only) => EditorCommand::PickBrush {
                        position,
                        ground_only,
                    },
                    None => EditorCommand::Paint { position },
                });
            }
            PointerGestureKind::DragStart => {
                if *owner != DragOwner::Brush {
                    continue;
                }
                // A pick is one act on one tile, so a drag that starts with alt
                // held reads the tile it started on and opens no stroke. The
                // drag is still claimed, so the board does not slide under it.
                if let Some(ground_only) = picking {
                    if let Some(position) = gesture.tile {
                        queue.push(EditorCommand::PickBrush {
                            position,
                            ground_only,
                        });
                    }
                    continue;
                }
                queue.push(EditorCommand::BeginStroke);
                stroke.0 = true;
                if let Some(position) = gesture.tile {
                    queue.push(EditorCommand::Paint { position });
                }
            }
            PointerGestureKind::DragMove => {
                if !stroke.0 {
                    continue;
                }
                if let Some(position) = gesture.tile {
                    queue.push(EditorCommand::Paint { position });
                }
            }
            PointerGestureKind::DragEnd | PointerGestureKind::DragCancel => {
                if stroke.0 {
                    queue.push(EditorCommand::EndStroke);
                    stroke.0 = false;
                }
            }
        }
    }
}

/// Undo and redo from the keyboard, where a drawing tool puts them.
pub(crate) fn undo_from_keyboard(
    keys: Res<ButtonInput<KeyCode>>,
    mut queue: ResMut<EditorCommandQueue>,
) {
    let held = keys.pressed(KeyCode::ControlLeft)
        || keys.pressed(KeyCode::ControlRight)
        || keys.pressed(KeyCode::SuperLeft)
        || keys.pressed(KeyCode::SuperRight);
    if !held {
        return;
    }

    let shifted = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    if keys.just_pressed(KeyCode::KeyZ) {
        queue.push(if shifted {
            EditorCommand::Redo
        } else {
            EditorCommand::Undo
        });
    } else if keys.just_pressed(KeyCode::KeyY) {
        queue.push(EditorCommand::Redo);
    }
}

#[derive(Debug)]
pub struct EditorPlugin;

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EditorCommandQueue>()
            .init_resource::<StrokeInFlight>()
            .add_systems(
                Update,
                claim_drag_for_brush
                    .in_set(PointerSet::Claim)
                    .run_if(in_state(GameMode::Editor)),
            )
            .add_systems(
                Update,
                paint_from_pointer
                    .in_set(PointerSet::Consume)
                    .run_if(in_state(GameMode::Editor)),
            )
            .add_systems(
                OnEnter(LoadingState::Complete),
                spawn_fold_cursors
                    .after(crate::loading::setup_ui_atlas)
                    .run_if(in_state(GameMode::Editor)),
            )
            .add_systems(
                Update,
                update_fold_cursors
                    .run_if(in_state(AppState::InGame))
                    .run_if(in_state(GameMode::Editor))
                    .run_if(resource_exists::<EditorSession>),
            )
            .add_systems(
                Update,
                undo_from_keyboard
                    .run_if(in_state(AppState::InGame))
                    .run_if(in_state(GameMode::Editor)),
            )
            .add_systems(OnExit(GameMode::Editor), cleanup_editor_input)
            .add_systems(
                Update,
                run_editor_commands
                    .after(PointerSet::Consume)
                    .run_if(in_state(GameMode::Editor))
                    .run_if(resource_exists::<EditorSession>),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use awbrn_map::editor::Connection;
    use awbrn_types::{GraphicalTerrain, PropertyKind};

    fn session(width: u8, height: u8) -> EditorSession {
        EditorSession::new(AwbwMap::new(
            Dimensions::new(width, height),
            AwbwTerrain::Plain,
        ))
    }

    /// A world with the board a headless editor needs, and nothing else.
    ///
    /// No renderer, no assets and no window: everything below is the pipeline
    /// from a command to the tiles on the board, which is the part that has to
    /// hold whether or not anything is on screen.
    fn editor_world(width: u8, height: u8) -> App {
        let mut app = App::new();
        app.add_plugins(awbrn_bevy::GameWorldPlugin);
        app.init_resource::<EditorCommandQueue>();
        app.world_mut()
            .insert_resource(crate::loading::LoadedEditorMap(AwbwMap::new(
                Dimensions::new(width, height),
                AwbwTerrain::Plain,
            )));
        initialize_editor_world(app.world_mut());
        app
    }

    fn queue(app: &mut App, command: EditorCommand) {
        app.world_mut()
            .resource_mut::<EditorCommandQueue>()
            .push(command);
    }

    fn drawn_at(app: &mut App, position: Pos) -> awbrn_types::GraphicalTerrain {
        let entity = app
            .world()
            .resource::<awbrn_bevy::world::BoardIndex>()
            .terrain_entity(position)
            .expect("the board holds this tile");
        app.world()
            .entity(entity)
            .get::<TerrainTile>()
            .expect("a terrain tile carries its terrain")
            .terrain
    }

    #[test]
    fn opening_the_editor_puts_every_tile_on_the_board() {
        let mut app = editor_world(6, 4);

        for position in Dimensions::new(6, 4).positions() {
            assert_eq!(drawn_at(&mut app, position), GraphicalTerrain::Plain);
        }
        assert_eq!(app.world().resource::<GameMap>().width(), 6);
    }

    #[test]
    fn a_painted_tile_reaches_the_board_it_is_drawn_on() {
        let mut app = editor_world(9, 5);
        queue(
            &mut app,
            EditorCommand::SetSymmetry {
                symmetry: Symmetry::MirrorLeftRight,
            },
        );
        queue(
            &mut app,
            EditorCommand::SetBrush {
                brush: Brush::Terrain {
                    terrain: AwbwTerrain::Mountain,
                },
            },
        );
        queue(
            &mut app,
            EditorCommand::Paint {
                position: Pos::new(1, 2),
            },
        );
        run_editor_commands(app.world_mut());

        // Both the tile and the image of it are on the board, not only in the
        // map the editor holds.
        assert_eq!(
            drawn_at(&mut app, Pos::new(1, 2)),
            GraphicalTerrain::Mountain
        );
        assert_eq!(
            drawn_at(&mut app, Pos::new(7, 2)),
            GraphicalTerrain::Mountain
        );
        assert_eq!(drawn_at(&mut app, Pos::new(4, 2)), GraphicalTerrain::Plain);
    }

    #[test]
    fn a_painted_unit_arrives_on_the_board_and_leaves_again() {
        let mut app = editor_world(8, 4);
        queue(
            &mut app,
            EditorCommand::SetBrush {
                brush: Brush::Unit {
                    unit: awbrn_types::Unit::Infantry,
                    faction: FactionCode::new(PlayerFaction::OrangeStar),
                    hp: VisualHp::new(10),
                },
            },
        );
        queue(
            &mut app,
            EditorCommand::Paint {
                position: Pos::new(2, 1),
            },
        );
        run_editor_commands(app.world_mut());

        let standing = app
            .world()
            .resource::<awbrn_bevy::world::BoardIndex>()
            .unit_entity(Pos::new(2, 1))
            .expect("the tile is on the board");
        assert!(standing.is_some(), "a unit entity stands on the tile");

        queue(
            &mut app,
            EditorCommand::SetBrush {
                brush: Brush::EraseUnit,
            },
        );
        queue(
            &mut app,
            EditorCommand::Paint {
                position: Pos::new(2, 1),
            },
        );
        run_editor_commands(app.world_mut());

        let after = app
            .world()
            .resource::<awbrn_bevy::world::BoardIndex>()
            .unit_entity(Pos::new(2, 1))
            .expect("the tile is on the board");
        assert!(after.is_none(), "the unit left the board with the edit");
    }

    #[test]
    fn the_board_opens_with_a_brush_the_rail_can_show_as_loaded() {
        // The rail lights the key whose brush matches the one the board
        // reports. A board that opened with a brush no palette cell carries
        // would leave every key dark and the fill key out of reach, with
        // nothing on the screen saying why.
        let opening = session(5, 5).brush;
        let palette = awbrn_map::editor::terrain_palette(Some(PlayerFaction::OrangeStar));

        assert!(
            palette.iter().any(|entry| entry.brush == opening),
            "the opening brush {opening:?} is not on the rail",
        );
    }

    #[test]
    fn a_pick_loads_the_brush_that_drew_the_tile_under_the_pointer() {
        let mut app = editor_world(7, 7);
        queue(
            &mut app,
            EditorCommand::SetBrush {
                brush: Brush::Connecting {
                    connection: Connection::Road,
                },
            },
        );
        for x in 2..5u8 {
            queue(
                &mut app,
                EditorCommand::Paint {
                    position: Pos::new(x, 3),
                },
            );
        }
        // Something else is loaded, so the pick has to put the road back.
        queue(
            &mut app,
            EditorCommand::SetBrush {
                brush: Brush::Terrain {
                    terrain: AwbwTerrain::Sea,
                },
            },
        );
        run_editor_commands(app.world_mut());

        queue(
            &mut app,
            EditorCommand::PickBrush {
                position: Pos::new(3, 3),
                ground_only: false,
            },
        );
        run_editor_commands(app.world_mut());

        assert_eq!(
            app.world().resource::<EditorSession>().brush,
            Brush::Connecting {
                connection: Connection::Road
            }
        );
    }

    #[test]
    fn a_pick_past_the_edge_of_the_board_keeps_the_loaded_brush() {
        let mut app = editor_world(4, 4);
        queue(
            &mut app,
            EditorCommand::SetBrush {
                brush: Brush::Terrain {
                    terrain: AwbwTerrain::Mountain,
                },
            },
        );
        queue(
            &mut app,
            EditorCommand::PickBrush {
                position: Pos::new(40, 40),
                ground_only: false,
            },
        );
        run_editor_commands(app.world_mut());

        assert_eq!(
            app.world().resource::<EditorSession>().brush,
            Brush::Terrain {
                terrain: AwbwTerrain::Mountain
            }
        );
    }

    #[test]
    fn a_pick_changes_nothing_on_the_board_and_costs_no_undo_step() {
        let mut app = editor_world(5, 5);
        queue(
            &mut app,
            EditorCommand::PickBrush {
                position: Pos::new(2, 2),
                ground_only: false,
            },
        );
        run_editor_commands(app.world_mut());

        assert_eq!(drawn_at(&mut app, Pos::new(2, 2)), GraphicalTerrain::Plain);
        assert!(!app.world().resource::<EditorSession>().editor.can_undo());
    }

    #[test]
    fn a_stroke_undoes_as_one_step() {
        let mut app = editor_world(8, 4);
        queue(
            &mut app,
            EditorCommand::SetBrush {
                brush: Brush::Terrain {
                    terrain: AwbwTerrain::Mountain,
                },
            },
        );
        queue(&mut app, EditorCommand::BeginStroke);
        queue(
            &mut app,
            EditorCommand::Paint {
                position: Pos::new(1, 1),
            },
        );
        queue(
            &mut app,
            EditorCommand::Paint {
                position: Pos::new(2, 1),
            },
        );
        queue(&mut app, EditorCommand::EndStroke);
        run_editor_commands(app.world_mut());

        queue(&mut app, EditorCommand::Undo);
        run_editor_commands(app.world_mut());

        assert_eq!(drawn_at(&mut app, Pos::new(1, 1)), GraphicalTerrain::Plain);
        assert_eq!(drawn_at(&mut app, Pos::new(2, 1)), GraphicalTerrain::Plain);
        assert!(!app.world().resource::<EditorSession>().editor.can_undo());
    }

    #[test]
    fn resizing_rebuilds_the_board_at_the_new_shape() {
        let mut app = editor_world(6, 6);
        queue(
            &mut app,
            EditorCommand::Resize {
                width: 10,
                height: 8,
                anchor: ResizeAnchor::TopLeft,
            },
        );
        run_editor_commands(app.world_mut());

        assert_eq!(app.world().resource::<GameMap>().width(), 10);
        assert_eq!(app.world().resource::<GameMap>().height(), 8);
        assert_eq!(drawn_at(&mut app, Pos::new(9, 7)), GraphicalTerrain::Plain);
    }

    #[test]
    fn a_blank_board_holds_no_armies() {
        let state = read_state(&session(10, 6));

        assert!(state.armies.is_empty());
        assert_eq!(state.units, 0);
    }

    #[test]
    fn the_readout_counts_what_each_army_holds() {
        let mut session = session(10, 6);
        session.editor.set_symmetry(Symmetry::MirrorLeftRight);
        session.editor.paint(
            Pos::new(1, 1),
            Brush::Property {
                property: PropertyKind::HQ,
                faction: Some(FactionCode::new(PlayerFaction::OrangeStar)),
            },
        );
        session.editor.paint(
            Pos::new(1, 3),
            Brush::Property {
                property: PropertyKind::Base,
                faction: Some(FactionCode::new(PlayerFaction::OrangeStar)),
            },
        );

        let state = read_state(&session);

        assert_eq!(state.armies.len(), 2);
        for army in &state.armies {
            assert_eq!(army.headquarters, 1);
            assert_eq!(army.properties, 2);
            assert_eq!(army.production, 1);
        }
        assert!(state.symmetric);
    }

    /// The board's worth is money rather than a count of buildings: a com
    /// tower and a lab are held like any other building and pay nothing.
    #[test]
    fn the_readout_prices_the_whole_board() {
        let mut session = session(11, 6);
        session.editor.paint(
            Pos::new(1, 1),
            Brush::Property {
                property: PropertyKind::HQ,
                faction: Some(FactionCode::new(PlayerFaction::OrangeStar)),
            },
        );
        session.editor.paint(
            Pos::new(5, 1),
            Brush::Property {
                property: PropertyKind::City,
                faction: None,
            },
        );
        session.editor.paint(
            Pos::new(7, 1),
            Brush::Property {
                property: PropertyKind::ComTower,
                faction: None,
            },
        );

        let state = read_state(&session);

        assert_eq!(state.board_income, 2_000, "the com tower pays nothing");
        assert_eq!(state.income_per_army, 1_000, "shared between the two seats");
    }

    /// The share is taken over the seats the fold holds, not the armies that
    /// happen to hold something: a board drawn for four is drawn towards a
    /// quarter each before the fourth army has been given anything.
    #[test]
    fn the_share_follows_the_roster_rather_than_the_board() {
        let mut session = session(12, 12);
        session.editor.set_symmetry(Symmetry::QuadMirror);
        session.editor.paint(
            Pos::new(1, 1),
            Brush::Property {
                property: PropertyKind::City,
                faction: None,
            },
        );

        let state = read_state(&session);

        assert_eq!(state.armies.len(), 0, "a neutral city seats nobody");
        assert_eq!(state.board_income, 4_000, "the fold put down four cities");
        assert_eq!(state.income_per_army, 1_000);
    }

    /// A count with no coordinate is a fault report a map maker cannot act on,
    /// so the readout names the first tile that does not fold.
    #[test]
    fn an_uneven_board_names_its_first_tile() {
        let mut session = session(10, 6);
        session.editor.set_symmetry(Symmetry::MirrorLeftRight);
        session.editor.paint(
            Pos::new(2, 1),
            Brush::Terrain {
                terrain: AwbwTerrain::Mountain,
            },
        );
        session.editor.set_symmetry(Symmetry::None);
        session.editor.paint(
            Pos::new(7, 1),
            Brush::Terrain {
                terrain: AwbwTerrain::Wood,
            },
        );

        session.editor.set_symmetry(Symmetry::MirrorLeftRight);
        let state = read_state(&session);

        assert!(!state.symmetric);
        assert_eq!(state.uneven_tiles, 2);
        assert_eq!(state.first_uneven_tile, Some(EditorTile { x: 2, y: 1 }));
    }

    #[test]
    fn a_building_one_army_holds_alone_still_reads_as_symmetric() {
        // A map maker answers the first-turn advantage with buildings that are
        // meant to be uneven: a base the second army takes on turn one, a city
        // one side reaches first. The fold is a promise about the ground, so
        // it must not call those a broken board.
        let mut session = session(10, 6);
        session.editor.set_symmetry(Symmetry::MirrorLeftRight);
        session.editor.paint(
            Pos::new(1, 1),
            Brush::Property {
                property: PropertyKind::HQ,
                faction: Some(FactionCode::new(PlayerFaction::OrangeStar)),
            },
        );
        // Painted with the fold off, so it lands on one side only.
        session.editor.set_symmetry(Symmetry::None);
        session.editor.paint(
            Pos::new(2, 1),
            Brush::Property {
                property: PropertyKind::Base,
                faction: Some(FactionCode::new(PlayerFaction::BlueMoon)),
            },
        );
        session.editor.set_symmetry(Symmetry::MirrorLeftRight);

        let state = read_state(&session);

        assert!(
            state.symmetric,
            "a building placed on purpose is not a broken fold",
        );
    }

    #[test]
    fn ground_that_does_not_fold_still_reads_as_uneven() {
        let mut session = session(10, 6);
        session.editor.set_symmetry(Symmetry::None);
        session.editor.paint(
            Pos::new(2, 2),
            Brush::Terrain {
                terrain: AwbwTerrain::Mountain,
            },
        );
        session.editor.set_symmetry(Symmetry::MirrorLeftRight);

        assert!(!read_state(&session).symmetric);
    }

    #[test]
    fn a_board_edited_out_of_symmetry_says_so() {
        let mut session = session(10, 6);
        session.editor.set_symmetry(Symmetry::MirrorLeftRight);
        session.editor.paint(
            Pos::new(1, 1),
            Brush::Connecting {
                connection: Connection::Road,
            },
        );
        assert!(read_state(&session).symmetric);

        session.editor.set_symmetry(Symmetry::None);
        session.editor.paint(
            Pos::new(4, 4),
            Brush::Terrain {
                terrain: AwbwTerrain::Mountain,
            },
        );
        session.editor.set_symmetry(Symmetry::MirrorLeftRight);

        assert!(!read_state(&session).symmetric);
    }

    /// The cursors the fold draws, with the keyboard holding the tile so that
    /// no window and no camera are needed to say where the pointer is.
    fn fold_cursor_positions(app: &mut App, at: Pos, symmetry: Symmetry) -> Vec<Vec3> {
        app.world_mut()
            .resource_mut::<EditorSession>()
            .editor
            .set_symmetry(symmetry);
        app.world_mut()
            .insert_resource(crate::features::input::KeyboardCursor(Some(at)));
        app.world_mut().init_resource::<ButtonInput<KeyCode>>();

        for _ in 0..FOLD_CURSOR_COUNT {
            app.world_mut()
                .spawn((Transform::default(), Visibility::Hidden, FoldCursor));
        }

        app.add_systems(Update, update_fold_cursors);
        app.update();

        app.world_mut()
            .query::<(&Transform, &Visibility, &FoldCursor)>()
            .iter(app.world())
            .filter(|(_, visibility, _)| **visibility == Visibility::Visible)
            .map(|(transform, _, _)| transform.translation)
            .collect()
    }

    #[test]
    fn the_fold_draws_a_cursor_wherever_else_the_stroke_would_land() {
        let mut app = editor_world(5, 5);
        let drawn = fold_cursor_positions(&mut app, Pos::new(0, 1), Symmetry::Rotate180);

        let map = app.world().resource::<GameMap>();
        let far_corner = position_to_world_translation(&TILE_CORE_SPRITE_SIZE, Pos::new(4, 3), map);

        assert_eq!(drawn.len(), 1);
        assert_eq!(drawn[0].x, far_corner.x);
        assert_eq!(drawn[0].y, far_corner.y);
    }

    #[test]
    fn the_eyedropper_puts_the_images_away_because_a_pick_lands_on_one_tile() {
        let mut app = editor_world(5, 5);
        app.world_mut().init_resource::<ButtonInput<KeyCode>>();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::AltLeft);

        assert!(fold_cursor_positions(&mut app, Pos::new(0, 1), Symmetry::Rotate180).is_empty());
    }

    #[test]
    fn a_free_board_draws_no_cursor_but_the_one_under_the_pointer() {
        let mut app = editor_world(5, 5);

        assert!(fold_cursor_positions(&mut app, Pos::new(0, 1), Symmetry::None).is_empty());
    }

    #[test]
    fn a_tile_on_the_fold_draws_no_second_cursor_on_itself() {
        let mut app = editor_world(5, 5);

        assert!(
            fold_cursor_positions(&mut app, Pos::new(2, 3), Symmetry::MirrorLeftRight).is_empty()
        );
    }

    #[test]
    fn a_quarter_turn_draws_the_other_three_corners() {
        let mut app = editor_world(5, 5);
        let drawn = fold_cursor_positions(&mut app, Pos::new(0, 1), Symmetry::Rotate90);

        assert_eq!(drawn.len(), 3);
    }

    #[test]
    fn a_board_that_is_not_square_offers_fewer_modes() {
        let wide = read_state(&session(10, 6));
        assert!(!wide.available_symmetries.contains(&Symmetry::Rotate90));

        let square = read_state(&session(8, 8));
        assert!(square.available_symmetries.contains(&Symmetry::Rotate90));
    }
}
