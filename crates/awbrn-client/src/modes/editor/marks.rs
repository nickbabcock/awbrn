//! What the editor draws over the map: where a stroke would land, and where
//! the board falls short of a match.

use super::{EditorSession, EditorStateChanged, eyedropper};
use crate::core::coords::{TILE_SIZE, position_to_world_translation, tile_center_world_position};
use crate::core::{GameMode, RenderLayer, SpriteSize};
use crate::features::input::BoardCursor;
use crate::render::{TerrainAtlasResource, UnitAtlasResource};
use awbrn_bevy::world::GameMap;
use awbrn_map::editor::{Brush, MapEditor, sample_tile};
use awbrn_map::{Dimensions, Pos};
use awbrn_types::{AwbwTerrain, GraphicalMovement, GraphicalTerrain, PlayerFaction, Unit, Weather};
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

const INK_950: Color = Color::srgb_u8(0x0a, 0x11, 0x28);
const WARN_FILL: Color = Color::srgb_u8(0xf4, 0xb7, 0x3a);
const AIRMAIL_BLUE: Color = Color::srgb_u8(0x27, 0x47, 0xb0);

/// Ink 950 at a coverage the design states in sRGB. The board blends in linear
/// light, where a dark wash at the same alpha reads far lighter.
fn ink(alpha: f32) -> Color {
    INK_950.with_alpha(1.0 - (1.0 - alpha).powf(2.2))
}

/// White at a coverage the design states in sRGB (see [`ink`]).
fn pale(alpha: f32) -> Color {
    Color::WHITE.with_alpha(alpha.powf(2.2))
}

/// Texels per board pixel in the generated art, so a keyline can be finer
/// than the board's own pixels.
const ART_TEXELS: f32 = 8.0;

/// How far the focus ring reaches past its tile on each side, in board pixels.
const RING_REACH: f32 = 3.0;

/// The palette swatch of connecting terrain and sea is drawn at 5/8 size: its
/// joins come from its neighbors, so the full sprite would be a guess.
const SWATCH_SIZE: f32 = TILE_SIZE * 5.0 / 8.0;

// Problem frames sit under the units, the focus ring over them, and the
// landing ghosts under the pointer's own cursor.
const FRAME_Z: f32 = RenderLayer::UNIT as f32 - 0.5;
const RING_Z: f32 = RenderLayer::STRIKE_MARK as f32;
const GHOST_Z: f32 = RenderLayer::CURSOR as f32 - 0.5;

#[derive(Resource, Debug)]
pub(crate) struct MarkArt {
    frame: Handle<Image>,
    edge: Handle<Image>,
    ring: Handle<Image>,
}

pub(crate) fn create_mark_art(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    existing: Option<Res<MarkArt>>,
) {
    if existing.is_some() {
        return;
    }
    let white = Color::WHITE;
    commands.insert_resource(MarkArt {
        frame: images.add(banded_square(
            TILE_SIZE,
            0.0,
            &[(0.5, ink(0.9)), (2.0, WARN_FILL), (0.5, ink(0.9))],
        )),
        edge: images.add(banded_square(
            TILE_SIZE,
            0.0,
            &[(1.5, ink(0.85)), (0.5, pale(0.7))],
        )),
        ring: images.add(banded_square(
            TILE_SIZE + 2.0 * RING_REACH,
            2.0,
            &[(0.75, white), (2.0, AIRMAIL_BLUE), (0.75, white)],
        )),
    });
}

/// A square `size` board pixels wide, painted inward from its edge in
/// `bands` of (thickness, color), and clear inside the last band.
fn banded_square(size: f32, radius: f32, bands: &[(f32, Color)]) -> Image {
    // Board sizes here are whole pixels, so the texel count is exact.
    let texels = (size * ART_TEXELS) as u32;
    let half = Vec2::splat(size / 2.0);
    let mut data = Vec::with_capacity((texels * texels * 4) as usize);
    for y in 0..texels {
        for x in 0..texels {
            let point = (Vec2::new(x as f32, y as f32) + 0.5) / ART_TEXELS - half;
            let corner = point.abs() - (half - radius);
            let depth = radius - corner.max(Vec2::ZERO).length() - corner.max_element().min(0.0);
            let mut reached = 0.0;
            let color = bands.iter().find_map(|(thickness, color)| {
                reached += thickness;
                (depth >= 0.0 && depth < reached).then_some(*color)
            });
            data.extend_from_slice(&color.map_or([0; 4], |color| color.to_srgba().to_u8_array()));
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: texels,
            height: texels,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::linear();
    image
}

/// A mark the board redraws after every report: a problem frame or the
/// focus ring.
#[derive(Component, Debug)]
pub(crate) struct ProblemMark;

/// Every tile that takes a problem frame: the uneven tiles, and every
/// headquarters of an army that holds more than one.
pub(crate) fn problem_tiles(state: &EditorStateChanged) -> Vec<Pos> {
    let uneven = state.uneven_groups.iter().flatten();
    let extra_hqs = state
        .armies
        .iter()
        .filter(|army| army.hq_at.len() > 1)
        .flat_map(|army| army.hq_at.iter());
    uneven.chain(extra_hqs).map(|tile| tile.pos()).collect()
}

/// The marks last drawn, and on which board shape.
#[derive(Resource, Debug, Default, PartialEq)]
struct DrawnProblems(Option<(Vec<Pos>, Vec<Pos>, Dimensions)>);

pub(crate) fn redraw_problem_marks(world: &mut World, state: &EditorStateChanged) {
    let Some(art) = world.get_resource::<MarkArt>() else {
        return;
    };
    let (frame, ring) = (art.frame.clone(), art.ring.clone());
    let session = world.resource::<EditorSession>();
    // A resize can leave the focus past the board's new edge.
    let dimensions = session.editor.dimensions();
    let focus: Vec<Pos> = session
        .focus
        .iter()
        .copied()
        .filter(|tile| dimensions.contains(*tile))
        .collect();

    let tiles = problem_tiles(state);

    let drawn: Vec<Entity> = world
        .query_filtered::<Entity, With<ProblemMark>>()
        .iter(world)
        .collect();
    // Reports arrive every frame of a stroke; the same marks stay where they are.
    let next = DrawnProblems(Some((tiles.clone(), focus.clone(), dimensions)));
    if *world.get_resource_or_init::<DrawnProblems>() == next
        && drawn.len() == tiles.len() + focus.len()
    {
        return;
    }
    world.insert_resource(next);
    for entity in drawn {
        world.entity_mut(entity).despawn();
    }

    let marks: Vec<(Sprite, Vec3)> = {
        let game_map = world.resource::<GameMap>();
        let at = |tile: Pos, z: f32| tile_center(tile, game_map).extend(z);
        tiles
            .into_iter()
            .map(|tile| (art_sprite(&frame, TILE_SIZE), at(tile, FRAME_Z)))
            .chain(focus.into_iter().map(|tile| {
                (
                    art_sprite(&ring, TILE_SIZE + 2.0 * RING_REACH),
                    at(tile, RING_Z),
                )
            }))
            .collect()
    };
    for (sprite, translation) in marks {
        world.spawn((
            sprite,
            Transform::from_translation(translation),
            ProblemMark,
            DespawnOnExit(GameMode::Editor),
        ));
    }
}

/// What a landing tile shows of the brush, in the army of its image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum LandingArt {
    /// An eraser shows only where it lands.
    Bare,
    Tile(GraphicalTerrain),
    Swatch(GraphicalTerrain),
    Unit(Unit, PlayerFaction),
}

/// One tile a stroke on the pointer would paint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Landing {
    pub(crate) tile: Pos,
    /// Whether the fold, rather than the pointer, puts the stroke here.
    pub(crate) image: bool,
    pub(crate) art: LandingArt,
}

pub(crate) fn landings(editor: &MapEditor, brush: Brush, pointer: Pos) -> Vec<Landing> {
    editor
        .reach(pointer)
        .into_iter()
        .map(|(tile, step)| Landing {
            tile,
            image: tile != pointer,
            art: landing_art(brush.for_image(editor.roster(), editor.symmetry(), step)),
        })
        // The pointer's own cursor already says where an eraser lands.
        .filter(|landing| landing.image || landing.art != LandingArt::Bare)
        .collect()
}

fn landing_art(brush: Brush) -> LandingArt {
    match brush {
        Brush::Unit { unit, faction, .. } => LandingArt::Unit(unit, faction.faction()),
        Brush::Connecting { .. }
        | Brush::Terrain {
            terrain: AwbwTerrain::Sea,
        } => brush.sample().map_or(LandingArt::Bare, |sample| {
            LandingArt::Swatch(sample_tile(sample))
        }),
        brush => brush.sample().map_or(LandingArt::Bare, |sample| {
            LandingArt::Tile(sample_tile(sample))
        }),
    }
}

#[derive(Component, Debug)]
pub(crate) struct LandingMark;

/// Draws the brush on every tile the stroke under the pointer would paint.
///
/// Nothing is drawn while Alt is held: a pick reads one tile and paints none.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_landings(
    mut commands: Commands,
    cursor: BoardCursor<'_, '_>,
    keys: Res<ButtonInput<KeyCode>>,
    game_map: Res<GameMap>,
    session: Res<EditorSession>,
    art: Res<MarkArt>,
    terrain_atlas: Res<TerrainAtlasResource>,
    unit_atlas: Res<UnitAtlasResource>,
    drawn: Query<Entity, With<LandingMark>>,
    mut last: Local<Vec<Landing>>,
) {
    let landings = match cursor.tile() {
        Some(pointer) if eyedropper(&keys).is_none() => {
            landings(&session.editor, session.brush, pointer)
        }
        _ => Vec::new(),
    };
    // A resize moves every tile on screen, so the same tiles are drawn again.
    if *last == landings && !game_map.is_changed() {
        return;
    }

    for entity in &drawn {
        commands.entity(entity).despawn();
    }
    for landing in &landings {
        let center = tile_center(landing.tile, &game_map);
        let mut spawn = |sprite: Sprite, offset: Vec2, layer: f32| {
            commands.spawn((
                sprite,
                Transform::from_translation((center + offset).extend(GHOST_Z + layer)),
                LandingMark,
                DespawnOnExit(GameMode::Editor),
            ));
        };

        if landing.art != LandingArt::Bare {
            spawn(
                Sprite::from_color(ink(0.78), Vec2::splat(TILE_SIZE)),
                Vec2::ZERO,
                0.0,
            );
        }
        match landing.art {
            LandingArt::Bare => {}
            LandingArt::Tile(terrain) => {
                // A terrain cell is two tiles tall, and the top one overhangs
                // the tile above, as the board draws it.
                spawn(
                    terrain_sprite(&terrain_atlas, terrain),
                    Vec2::new(0.0, TILE_SIZE / 2.0),
                    0.2,
                );
            }
            LandingArt::Swatch(terrain) => {
                spawn(
                    Sprite::from_color(pale(0.5), Vec2::splat(SWATCH_SIZE + 1.0)),
                    Vec2::ZERO,
                    0.1,
                );
                let mut swatch = terrain_sprite(&terrain_atlas, terrain);
                swatch.rect = Some(Rect::new(0.0, TILE_SIZE, TILE_SIZE, 2.0 * TILE_SIZE));
                swatch.custom_size = Some(Vec2::splat(SWATCH_SIZE));
                spawn(swatch, Vec2::ZERO, 0.2);
            }
            LandingArt::Unit(unit, faction) => {
                let frames = awbrn_content::get_unit_animation_frames(
                    GraphicalMovement::Idle,
                    unit,
                    faction,
                );
                let sprite = Sprite::from_atlas_image(
                    unit_atlas.texture.clone(),
                    TextureAtlas {
                        layout: unit_atlas.layout.clone(),
                        index: frames.start_index() as usize,
                    },
                );
                spawn(sprite, unit_offset(&game_map, landing.tile, center), 0.2);
            }
        }
        if landing.image {
            spawn(art_sprite(&art.edge, TILE_SIZE), Vec2::ZERO, 0.3);
        }
    }
    *last = landings;
}

fn tile_center(tile: Pos, game_map: &GameMap) -> Vec2 {
    tile_center_world_position(tile, game_map).to_vec2()
}

/// Where a unit sprite sits against its tile's center, as the board places
/// units.
fn unit_offset(game_map: &GameMap, tile: Pos, center: Vec2) -> Vec2 {
    let unit_sprite = SpriteSize {
        width: awbrn_content::UNIT_SPRITE_WIDTH as f32,
        height: awbrn_content::UNIT_SPRITE_HEIGHT as f32,
        z_index: 0,
    };
    position_to_world_translation(&unit_sprite, tile, game_map).truncate() - center
}

fn terrain_sprite(atlas: &TerrainAtlasResource, terrain: GraphicalTerrain) -> Sprite {
    Sprite::from_atlas_image(
        atlas.texture.clone(),
        TextureAtlas {
            layout: atlas.layout.clone(),
            index: awbrn_content::spritesheet_index(Weather::Clear, terrain).index() as usize,
        },
    )
}

fn art_sprite(image: &Handle<Image>, size: f32) -> Sprite {
    Sprite {
        custom_size: Some(Vec2::splat(size)),
        ..Sprite::from_image(image.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modes::editor::{EditorArmy, EditorTile};
    use awbrn_map::AwbwMap;
    use awbrn_map::editor::{ArmyRoster, Connection, Symmetry};
    use awbrn_types::{Faction, FactionCode, Property, PropertyKind, VisualHp};

    fn editor(width: u8, height: u8, symmetry: Symmetry) -> MapEditor {
        let mut editor = MapEditor::blank(Dimensions::new(width, height));
        editor.set_roster(ArmyRoster::new([
            PlayerFaction::OrangeStar,
            PlayerFaction::BlueMoon,
        ]));
        editor.set_symmetry(symmetry);
        editor
    }

    fn mountain() -> Brush {
        Brush::Terrain {
            terrain: AwbwTerrain::Mountain,
        }
    }

    fn images(landings: &[Landing]) -> Vec<Pos> {
        landings
            .iter()
            .filter(|landing| landing.image)
            .map(|landing| landing.tile)
            .collect()
    }

    #[test]
    fn the_stroke_lands_under_the_pointer_and_wherever_the_fold_repeats_it() {
        let drawn = landings(
            &editor(5, 5, Symmetry::Rotate180),
            mountain(),
            Pos::new(0, 1),
        );

        assert_eq!(drawn[0].tile, Pos::new(0, 1));
        assert!(!drawn[0].image);
        assert_eq!(images(&drawn), vec![Pos::new(4, 3)]);
    }

    #[test]
    fn a_free_board_lands_only_under_the_pointer() {
        let drawn = landings(&editor(5, 5, Symmetry::None), mountain(), Pos::new(0, 1));

        assert_eq!(drawn.len(), 1);
        assert!(images(&drawn).is_empty());
    }

    #[test]
    fn a_tile_on_the_fold_is_its_own_only_image() {
        let drawn = landings(
            &editor(5, 5, Symmetry::MirrorLeftRight),
            mountain(),
            Pos::new(2, 3),
        );

        assert_eq!(drawn.len(), 1);
    }

    #[test]
    fn a_quarter_turn_lands_in_all_four_corners() {
        let drawn = landings(
            &editor(5, 5, Symmetry::Rotate90),
            mountain(),
            Pos::new(0, 1),
        );

        assert_eq!(images(&drawn).len(), 3);
    }

    #[test]
    fn a_building_lands_in_the_army_of_each_image() {
        let headquarters = Brush::Property {
            property: PropertyKind::HQ,
            faction: Some(FactionCode::new(PlayerFaction::OrangeStar)),
        };
        let drawn = landings(
            &editor(6, 6, Symmetry::Rotate180),
            headquarters,
            Pos::new(1, 1),
        );

        let drawn_as = |owner: PlayerFaction| {
            LandingArt::Tile(sample_tile(AwbwTerrain::Property(Property::HQ(owner))))
        };
        assert_eq!(drawn[0].art, drawn_as(PlayerFaction::OrangeStar));
        assert_eq!(drawn[1].art, drawn_as(PlayerFaction::BlueMoon));
    }

    #[test]
    fn a_neutral_building_stays_neutral_in_every_image() {
        let city = Brush::Property {
            property: PropertyKind::City,
            faction: None,
        };
        let drawn = landings(&editor(6, 6, Symmetry::Rotate180), city, Pos::new(1, 1));

        let neutral = LandingArt::Tile(sample_tile(AwbwTerrain::Property(Property::City(
            Faction::Neutral,
        ))));
        assert!(drawn.iter().all(|landing| landing.art == neutral));
    }

    #[test]
    fn a_unit_lands_in_the_army_of_each_image() {
        let infantry = Brush::Unit {
            unit: Unit::Infantry,
            faction: FactionCode::new(PlayerFaction::OrangeStar),
            hp: VisualHp::new(10),
        };
        let drawn = landings(
            &editor(6, 6, Symmetry::MirrorLeftRight),
            infantry,
            Pos::new(1, 1),
        );

        assert_eq!(
            drawn[1].art,
            LandingArt::Unit(Unit::Infantry, PlayerFaction::BlueMoon)
        );
    }

    /// A road's joins come from its neighbors, so the ghost shows the palette
    /// swatch rather than guessing the piece it will become.
    #[test]
    fn connecting_terrain_and_sea_land_as_swatches() {
        let board = editor(5, 5, Symmetry::None);
        let road = Brush::Connecting {
            connection: Connection::Road,
        };
        let sea = Brush::Terrain {
            terrain: AwbwTerrain::Sea,
        };

        assert!(matches!(
            landings(&board, road, Pos::new(1, 1))[0].art,
            LandingArt::Swatch(_)
        ));
        assert!(matches!(
            landings(&board, sea, Pos::new(1, 1))[0].art,
            LandingArt::Swatch(_)
        ));
        assert!(matches!(
            landings(&board, mountain(), Pos::new(1, 1))[0].art,
            LandingArt::Tile(_)
        ));
    }

    #[test]
    fn an_eraser_marks_only_its_images() {
        let drawn = landings(
            &editor(5, 5, Symmetry::Rotate180),
            Brush::Erase,
            Pos::new(0, 1),
        );

        assert_eq!(
            drawn,
            vec![Landing {
                tile: Pos::new(4, 3),
                image: true,
                art: LandingArt::Bare,
            }]
        );
    }

    fn army(faction: PlayerFaction, hq_at: &[(u8, u8)]) -> EditorArmy {
        EditorArmy {
            faction: FactionCode::new(faction),
            hq_at: hq_at.iter().map(|&(x, y)| EditorTile { x, y }).collect(),
            properties: 0,
            production: 0,
            units: 0,
            income: 0,
        }
    }

    #[test]
    fn every_uneven_tile_and_every_headquarters_of_an_army_with_two_is_framed() {
        let session = EditorSession::new(AwbwMap::new(Dimensions::new(8, 8), AwbwTerrain::Plain));
        let mut state = super::super::read_state(&session);
        state.uneven_groups = vec![vec![EditorTile { x: 0, y: 0 }, EditorTile { x: 7, y: 0 }]];
        state.armies = vec![
            army(PlayerFaction::OrangeStar, &[(1, 1)]),
            army(PlayerFaction::BlueMoon, &[(2, 2), (5, 5)]),
        ];

        assert_eq!(
            problem_tiles(&state),
            vec![
                Pos::new(0, 0),
                Pos::new(7, 0),
                Pos::new(2, 2),
                Pos::new(5, 5)
            ]
        );
    }

    #[test]
    fn the_ring_is_wider_than_its_tile_and_clear_inside() {
        let ring = banded_square(TILE_SIZE + 2.0 * RING_REACH, 2.0, &[(1.0, Color::WHITE)]);
        let texels = ring.texture_descriptor.size.width;
        let alpha_at =
            |x: u32, y: u32| ring.data.as_ref().unwrap()[((y * texels + x) * 4 + 3) as usize];

        assert_eq!(texels as f32, (TILE_SIZE + 2.0 * RING_REACH) * ART_TEXELS);
        assert_eq!(alpha_at(texels / 2, 2), 255, "the band runs along the edge");
        assert_eq!(
            alpha_at(texels / 2, texels / 2),
            0,
            "the tile shows through"
        );
        assert_eq!(alpha_at(0, 0), 0, "the corners are rounded");
    }
}
