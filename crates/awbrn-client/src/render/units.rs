use crate::core::INACTIVE_UNIT_COLOR;
use crate::core::SpriteSize;
use crate::core::coords::TILE_SIZE;
use crate::features::ReducedMotion;
use crate::modes::replay::navigation;
use crate::projection::{ClientProjectionSet, ProjectedUnitOverlayFlags, ProjectedUnitRenderState};
use crate::render::animation::{
    Animation, UnitPathAnimation, UnitVisualState, restore_unit_visual_state,
};
use crate::render::{UiAtlas, UnitAtlasResource};
use awbrn_bevy::world::{Faction, Unit, UnitActive};
use awbrn_content::get_unit_animation_frames;
use awbrn_content::{UnitOverlay, UnitOverlaySpec, unit_overlay_spec};
use bevy::sprite::Anchor;
use bevy::{log, prelude::*};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OverlayKind {
    Health,
    StatusCorner,
}

#[derive(Component, Debug, Default)]
pub struct UnitOverlayRegistry {
    health: Option<Entity>,
    status_corner: Option<Entity>,
}

impl UnitOverlayRegistry {
    fn overlay(&self, kind: OverlayKind) -> Option<Entity> {
        *self.overlay_slot(kind)
    }

    fn set_overlay(&mut self, kind: OverlayKind, entity: Entity) {
        *self.overlay_slot_mut(kind) = Some(entity);
    }

    fn clear_overlay(&mut self, kind: OverlayKind) -> Option<Entity> {
        self.overlay_slot_mut(kind).take()
    }

    fn overlay_slot(&self, kind: OverlayKind) -> &Option<Entity> {
        match kind {
            OverlayKind::Health => &self.health,
            OverlayKind::StatusCorner => &self.status_corner,
        }
    }

    fn overlay_slot_mut(&mut self, kind: OverlayKind) -> &mut Option<Entity> {
        match kind {
            OverlayKind::Health => &mut self.health,
            OverlayKind::StatusCorner => &mut self.status_corner,
        }
    }
}

/// The HP that a unit's health overlay shows in place of its own HP.
///
/// The move planner uses this to show the HP that a unit keeps after a
/// planned power. The unit's real HP does not change. Remove the component to
/// show the real HP again.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HealthOverlayOverride(pub awbrn_bevy::world::GraphicalHp);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlayVisual {
    pub kind: OverlayKind,
}

/// A unit's bottom-left corner, which shows one of its status marks at a time.
///
/// Capture, cargo, dive and the two warnings all claim the same corner, so
/// they take turns on it rather than drawing over one another.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct StatusCorner {
    /// The unit's marks, in the order the corner steps through them.
    marks: Vec<UnitOverlay>,
}

impl StatusCorner {
    fn from_flags(flags: ProjectedUnitOverlayFlags) -> Self {
        let marks = [
            (flags.capturing, UnitOverlay::Capturing),
            (flags.cargo, UnitOverlay::Cargo),
            (flags.dive, UnitOverlay::Dive),
            (flags.low_fuel, UnitOverlay::LowFuel),
            (flags.low_ammo, UnitOverlay::LowAmmo),
        ];
        Self {
            marks: marks
                .into_iter()
                .filter_map(|(present, mark)| present.then_some(mark))
                .collect(),
        }
    }

    /// The mark shown in board second `second`, or `None` for a blank frame.
    ///
    /// Every corner reads the same second, so the board steps in unison
    /// rather than shimmering.
    fn frame(&self, second: u64, reduced_motion: bool) -> Option<UnitOverlay> {
        match self.marks.as_slice() {
            [] => None,
            // A lone warning flashes against blank. Under reduced motion it
            // holds steady instead, since a blinking mark is the motion.
            [UnitOverlay::LowFuel | UnitOverlay::LowAmmo] if !reduced_motion && second % 2 == 1 => {
                None
            }
            marks => marks.get((second % marks.len() as u64) as usize).copied(),
        }
    }
}

type ProjectedUnitRenderQueryItem<'a> = (
    Entity,
    &'a ProjectedUnitRenderState,
    &'a mut Sprite,
    Option<&'a mut Animation>,
    &'a mut Visibility,
    &'a mut UnitOverlayRegistry,
    Has<UnitPathAnimation>,
);

type ProjectedUnitRenderFilter = (
    Or<(
        Added<ProjectedUnitRenderState>,
        Changed<ProjectedUnitRenderState>,
    )>,
    Without<OverlayVisual>,
);

type HealthOverlayQuery<'w, 's> = Query<
    'w,
    's,
    (&'static mut Sprite, &'static mut Transform),
    (With<OverlayVisual>, Without<StatusCorner>),
>;

/// `None` when the overlay has nothing to draw, as full-health and destroyed
/// units do.
fn health_overlay(health: awbrn_bevy::world::GraphicalHp) -> Option<UnitOverlaySpec> {
    unit_overlay_spec(UnitOverlay::Health(health))
}

fn overlay_translation(spec: &UnitOverlaySpec) -> Vec3 {
    Vec3::new(spec.offset.x, -spec.offset.y, 1.0)
}

fn spawn_overlay_entity(
    commands: &mut Commands,
    unit_entity: Entity,
    kind: OverlayKind,
    spec: &UnitOverlaySpec,
    ui_atlas: &UiAtlas,
) -> Entity {
    commands
        .spawn((
            ui_atlas.sprite_for(&spec.sprite_name),
            Transform::from_translation(overlay_translation(spec)),
            ChildOf(unit_entity),
            OverlayVisual { kind },
        ))
        .id()
}

fn reconcile_health_overlay(
    unit_entity: Entity,
    registry: &mut UnitOverlayRegistry,
    desired: Option<UnitOverlaySpec>,
    commands: &mut Commands,
    ui_atlas: &UiAtlas,
    overlay_query: &mut HealthOverlayQuery,
) {
    let kind = OverlayKind::Health;
    let Some(spec) = desired else {
        if let Some(overlay_entity) = registry.clear_overlay(kind) {
            commands.entity(overlay_entity).try_despawn();
        }
        return;
    };

    let existing = registry
        .overlay(kind)
        .and_then(|existing| overlay_query.get_mut(existing).ok());
    if let Some((mut sprite, mut transform)) = existing {
        *sprite = ui_atlas.sprite_for(&spec.sprite_name);
        transform.translation = overlay_translation(&spec);
    } else {
        let overlay_entity = spawn_overlay_entity(commands, unit_entity, kind, &spec, ui_atlas);
        registry.set_overlay(kind, overlay_entity);
    }
}

/// Keep one corner child while the unit has any mark; [`cycle_status_corners`]
/// picks what it shows.
fn reconcile_status_corner(
    unit_entity: Entity,
    registry: &mut UnitOverlayRegistry,
    desired: StatusCorner,
    commands: &mut Commands,
    ui_atlas: &UiAtlas,
    corners: &mut Query<&mut StatusCorner>,
) {
    let kind = OverlayKind::StatusCorner;
    let Some(first) = desired.marks.first().copied().and_then(unit_overlay_spec) else {
        if let Some(overlay_entity) = registry.clear_overlay(kind) {
            commands.entity(overlay_entity).try_despawn();
        }
        return;
    };

    match registry
        .overlay(kind)
        .and_then(|existing| corners.get_mut(existing).ok())
    {
        Some(mut corner) => {
            corner.set_if_neq(desired);
        }
        None => {
            let overlay_entity =
                spawn_overlay_entity(commands, unit_entity, kind, &first, ui_atlas);
            commands.entity(overlay_entity).insert(desired);
            registry.set_overlay(kind, overlay_entity);
        }
    }
}

pub(crate) fn sync_projected_unit_render_state(
    mut commands: Commands,
    ui_atlas: UiAtlas,
    mut units: Query<ProjectedUnitRenderQueryItem<'_>, ProjectedUnitRenderFilter>,
    mut health_overlays: HealthOverlayQuery,
    mut corners: Query<&mut StatusCorner>,
) {
    for (
        entity,
        projected,
        mut sprite,
        animation,
        mut visibility,
        mut registry,
        has_path_animation,
    ) in &mut units
    {
        if has_path_animation {
            continue;
        }

        let target = if projected.visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        visibility.set_if_neq(target);

        let visual_state = UnitVisualState {
            unit: projected.unit,
            faction: projected.faction,
            flip_x: sprite.flip_x,
        };
        restore_unit_visual_state(
            &mut commands,
            entity,
            &mut sprite,
            animation,
            visual_state,
            projected.active,
        );

        reconcile_health_overlay(
            entity,
            &mut registry,
            projected.overlays.health.and_then(health_overlay),
            &mut commands,
            &ui_atlas,
            &mut health_overlays,
        );
        reconcile_status_corner(
            entity,
            &mut registry,
            StatusCorner::from_flags(projected.overlays),
            &mut commands,
            &ui_atlas,
            &mut corners,
        );
    }
}

/// Step every status corner on the board's one-second clock.
///
/// A frame is a cut, never a fade or a scale: under reduced motion the swap
/// stays, because a cut is not motion.
fn cycle_status_corners(
    time: Res<Time>,
    reduced_motion: Res<ReducedMotion>,
    ui_atlas: UiAtlas,
    mut last_clock: Local<Option<(u64, bool)>>,
    mut corners: Query<(
        Ref<StatusCorner>,
        &mut Sprite,
        &mut Transform,
        &mut Visibility,
    )>,
) {
    let clock = (time.elapsed().as_secs(), reduced_motion.0);
    let previous = last_clock.replace(clock);

    for (corner, mut sprite, mut transform, mut visibility) in &mut corners {
        let shown = corner.frame(clock.0, clock.1);
        let unchanged = previous.map(|(second, reduced)| corner.frame(second, reduced));
        if !corner.is_changed() && unchanged == Some(shown) {
            continue;
        }

        let Some(spec) = shown.and_then(unit_overlay_spec) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        *sprite = ui_atlas.sprite_for(&spec.sprite_name);
        transform.translation = overlay_translation(&spec);
        visibility.set_if_neq(Visibility::Inherited);
    }
}

fn align_unit_sprites(mut query: Query<(&Sprite, &SpriteSize, &mut Anchor), With<Unit>>) {
    for (sprite, size, mut anchor) in &mut query {
        // Atlas frames align with the right edge of the cell. Move a flipped
        // frame by the extra cell width to keep its tile position constant.
        let x = if sprite.flip_x {
            (TILE_SIZE - size.width) / size.width
        } else {
            0.0
        };
        anchor.set_if_neq(Anchor(Vec2::new(x, 0.0)));
    }
}

/// Observer that handles unit spawning - creates the base sprite bundle.
pub(crate) fn handle_unit_spawn(
    trigger: On<Insert<Unit>>,
    mut commands: Commands,
    unit_atlas: Res<UnitAtlasResource>,
    query: Query<(&Unit, &Faction, Has<UnitActive>), Without<Sprite>>,
) {
    let entity = trigger.entity;
    let Ok((unit, faction, has_active)) = query.get(entity) else {
        return;
    };

    log::debug!(
        "Spawning unit of type {:?} for faction {:?} at entity {:?}",
        unit.0,
        faction.0,
        entity
    );

    let animation_frames =
        get_unit_animation_frames(awbrn_types::GraphicalMovement::Idle, unit.0, faction.0);

    let color = if has_active {
        Color::WHITE
    } else {
        INACTIVE_UNIT_COLOR
    };

    let mut sprite = Sprite::from_atlas_image(
        unit_atlas.texture.clone(),
        TextureAtlas {
            layout: unit_atlas.layout.clone(),
            index: animation_frames.start_index() as usize,
        },
    );
    sprite.color = color;

    commands
        .entity(entity)
        .insert((sprite, Anchor::default(), Visibility::Hidden));
}

#[derive(Debug)]
pub struct UnitRenderingPlugin;

impl Plugin for UnitRenderingPlugin {
    fn build(&self, app: &mut App) {
        app.register_required_components::<Unit, UnitOverlayRegistry>()
            .add_observer(handle_unit_spawn)
            .add_systems(
                Update,
                (
                    sync_projected_unit_render_state
                        .in_set(ClientProjectionSet::SyncRender)
                        .after(navigation::animate_unit_paths)
                        .before(crate::render::animation::animate_units),
                    cycle_status_corners.after(sync_projected_unit_render_state),
                    align_unit_sprites.after(sync_projected_unit_render_state),
                )
                    .run_if(in_state(crate::core::AppState::InGame)),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projection::project_unit_render_state;
    use crate::render::UiAtlasResource;
    use awbrn_bevy::world::{
        Ammo, CaptureProgress, CarriedBy, FriendlyFactions, Fuel, GraphicalHp, ViewerVisibility,
    };
    use awbrn_types::{GraphicalMovement, PlayerFaction};

    fn unit_render_test_app() -> App {
        let mut app = App::new();
        let mut atlas_assets = Assets::<crate::UiAtlasAsset>::default();
        let atlas_handle = atlas_assets.add(crate::UiAtlasAsset(awbrn_content::UiAtlasManifest {
            size: crate::UiAtlasSize {
                width: 128,
                height: 128,
            },
            sprites: vec![
                crate::UiAtlasSprite {
                    name: "HasCargo.png".to_string(),
                    x: 0,
                    y: 0,
                    width: 8,
                    height: 8,
                },
                crate::UiAtlasSprite {
                    name: "Capturing.png".to_string(),
                    x: 8,
                    y: 0,
                    width: 8,
                    height: 8,
                },
                crate::UiAtlasSprite {
                    name: "Healthv2/1.png".to_string(),
                    x: 16,
                    y: 0,
                    width: 8,
                    height: 8,
                },
                crate::UiAtlasSprite {
                    name: "Healthv2/5.png".to_string(),
                    x: 24,
                    y: 0,
                    width: 8,
                    height: 8,
                },
                crate::UiAtlasSprite {
                    name: "Healthv2/9.png".to_string(),
                    x: 32,
                    y: 0,
                    width: 8,
                    height: 8,
                },
                crate::UiAtlasSprite {
                    name: "Healthv2/Question.png".to_string(),
                    x: 40,
                    y: 0,
                    width: 8,
                    height: 8,
                },
                crate::UiAtlasSprite {
                    name: "Dive.png".to_string(),
                    x: 48,
                    y: 0,
                    width: 8,
                    height: 8,
                },
                crate::UiAtlasSprite {
                    name: "LowFuel.png".to_string(),
                    x: 56,
                    y: 0,
                    width: 7,
                    height: 8,
                },
                crate::UiAtlasSprite {
                    name: "LowAmmo.png".to_string(),
                    x: 64,
                    y: 0,
                    width: 7,
                    height: 8,
                },
            ],
        }));

        app.insert_resource(UnitAtlasResource {
            texture: Handle::default(),
            layout: Handle::default(),
        });
        app.insert_resource(UiAtlasResource {
            handle: atlas_handle,
            texture: Handle::default(),
            layout: Handle::default(),
        });
        app.insert_resource(atlas_assets);
        app.init_resource::<ViewerVisibility>();
        app.init_resource::<FriendlyFactions>();
        app.init_resource::<ReducedMotion>();
        app.insert_resource(Time::<()>::default());
        app.register_required_components::<Unit, UnitOverlayRegistry>()
            .add_observer(handle_unit_spawn)
            .add_systems(
                Update,
                (
                    project_unit_render_state,
                    sync_projected_unit_render_state
                        .before(crate::render::animation::animate_units),
                    cycle_status_corners,
                )
                    .chain(),
            );
        app
    }

    /// Move the board clock to `second` and draw that frame.
    fn update_at_second(app: &mut App, second: u64) {
        let target = std::time::Duration::from_secs(second);
        let elapsed = app.world().resource::<Time>().elapsed();
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(target - elapsed);
        app.update();
    }

    fn status_corner(app: &App, unit: Entity) -> Option<Entity> {
        app.world()
            .get::<UnitOverlayRegistry>(unit)
            .unwrap()
            .overlay(OverlayKind::StatusCorner)
    }

    /// The mark the unit's corner shows now, or `None` for blank.
    fn shown_mark(app: &App, unit: Entity) -> Option<&'static str> {
        let corner = status_corner(app, unit)?;
        if app.world().get::<Visibility>(corner) == Some(&Visibility::Hidden) {
            return None;
        }
        let index = overlay_sprite_index(app, corner);
        [
            "Capturing.png",
            "HasCargo.png",
            "Dive.png",
            "LowFuel.png",
            "LowAmmo.png",
        ]
        .into_iter()
        .find(|name| atlas_index(app, name) == index)
    }

    fn corner_children(app: &mut App, unit: Entity) -> usize {
        let mut query = app
            .world_mut()
            .query_filtered::<&ChildOf, (With<OverlayVisual>, With<StatusCorner>)>();
        query
            .iter(app.world())
            .filter(|parent| parent.parent() == unit)
            .count()
    }

    fn spawn_marked_unit(app: &mut App, unit: awbrn_types::Unit, marks: impl Bundle) -> Entity {
        app.world_mut()
            .spawn((
                Unit(unit),
                Faction(PlayerFaction::OrangeStar),
                UnitActive,
                marks,
            ))
            .id()
    }

    fn spawn_test_unit(app: &mut App, faction: PlayerFaction, active: bool) -> Entity {
        let entity = app
            .world_mut()
            .spawn((Unit(awbrn_types::Unit::Infantry), Faction(faction)))
            .id();

        if active {
            app.world_mut().entity_mut(entity).insert(UnitActive);
        }

        entity
    }

    fn atlas_index(app: &App, sprite_name: &str) -> usize {
        let atlas_res = app.world().resource::<UiAtlasResource>();
        let atlas_assets = app.world().resource::<Assets<crate::UiAtlasAsset>>();
        let atlas = atlas_assets.get(&atlas_res.handle).unwrap();
        *atlas.index_map().get(sprite_name).unwrap()
    }

    fn overlay_sprite_index(app: &App, overlay_entity: Entity) -> usize {
        app.world()
            .entity(overlay_entity)
            .get::<Sprite>()
            .unwrap()
            .texture_atlas
            .as_ref()
            .unwrap()
            .index
    }

    #[test]
    fn flipped_movement_and_idle_frames_keep_the_same_tile_alignment() {
        use crate::core::coords::position_to_world_translation;
        use crate::modes::replay::presentation::ReplayAdvanceLock;
        use awbrn_bevy::world::GameMap;
        use awbrn_map::Pos;
        use std::time::Duration;

        for (from, to) in [
            (Pos::new(0, 0), Pos::new(1, 0)),
            (Pos::new(1, 0), Pos::new(0, 0)),
        ] {
            for idle_flip_x in [false, true] {
                let mut app = App::new();
                let mut map = GameMap::default();
                map.set(awbrn_map::AwbrnMap::new(
                    awbrn_map::Dimensions::new(2, 1),
                    awbrn_types::GraphicalTerrain::Plain,
                ));
                let size = SpriteSize {
                    width: 23.0,
                    height: 24.0,
                    z_index: 2,
                };
                let start = position_to_world_translation(&size, from, &map);
                let end = position_to_world_translation(&size, to, &map);
                app.insert_resource(map)
                    .insert_resource(Time::<()>::default())
                    .init_resource::<ReplayAdvanceLock>()
                    .init_resource::<ViewerVisibility>()
                    .add_systems(
                        Update,
                        (navigation::animate_unit_paths, align_unit_sprites).chain(),
                    );
                let entity = app
                    .world_mut()
                    .spawn((
                        Unit(awbrn_types::Unit::Infantry),
                        Faction(PlayerFaction::GreenEarth),
                        Sprite::default(),
                        Anchor::default(),
                        size,
                        Transform::from_translation(start),
                        Visibility::Inherited,
                        UnitPathAnimation::new(vec![from, to], idle_flip_x).unwrap(),
                    ))
                    .id();

                for _ in 0..30 {
                    app.world_mut()
                        .resource_mut::<Time>()
                        .advance_by(Duration::from_millis(10));
                    app.update();
                    let unit = app.world().entity(entity);
                    let sprite = unit.get::<Sprite>().unwrap();
                    let anchor = unit.get::<Anchor>().unwrap();
                    let translation = unit.get::<Transform>().unwrap().translation;
                    // A 16-pixel frame has 7 pixels of padding on its left.
                    // The padding moves to the right when the frame is flipped.
                    let padding = if sprite.flip_x {
                        0.0
                    } else {
                        size.width - TILE_SIZE
                    };
                    let frame_left =
                        translation.x - size.width / 2.0 - anchor.x * size.width + padding;
                    let tile_left =
                        translation.x + (size.width - TILE_SIZE) / 2.0 - TILE_SIZE / 2.0;
                    assert!((frame_left - tile_left).abs() < 1e-5);
                    assert!(translation.x >= start.x.min(end.x));
                    assert!(translation.x <= start.x.max(end.x));
                }
                let unit = app.world().entity(entity);
                assert!(unit.get::<UnitPathAnimation>().is_none());
                assert_eq!(unit.get::<Transform>().unwrap().translation, end);
                assert_eq!(unit.get::<Sprite>().unwrap().flip_x, idle_flip_x);
            }
        }
    }

    #[test]
    fn active_unit_spawn_inserts_idle_animation() {
        let mut app = unit_render_test_app();
        let entity = spawn_test_unit(&mut app, PlayerFaction::GreenEarth, true);

        app.update();

        let expected = get_unit_animation_frames(
            GraphicalMovement::Idle,
            awbrn_types::Unit::Infantry,
            PlayerFaction::GreenEarth,
        );
        let sprite = app.world().entity(entity).get::<Sprite>().unwrap();
        let animation = app.world().entity(entity).get::<Animation>().unwrap();

        assert_eq!(sprite.color, Color::WHITE);
        assert_eq!(
            sprite.texture_atlas.as_ref().unwrap().index,
            expected.start_index() as usize
        );
        assert_eq!(animation.start_index, expected.start_index());
        assert_eq!(animation.frame_durations, expected.raw());
        assert_eq!(animation.current_frame, 0);
    }

    /// Transition sync rewrites `Unit` on every action.
    ///
    /// The spawn observer must not treat that as a new unit: it would restart
    /// the idle animation and hide a unit whose projected state has not
    /// changed, which leaves every unit but the ones that acted invisible.
    #[test]
    fn rewriting_unit_keeps_the_sprite_and_the_visibility() {
        let mut app = unit_render_test_app();
        let entity = spawn_test_unit(&mut app, PlayerFaction::GreenEarth, true);

        app.update();
        let frame = 3;
        app.world_mut()
            .entity_mut(entity)
            .get_mut::<Animation>()
            .unwrap()
            .current_frame = frame;

        app.world_mut()
            .entity_mut(entity)
            .insert(Unit(awbrn_types::Unit::Infantry));
        app.update();

        assert_eq!(
            app.world().entity(entity).get::<Visibility>(),
            Some(&Visibility::Inherited),
            "a rewritten unit stays visible"
        );
        assert_eq!(
            app.world()
                .entity(entity)
                .get::<Animation>()
                .unwrap()
                .current_frame,
            frame,
            "a rewritten unit keeps its idle animation"
        );
    }

    #[test]
    fn inactive_unit_spawn_stays_grey_and_unanimated() {
        let mut app = unit_render_test_app();
        let entity = spawn_test_unit(&mut app, PlayerFaction::GreenEarth, false);

        app.update();

        let expected = get_unit_animation_frames(
            GraphicalMovement::Idle,
            awbrn_types::Unit::Infantry,
            PlayerFaction::GreenEarth,
        );
        let sprite = app.world().entity(entity).get::<Sprite>().unwrap();

        assert_eq!(sprite.color, INACTIVE_UNIT_COLOR);
        assert_eq!(
            sprite.texture_atlas.as_ref().unwrap().index,
            expected.start_index() as usize
        );
        assert!(app.world().entity(entity).get::<Animation>().is_none());
    }

    #[test]
    fn reinserting_unit_active_refreshes_idle_animation() {
        let mut app = unit_render_test_app();
        let entity = spawn_test_unit(&mut app, PlayerFaction::GreenEarth, true);

        app.update();

        {
            let mut entity_mut = app.world_mut().entity_mut(entity);
            let mut animation = entity_mut.get_mut::<Animation>().unwrap();
            animation.start_index = 999;
            animation.current_frame = 3;
        };

        app.world_mut().entity_mut(entity).insert(UnitActive);
        app.update();

        let expected = get_unit_animation_frames(
            GraphicalMovement::Idle,
            awbrn_types::Unit::Infantry,
            PlayerFaction::GreenEarth,
        );
        let animation = app.world().entity(entity).get::<Animation>().unwrap();

        assert_eq!(animation.start_index, expected.start_index());
        assert_eq!(animation.frame_durations, expected.raw());
        assert_eq!(animation.current_frame, 0);
    }

    #[test]
    fn faction_change_updates_active_and_inactive_idle_state() {
        let mut app = unit_render_test_app();
        let active_entity = spawn_test_unit(&mut app, PlayerFaction::GreenEarth, true);
        let inactive_entity = spawn_test_unit(&mut app, PlayerFaction::GreenEarth, false);

        app.update();

        app.world_mut()
            .entity_mut(active_entity)
            .insert(Faction(PlayerFaction::BlueMoon));
        app.world_mut()
            .entity_mut(inactive_entity)
            .insert(Faction(PlayerFaction::BlueMoon));
        app.update();

        let expected = get_unit_animation_frames(
            GraphicalMovement::Idle,
            awbrn_types::Unit::Infantry,
            PlayerFaction::BlueMoon,
        );

        let active_sprite = app.world().entity(active_entity).get::<Sprite>().unwrap();
        let active_animation = app
            .world()
            .entity(active_entity)
            .get::<Animation>()
            .unwrap();
        assert_eq!(
            active_sprite.texture_atlas.as_ref().unwrap().index,
            expected.start_index() as usize
        );
        assert_eq!(active_animation.start_index, expected.start_index());
        assert_eq!(active_sprite.color, Color::WHITE);

        let inactive_sprite = app.world().entity(inactive_entity).get::<Sprite>().unwrap();
        assert_eq!(
            inactive_sprite.texture_atlas.as_ref().unwrap().index,
            expected.start_index() as usize
        );
        assert_eq!(inactive_sprite.color, INACTIVE_UNIT_COLOR);
        assert!(
            app.world()
                .entity(inactive_entity)
                .get::<Animation>()
                .is_none()
        );
    }

    #[test]
    fn health_overlay_updates_in_place_when_hp_changes() {
        let mut app = unit_render_test_app();
        let unit = spawn_test_unit(&mut app, PlayerFaction::GreenEarth, true);
        app.update();

        app.world_mut()
            .entity_mut(unit)
            .insert(GraphicalHp::from(awvm::semantic::ObservedUnitHp::Exact(90)));
        app.update();
        let initial_overlay = app
            .world()
            .entity(unit)
            .get::<UnitOverlayRegistry>()
            .unwrap()
            .overlay(OverlayKind::Health)
            .unwrap();
        assert_eq!(
            overlay_sprite_index(&app, initial_overlay),
            atlas_index(&app, "Healthv2/9.png")
        );

        app.world_mut()
            .entity_mut(unit)
            .insert(GraphicalHp::from(awvm::semantic::ObservedUnitHp::Exact(10)));
        app.update();
        let updated_overlay = app
            .world()
            .entity(unit)
            .get::<UnitOverlayRegistry>()
            .unwrap()
            .overlay(OverlayKind::Health)
            .unwrap();

        assert_eq!(initial_overlay, updated_overlay);
        assert_eq!(
            overlay_sprite_index(&app, updated_overlay),
            atlas_index(&app, "Healthv2/1.png")
        );
    }

    #[test]
    fn health_overlay_override_replaces_the_shown_hp_until_removed() {
        let mut app = unit_render_test_app();
        let entity = spawn_test_unit(&mut app, PlayerFaction::OrangeStar, true);
        app.world_mut()
            .entity_mut(entity)
            .insert(GraphicalHp::from(awbrn_types::ExactHp::new(100)));
        app.update();
        let registry = app.world().get::<UnitOverlayRegistry>(entity).unwrap();
        assert!(registry.overlay(OverlayKind::Health).is_none());

        app.world_mut()
            .entity_mut(entity)
            .insert(HealthOverlayOverride(GraphicalHp::from(
                awbrn_types::ExactHp::new(1),
            )));
        app.update();
        let health = app
            .world()
            .get::<UnitOverlayRegistry>(entity)
            .unwrap()
            .overlay(OverlayKind::Health)
            .unwrap();
        assert_eq!(
            overlay_sprite_index(&app, health),
            atlas_index(&app, "Healthv2/1.png")
        );

        app.world_mut()
            .entity_mut(entity)
            .remove::<HealthOverlayOverride>();
        app.update();
        let registry = app.world().get::<UnitOverlayRegistry>(entity).unwrap();
        assert!(registry.overlay(OverlayKind::Health).is_none());
    }

    #[test]
    fn health_overlay_is_removed_at_full_health() {
        let mut app = unit_render_test_app();
        let unit = spawn_test_unit(&mut app, PlayerFaction::GreenEarth, true);
        app.update();

        app.world_mut()
            .entity_mut(unit)
            .insert(GraphicalHp::from(awvm::semantic::ObservedUnitHp::Exact(90)));
        app.update();
        assert!(
            app.world()
                .entity(unit)
                .get::<UnitOverlayRegistry>()
                .unwrap()
                .overlay(OverlayKind::Health)
                .is_some()
        );

        app.world_mut().entity_mut(unit).insert(GraphicalHp::from(
            awvm::semantic::ObservedUnitHp::Exact(100),
        ));
        app.update();
        assert!(
            app.world()
                .entity(unit)
                .get::<UnitOverlayRegistry>()
                .unwrap()
                .overlay(OverlayKind::Health)
                .is_none()
        );
    }

    #[test]
    fn hidden_hp_uses_the_question_mark_overlay() {
        let mut app = unit_render_test_app();
        let unit = spawn_test_unit(&mut app, PlayerFaction::GreenEarth, true);
        app.world_mut().entity_mut(unit).insert(GraphicalHp::Hidden);
        app.update();

        let overlay = app
            .world()
            .entity(unit)
            .get::<UnitOverlayRegistry>()
            .unwrap()
            .overlay(OverlayKind::Health)
            .unwrap();
        assert_eq!(
            overlay_sprite_index(&app, overlay),
            atlas_index(&app, "Healthv2/Question.png")
        );
    }

    #[test]
    fn health_and_status_corner_overlays_can_coexist() {
        let mut app = unit_render_test_app();
        let unit = spawn_test_unit(&mut app, PlayerFaction::GreenEarth, true);
        app.update();

        app.world_mut()
            .entity_mut(unit)
            .insert(GraphicalHp::from(awvm::semantic::ObservedUnitHp::Exact(50)));
        app.world_mut()
            .entity_mut(unit)
            .insert(CaptureProgress::new(10).unwrap());
        app.update();

        let registry = app
            .world()
            .entity(unit)
            .get::<UnitOverlayRegistry>()
            .unwrap();
        let health = registry.overlay(OverlayKind::Health).unwrap();
        let corner = registry.overlay(OverlayKind::StatusCorner).unwrap();

        assert_ne!(health, corner);
        assert_eq!(
            overlay_sprite_index(&app, health),
            atlas_index(&app, "Healthv2/5.png")
        );
        assert_eq!(shown_mark(&app, unit), Some("Capturing.png"));
    }

    #[test]
    fn health_and_status_corner_overlays_are_tracked_when_spawned_together() {
        let mut app = unit_render_test_app();
        let unit = spawn_marked_unit(
            &mut app,
            awbrn_types::Unit::Infantry,
            (
                GraphicalHp::from(awvm::semantic::ObservedUnitHp::Exact(50)),
                CaptureProgress::new(10).unwrap(),
            ),
        );

        app.update();

        let health = app
            .world()
            .get::<UnitOverlayRegistry>(unit)
            .unwrap()
            .overlay(OverlayKind::Health)
            .unwrap();
        assert_eq!(
            overlay_sprite_index(&app, health),
            atlas_index(&app, "Healthv2/5.png")
        );
        assert_eq!(shown_mark(&app, unit), Some("Capturing.png"));
    }

    #[test]
    fn cargo_mark_tracks_has_cargo_relationship() {
        let mut app = unit_render_test_app();
        let transport = spawn_test_unit(&mut app, PlayerFaction::GreenEarth, true);
        let cargo = spawn_test_unit(&mut app, PlayerFaction::GreenEarth, true);
        app.update();

        app.world_mut()
            .entity_mut(cargo)
            .insert(CarriedBy(transport));
        app.update();
        assert_eq!(shown_mark(&app, transport), Some("HasCargo.png"));

        app.world_mut().entity_mut(cargo).remove::<CarriedBy>();
        app.update();
        assert!(status_corner(&app, transport).is_none());
    }

    /// Every mark shares the bottom-left corner, so they take turns in one
    /// child instead of drawing over each other, in the order capturing,
    /// cargo, dive, low fuel, low ammo.
    #[test]
    fn status_corner_steps_through_every_mark_in_one_child() {
        let mut app = unit_render_test_app();
        let mech = spawn_marked_unit(
            &mut app,
            awbrn_types::Unit::Mech,
            (CaptureProgress::new(10).unwrap(), Fuel(10), Ammo(0)),
        );

        let shown: Vec<_> = (0..4)
            .map(|second| {
                update_at_second(&mut app, second);
                shown_mark(&app, mech)
            })
            .collect();
        assert_eq!(
            shown,
            [
                Some("Capturing.png"),
                Some("LowFuel.png"),
                Some("LowAmmo.png"),
                Some("Capturing.png"),
            ]
        );
        assert_eq!(corner_children(&mut app, mech), 1);
    }

    /// The board blinks in unison: a unit marked later joins the clock where
    /// it stands rather than starting its own.
    #[test]
    fn status_corners_share_one_board_clock() {
        let mut app = unit_render_test_app();
        let first = spawn_marked_unit(
            &mut app,
            awbrn_types::Unit::Mech,
            (CaptureProgress::new(10).unwrap(), Fuel(10)),
        );
        update_at_second(&mut app, 0);
        update_at_second(&mut app, 1);

        let second = spawn_marked_unit(
            &mut app,
            awbrn_types::Unit::Infantry,
            (CaptureProgress::new(10).unwrap(), Fuel(10)),
        );
        app.update();
        assert_eq!(shown_mark(&app, first), Some("LowFuel.png"));
        assert_eq!(shown_mark(&app, second), Some("LowFuel.png"));

        update_at_second(&mut app, 2);
        assert_eq!(shown_mark(&app, first), Some("Capturing.png"));
        assert_eq!(shown_mark(&app, second), Some("Capturing.png"));
    }

    #[test]
    fn a_lone_warning_flashes_and_holds_steady_under_reduced_motion() {
        let mut app = unit_render_test_app();
        let unit = spawn_marked_unit(&mut app, awbrn_types::Unit::Infantry, Fuel(10));

        update_at_second(&mut app, 0);
        assert_eq!(shown_mark(&app, unit), Some("LowFuel.png"));
        update_at_second(&mut app, 1);
        assert_eq!(shown_mark(&app, unit), None, "a lone warning blinks");
        update_at_second(&mut app, 2);
        assert_eq!(shown_mark(&app, unit), Some("LowFuel.png"));

        app.insert_resource(ReducedMotion(true));
        update_at_second(&mut app, 3);
        assert_eq!(
            shown_mark(&app, unit),
            Some("LowFuel.png"),
            "reduced motion holds a lone warning steady"
        );
    }

    #[test]
    fn a_lone_persistent_mark_holds_still() {
        let mut app = unit_render_test_app();
        let unit = spawn_marked_unit(
            &mut app,
            awbrn_types::Unit::Infantry,
            CaptureProgress::new(10).unwrap(),
        );

        for second in 0..3 {
            update_at_second(&mut app, second);
            assert_eq!(shown_mark(&app, unit), Some("Capturing.png"));
        }
    }

    /// Reduced motion keeps the one-second swap between several marks, since
    /// a cut from one frame to the next is not motion.
    #[test]
    fn reduced_motion_keeps_the_swap_between_marks() {
        let mut app = unit_render_test_app();
        app.insert_resource(ReducedMotion(true));
        let unit = spawn_marked_unit(
            &mut app,
            awbrn_types::Unit::Mech,
            (CaptureProgress::new(10).unwrap(), Ammo(0)),
        );

        update_at_second(&mut app, 0);
        assert_eq!(shown_mark(&app, unit), Some("Capturing.png"));
        update_at_second(&mut app, 1);
        assert_eq!(shown_mark(&app, unit), Some("LowAmmo.png"));
    }
}
