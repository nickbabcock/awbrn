use awbrn_map::Position;
use bevy::prelude::*;

use crate::core::{AppState, RenderLayer, SpriteSize};
use crate::features::fog::{FogActive, FogOfWarMap};
use awbrn_game::world::GameMap;

const FOG_OVERLAY_ALPHA: f32 = 0.75;

#[derive(Component)]
pub struct FogOverlayTile;

#[derive(Component)]
struct FogTilePosition(Position);

const FOG_OVERLAY_SPRITE_SIZE: SpriteSize = SpriteSize {
    width: 16.0,
    height: 16.0,
    z_index: RenderLayer::FOG_OVERLAY,
};

pub fn spawn_fog_overlay_tiles(
    mut commands: Commands,
    game_map: Res<GameMap>,
    existing: Query<Entity, With<FogOverlayTile>>,
) {
    if !existing.is_empty() {
        return;
    }

    for y in 0..game_map.height() {
        for x in 0..game_map.width() {
            let pos = Position::new(x, y);
            let world_pos = crate::core::position_to_world_translation(
                &FOG_OVERLAY_SPRITE_SIZE,
                pos,
                &game_map,
            );

            commands.spawn((
                FogOverlayTile,
                FogTilePosition(pos),
                Sprite::from_color(
                    Color::srgba(0.0, 0.0, 0.0, 0.0),
                    Vec2::new(
                        FOG_OVERLAY_SPRITE_SIZE.width,
                        FOG_OVERLAY_SPRITE_SIZE.height,
                    ),
                ),
                FOG_OVERLAY_SPRITE_SIZE,
                Transform::from_translation(world_pos),
            ));
        }
    }
}

fn update_fog_overlay(
    fog_map: Res<FogOfWarMap>,
    fog_active: Res<FogActive>,
    mut query: Query<(&FogTilePosition, &mut Sprite), With<FogOverlayTile>>,
) {
    for (tile_pos, mut sprite) in &mut query {
        let target_alpha = if fog_active.0 && fog_map.is_fogged(tile_pos.0) {
            FOG_OVERLAY_ALPHA
        } else {
            0.0
        };
        sprite.color.set_alpha(target_alpha);
    }
}

pub struct FogOverlayPlugin;

impl Plugin for FogOverlayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            update_fog_overlay
                .run_if(in_state(AppState::InGame))
                .run_if(resource_changed::<FogOfWarMap>.or(resource_changed::<FogActive>)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use awbrn_types::GraphicalTerrain;
    use bevy::ecs::system::RunSystemOnce;

    fn fog_overlay_test_app() -> App {
        let mut app = App::new();
        app.init_resource::<GameMap>();
        app.init_resource::<FogOfWarMap>();
        app.init_resource::<FogActive>();
        app
    }

    #[test]
    fn spawn_creates_correct_number_of_overlay_entities() {
        let mut app = fog_overlay_test_app();
        app.world_mut()
            .resource_mut::<GameMap>()
            .set(awbrn_map::AwbrnMap::new(3, 2, GraphicalTerrain::Plain));

        app.world_mut()
            .run_system_once(spawn_fog_overlay_tiles)
            .unwrap();

        let count = app
            .world_mut()
            .query_filtered::<Entity, With<FogOverlayTile>>()
            .iter(app.world())
            .count();
        assert_eq!(count, 6);
    }

    #[test]
    fn fog_active_with_hidden_map_sets_overlay_alpha() {
        let mut app = fog_overlay_test_app();
        app.world_mut()
            .resource_mut::<GameMap>()
            .set(awbrn_map::AwbrnMap::new(2, 2, GraphicalTerrain::Plain));
        app.world_mut().resource_mut::<FogActive>().0 = true;
        app.world_mut().resource_mut::<FogOfWarMap>().reset(2, 2);

        app.world_mut()
            .run_system_once(spawn_fog_overlay_tiles)
            .unwrap();
        app.world_mut().run_system_once(update_fog_overlay).unwrap();

        let mut query = app
            .world_mut()
            .query_filtered::<&Sprite, With<FogOverlayTile>>();
        for sprite in query.iter(app.world()) {
            assert!(
                (sprite.color.alpha() - FOG_OVERLAY_ALPHA).abs() < f32::EPSILON,
                "fogged tile overlay alpha should be {}",
                FOG_OVERLAY_ALPHA
            );
        }
    }

    #[test]
    fn fog_inactive_sets_overlay_alpha_zero() {
        let mut app = fog_overlay_test_app();
        app.world_mut()
            .resource_mut::<GameMap>()
            .set(awbrn_map::AwbrnMap::new(2, 2, GraphicalTerrain::Plain));
        app.world_mut().resource_mut::<FogActive>().0 = false;

        app.world_mut()
            .run_system_once(spawn_fog_overlay_tiles)
            .unwrap();
        app.world_mut().run_system_once(update_fog_overlay).unwrap();

        let mut query = app
            .world_mut()
            .query_filtered::<&Sprite, With<FogOverlayTile>>();
        for sprite in query.iter(app.world()) {
            assert!(
                sprite.color.alpha().abs() < f32::EPSILON,
                "inactive fog overlay alpha should be 0"
            );
        }
    }

    #[test]
    fn revealed_tiles_get_alpha_zero() {
        let mut app = fog_overlay_test_app();
        app.world_mut()
            .resource_mut::<GameMap>()
            .set(awbrn_map::AwbrnMap::new(2, 1, GraphicalTerrain::Plain));
        app.world_mut().resource_mut::<FogActive>().0 = true;
        {
            let mut fog_map = app.world_mut().resource_mut::<FogOfWarMap>();
            fog_map.reset(2, 1);
            fog_map.reveal(Position::new(0, 0));
        }

        app.world_mut()
            .run_system_once(spawn_fog_overlay_tiles)
            .unwrap();
        app.world_mut().run_system_once(update_fog_overlay).unwrap();

        let mut query = app
            .world_mut()
            .query_filtered::<(&FogTilePosition, &Sprite), With<FogOverlayTile>>();
        for (tile_pos, sprite) in query.iter(app.world()) {
            if tile_pos.0 == Position::new(0, 0) {
                assert!(
                    sprite.color.alpha().abs() < f32::EPSILON,
                    "revealed tile should have alpha 0"
                );
            } else {
                assert!(
                    (sprite.color.alpha() - FOG_OVERLAY_ALPHA).abs() < f32::EPSILON,
                    "hidden tile should have alpha {}",
                    FOG_OVERLAY_ALPHA
                );
            }
        }
    }

    #[test]
    fn air_units_visible_tiles_still_get_fog_overlay() {
        let mut app = fog_overlay_test_app();
        app.world_mut()
            .resource_mut::<GameMap>()
            .set(awbrn_map::AwbrnMap::new(1, 1, GraphicalTerrain::Plain));
        app.world_mut().resource_mut::<FogActive>().0 = true;
        {
            let mut fog_map = app.world_mut().resource_mut::<FogOfWarMap>();
            fog_map.reset(1, 1);
            fog_map.reveal_air_units(Position::new(0, 0));
        }

        app.world_mut()
            .run_system_once(spawn_fog_overlay_tiles)
            .unwrap();
        app.world_mut().run_system_once(update_fog_overlay).unwrap();

        let mut query = app
            .world_mut()
            .query_filtered::<&Sprite, With<FogOverlayTile>>();
        let sprite = query.iter(app.world()).next().unwrap();
        assert!(
            (sprite.color.alpha() - FOG_OVERLAY_ALPHA).abs() < f32::EPSILON,
            "AirUnitsVisible tile should still have fog overlay"
        );
    }
}
