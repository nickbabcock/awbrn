use crate::core::INACTIVE_UNIT_COLOR;
use crate::modes::replay::navigation;
use crate::projection::{ClientProjectionSet, ProjectedUnitOverlayFlags, ProjectedUnitRenderState};
use crate::render::animation::{
    Animation, UnitPathAnimation, UnitVisualState, restore_unit_visual_state,
};
use crate::render::{UiAtlas, UnitAtlasResource};
use awbrn_content::get_unit_animation_frames;
use awbrn_game::world::{Faction, Unit, UnitActive};
use bevy::sprite::Anchor;
use bevy::{log, prelude::*};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OverlayKind {
    Health,
    Capturing,
    Cargo,
    Dive,
    LowAmmo,
    LowFuel,
}

#[derive(Component, Debug, Default)]
pub struct UnitOverlayRegistry {
    health: Option<Entity>,
    capturing: Option<Entity>,
    cargo: Option<Entity>,
    dive: Option<Entity>,
    low_ammo: Option<Entity>,
    low_fuel: Option<Entity>,
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
            OverlayKind::Capturing => &self.capturing,
            OverlayKind::Cargo => &self.cargo,
            OverlayKind::Dive => &self.dive,
            OverlayKind::LowAmmo => &self.low_ammo,
            OverlayKind::LowFuel => &self.low_fuel,
        }
    }

    fn overlay_slot_mut(&mut self, kind: OverlayKind) -> &mut Option<Entity> {
        match kind {
            OverlayKind::Health => &mut self.health,
            OverlayKind::Capturing => &mut self.capturing,
            OverlayKind::Cargo => &mut self.cargo,
            OverlayKind::Dive => &mut self.dive,
            OverlayKind::LowAmmo => &mut self.low_ammo,
            OverlayKind::LowFuel => &mut self.low_fuel,
        }
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlayVisual {
    pub kind: OverlayKind,
}

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct OverlayBlink {
    pub period_secs: f32,
    pub min_alpha: f32,
    pub max_alpha: f32,
}

#[derive(Debug, Clone)]
struct OverlaySpec {
    sprite_name: String,
    translation: Vec3,
    blink: Option<OverlayBlink>,
}

impl OverlaySpec {
    fn new(sprite_name: impl Into<String>, translation: Vec3) -> Self {
        Self {
            sprite_name: sprite_name.into(),
            translation,
            blink: None,
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

fn health_overlay(value: u8) -> OverlaySpec {
    OverlaySpec::new(format!("Healthv2/{}.png", value), Vec3::new(7.5, -8.0, 1.0))
}

fn capturing_overlay() -> OverlaySpec {
    OverlaySpec::new("Capturing.png", Vec3::new(0.0, -8.0, 1.0))
}

fn cargo_overlay() -> OverlaySpec {
    OverlaySpec::new("HasCargo.png", Vec3::new(0.0, -8.0, 1.0))
}

fn dive_overlay() -> OverlaySpec {
    OverlaySpec::new("Dive.png", Vec3::new(0.0, -8.0, 1.0))
}

fn spawn_overlay_entity(
    commands: &mut Commands,
    unit_entity: Entity,
    kind: OverlayKind,
    spec: &OverlaySpec,
    ui_atlas: &UiAtlas,
) -> Entity {
    let mut entity_commands = commands.spawn((
        ui_atlas.sprite_for(&spec.sprite_name),
        Transform::from_translation(spec.translation),
        ChildOf(unit_entity),
        OverlayVisual { kind },
    ));

    if let Some(blink) = spec.blink {
        entity_commands.insert(blink);
    }

    entity_commands.id()
}

fn reconcile_overlay(
    unit_entity: Entity,
    registry: &mut UnitOverlayRegistry,
    kind: OverlayKind,
    desired: Option<OverlaySpec>,
    commands: &mut Commands,
    ui_atlas: &UiAtlas,
    overlay_query: &mut Query<
        (&mut Sprite, &mut Transform, Option<&mut OverlayBlink>),
        With<OverlayVisual>,
    >,
) {
    match desired {
        Some(spec) => {
            let Some(existing) = registry.overlay(kind) else {
                let overlay_entity =
                    spawn_overlay_entity(commands, unit_entity, kind, &spec, ui_atlas);
                registry.set_overlay(kind, overlay_entity);
                return;
            };

            let overlay_updated =
                if let Ok((mut sprite, mut transform, blink)) = overlay_query.get_mut(existing) {
                    *sprite = ui_atlas.sprite_for(&spec.sprite_name);
                    transform.translation = spec.translation;

                    match (blink, spec.blink) {
                        (Some(mut existing), Some(next)) => {
                            *existing = next;
                        }
                        (Some(_), None) => {
                            commands.entity(existing).remove::<OverlayBlink>();
                        }
                        (None, Some(next)) => {
                            commands.entity(existing).insert(next);
                        }
                        (None, None) => {}
                    }

                    sprite.color.set_alpha(1.0);
                    true
                } else {
                    false
                };

            if !overlay_updated {
                let overlay_entity =
                    spawn_overlay_entity(commands, unit_entity, kind, &spec, ui_atlas);
                registry.set_overlay(kind, overlay_entity);
            }
        }
        None => {
            if let Some(overlay_entity) = registry.clear_overlay(kind) {
                commands.entity(overlay_entity).try_despawn();
            }
        }
    }
}

fn sync_projected_overlays(
    entity: Entity,
    overlays: ProjectedUnitOverlayFlags,
    registry: &mut UnitOverlayRegistry,
    commands: &mut Commands,
    ui_atlas: &UiAtlas,
    overlay_query: &mut Query<
        (&mut Sprite, &mut Transform, Option<&mut OverlayBlink>),
        With<OverlayVisual>,
    >,
) {
    reconcile_overlay(
        entity,
        registry,
        OverlayKind::Health,
        overlays.health.map(health_overlay),
        commands,
        ui_atlas,
        overlay_query,
    );
    reconcile_overlay(
        entity,
        registry,
        OverlayKind::Capturing,
        overlays.capturing.then(capturing_overlay),
        commands,
        ui_atlas,
        overlay_query,
    );
    reconcile_overlay(
        entity,
        registry,
        OverlayKind::Cargo,
        overlays.cargo.then(cargo_overlay),
        commands,
        ui_atlas,
        overlay_query,
    );
    reconcile_overlay(
        entity,
        registry,
        OverlayKind::Dive,
        overlays.dive.then(dive_overlay),
        commands,
        ui_atlas,
        overlay_query,
    );
}

pub(crate) fn sync_projected_unit_render_state(
    mut commands: Commands,
    ui_atlas: UiAtlas,
    mut units: Query<ProjectedUnitRenderQueryItem<'_>, ProjectedUnitRenderFilter>,
    mut overlay_query: Query<
        (&mut Sprite, &mut Transform, Option<&mut OverlayBlink>),
        With<OverlayVisual>,
    >,
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

        sync_projected_overlays(
            entity,
            projected.overlays,
            &mut registry,
            &mut commands,
            &ui_atlas,
            &mut overlay_query,
        );
    }
}

fn animate_blinking_overlays(time: Res<Time>, mut query: Query<(&OverlayBlink, &mut Sprite)>) {
    let elapsed = time.elapsed_secs();

    for (blink, mut sprite) in &mut query {
        if blink.period_secs <= 0.0 {
            sprite.color.set_alpha(blink.max_alpha);
            continue;
        }

        let phase = (elapsed / blink.period_secs) * core::f32::consts::TAU;
        let t = (phase.sin() + 1.0) * 0.5;
        let alpha = blink.min_alpha + (blink.max_alpha - blink.min_alpha) * t;
        sprite.color.set_alpha(alpha);
    }
}

/// Observer that handles unit spawning - creates the base sprite bundle.
pub(crate) fn handle_unit_spawn(
    trigger: On<Insert, Unit>,
    mut commands: Commands,
    unit_atlas: Res<UnitAtlasResource>,
    mut query: Query<(&Unit, &Faction, Has<UnitActive>)>,
) {
    let entity = trigger.entity;
    let Ok((unit, faction, has_active)) = query.get_mut(entity) else {
        warn!("Unit entity {:?} not found in query", entity);
        return;
    };

    log::info!(
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
                    animate_blinking_overlays,
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
    use awbrn_game::world::{
        CaptureProgress, CarriedBy, FogActive, FogOfWarMap, FriendlyFactions, GraphicalHp,
        UnitActive,
    };
    use awbrn_types::{GraphicalMovement, PlayerFaction};
    use bevy::asset::Assets;

    fn unit_render_test_app() -> App {
        let mut app = App::new();
        let mut atlas_assets = Assets::<crate::UiAtlasAsset>::default();
        let atlas_handle = atlas_assets.add(crate::UiAtlasAsset {
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
                    name: "Dive.png".to_string(),
                    x: 40,
                    y: 0,
                    width: 8,
                    height: 8,
                },
            ],
        });

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
        app.init_resource::<FogOfWarMap>();
        app.init_resource::<FogActive>();
        app.init_resource::<FriendlyFactions>();
        app.register_required_components::<Unit, UnitOverlayRegistry>()
            .add_observer(handle_unit_spawn)
            .add_systems(
                Update,
                (
                    project_unit_render_state,
                    sync_projected_unit_render_state
                        .before(crate::render::animation::animate_units),
                )
                    .chain(),
            );
        app
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
        }

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

        app.world_mut().entity_mut(unit).insert(GraphicalHp(9));
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

        app.world_mut().entity_mut(unit).insert(GraphicalHp(1));
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
    fn health_overlay_is_removed_at_full_health() {
        let mut app = unit_render_test_app();
        let unit = spawn_test_unit(&mut app, PlayerFaction::GreenEarth, true);
        app.update();

        app.world_mut().entity_mut(unit).insert(GraphicalHp(9));
        app.update();
        assert!(
            app.world()
                .entity(unit)
                .get::<UnitOverlayRegistry>()
                .unwrap()
                .overlay(OverlayKind::Health)
                .is_some()
        );

        app.world_mut().entity_mut(unit).insert(GraphicalHp(10));
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
    fn health_and_capturing_overlays_can_coexist() {
        let mut app = unit_render_test_app();
        let unit = spawn_test_unit(&mut app, PlayerFaction::GreenEarth, true);
        app.update();

        app.world_mut().entity_mut(unit).insert(GraphicalHp(5));
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
        let capturing = registry.overlay(OverlayKind::Capturing).unwrap();

        assert_ne!(health, capturing);
        assert_eq!(
            overlay_sprite_index(&app, health),
            atlas_index(&app, "Healthv2/5.png")
        );
        assert_eq!(
            overlay_sprite_index(&app, capturing),
            atlas_index(&app, "Capturing.png")
        );
    }

    #[test]
    fn health_and_capturing_overlays_are_tracked_when_spawned_together() {
        let mut app = unit_render_test_app();
        let unit = app
            .world_mut()
            .spawn((
                Unit(awbrn_types::Unit::Infantry),
                Faction(PlayerFaction::GreenEarth),
                UnitActive,
                GraphicalHp(5),
                CaptureProgress::new(10).unwrap(),
            ))
            .id();

        app.update();

        let registry = app
            .world()
            .entity(unit)
            .get::<UnitOverlayRegistry>()
            .unwrap();
        let health = registry.overlay(OverlayKind::Health).unwrap();
        let capturing = registry.overlay(OverlayKind::Capturing).unwrap();

        assert_ne!(health, capturing);
        assert_eq!(
            overlay_sprite_index(&app, health),
            atlas_index(&app, "Healthv2/5.png")
        );
        assert_eq!(
            overlay_sprite_index(&app, capturing),
            atlas_index(&app, "Capturing.png")
        );
    }

    #[test]
    fn cargo_overlay_tracks_has_cargo_relationship() {
        let mut app = unit_render_test_app();
        let transport = spawn_test_unit(&mut app, PlayerFaction::GreenEarth, true);
        let cargo = spawn_test_unit(&mut app, PlayerFaction::GreenEarth, true);
        app.update();

        app.world_mut()
            .entity_mut(cargo)
            .insert(CarriedBy(transport));
        app.update();

        let cargo_overlay = app
            .world()
            .entity(transport)
            .get::<UnitOverlayRegistry>()
            .unwrap()
            .overlay(OverlayKind::Cargo)
            .unwrap();
        assert_eq!(
            overlay_sprite_index(&app, cargo_overlay),
            atlas_index(&app, "HasCargo.png")
        );

        app.world_mut().entity_mut(cargo).remove::<CarriedBy>();
        app.update();
        assert!(
            app.world()
                .entity(transport)
                .get::<UnitOverlayRegistry>()
                .unwrap()
                .overlay(OverlayKind::Cargo)
                .is_none()
        );
    }
}
