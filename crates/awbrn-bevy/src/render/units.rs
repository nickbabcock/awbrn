use crate::core::{
    Capturing, Faction, GraphicalHp, HasCargo, INACTIVE_UNIT_COLOR, Unit, UnitActive,
};
use crate::render::animation::{
    Animation, UnitPathAnimation, UnitVisualState, restore_unit_visual_state,
};
use crate::render::{UiAtlas, UnitAtlasResource};
use awbrn_core::get_unit_animation_frames;
use bevy::ecs::query::QueryData;
use bevy::sprite::Anchor;
use bevy::{log, prelude::*};

/// Component to track the capturing indicator sprite child entity
#[derive(Component, Debug)]
pub struct CapturingIndicator(pub Entity);

/// Component to track the cargo indicator sprite child entity
#[derive(Component, Debug)]
pub struct CargoIndicator(pub Entity);

/// Component to track the health indicator sprite child entity
#[derive(Component, Debug)]
pub struct HealthIndicator(pub Entity);

#[derive(QueryData)]
#[query_data(mutable)]
struct IdleUnitVisualQuery {
    entity: Entity,
    unit: &'static Unit,
    faction: &'static Faction,
    sprite: &'static mut Sprite,
    animation: Option<&'static mut Animation>,
    has_active: Has<UnitActive>,
}

type IdleUnitVisualFilter = (
    Without<UnitPathAnimation>,
    Or<(Changed<Unit>, Changed<Faction>, Changed<UnitActive>)>,
);

/// Observer that triggers when Capturing component is removed - cleans up the indicator
pub(crate) fn on_capturing_remove(
    trigger: On<Remove, Capturing>,
    mut commands: Commands,
    query: Query<&CapturingIndicator>,
) {
    let entity = trigger.entity;

    if let Ok(indicator) = query.get(entity) {
        commands.entity(indicator.0).try_despawn();
    }
}

/// Observer that triggers when HasCargo component is removed - cleans up the indicator
pub(crate) fn on_cargo_remove(
    trigger: On<Remove, HasCargo>,
    mut commands: Commands,
    query: Query<&CargoIndicator>,
) {
    let entity = trigger.entity;

    if let Ok(indicator) = query.get(entity) {
        commands.entity(indicator.0).try_despawn();
    }
}

/// Observer that triggers when Capturing component is inserted - spawns the indicator
pub(crate) fn on_capturing_insert(
    trigger: On<Insert, Capturing>,
    mut commands: Commands,
    ui_atlas: UiAtlas,
) {
    let entity = trigger.entity;

    let indicator_entity = commands
        .spawn((ui_atlas.capturing_sprite(), ChildOf(entity)))
        .id();

    commands
        .entity(entity)
        .insert(CapturingIndicator(indicator_entity));
}

/// Observer that triggers when HasCargo component is inserted - spawns the indicator
pub(crate) fn on_cargo_insert(
    trigger: On<Insert, HasCargo>,
    mut commands: Commands,
    ui_atlas: UiAtlas,
) {
    let entity = trigger.entity;

    let indicator_entity = commands
        .spawn((ui_atlas.cargo_sprite(), ChildOf(entity)))
        .id();

    commands
        .entity(entity)
        .insert(CargoIndicator(indicator_entity));
}

/// Observer that triggers when GraphicalHp component is inserted
pub(crate) fn on_health_insert(
    trigger: On<Insert, GraphicalHp>,
    mut commands: Commands,
    ui_atlas: UiAtlas,
    query: Query<&GraphicalHp>,
) {
    let entity = trigger.entity;

    let Ok(hp) = query.get(entity) else {
        log::warn!("GraphicalHp component not found for entity {:?}", entity);
        return;
    };

    if hp.is_full_health() || hp.is_destroyed() {
        return;
    }

    let hp_value = hp.value();
    let sprite_name = format!("Healthv2/{}.png", hp_value);

    let indicator_entity = commands
        .spawn((ui_atlas.health_sprite(&sprite_name), ChildOf(entity)))
        .id();

    commands
        .entity(entity)
        .insert(HealthIndicator(indicator_entity));

    log::info!(
        "Spawned health indicator for entity {:?} with HP {}",
        entity,
        hp_value
    );
}

/// Observer that triggers when GraphicalHp component is removed
pub(crate) fn on_health_remove(
    trigger: On<Remove, GraphicalHp>,
    mut commands: Commands,
    query: Query<&HealthIndicator>,
) {
    let entity = trigger.entity;

    if let Ok(indicator) = query.get(entity) {
        commands.entity(indicator.0).try_despawn();
        commands.entity(entity).try_remove::<HealthIndicator>();
    }
}

/// Observer that triggers when UnitActive component is removed - applies grey filter and freezes animation
pub(crate) fn on_unit_active_remove(
    trigger: On<Remove, UnitActive>,
    mut commands: Commands,
    mut query: Query<&mut Sprite>,
) {
    let entity = trigger.entity;

    let Ok(mut sprite) = query.get_mut(entity) else {
        return;
    };

    sprite.color = INACTIVE_UNIT_COLOR;
    commands.entity(entity).try_remove::<Animation>();
}

/// Observer that triggers when UnitActive component is inserted - restores active coloring.
pub(crate) fn on_unit_active_insert(
    trigger: On<Insert, UnitActive>,
    mut query: Query<&mut Sprite>,
) {
    let entity = trigger.entity;

    let Ok(mut sprite) = query.get_mut(entity) else {
        return;
    };

    sprite.color = Color::WHITE;
}

/// Recomputes idle unit animation when the derived inputs change.
fn sync_unit_animation(
    mut commands: Commands,
    mut query: Query<IdleUnitVisualQuery, IdleUnitVisualFilter>,
) {
    for mut unit_visual in &mut query {
        let visual_state = UnitVisualState {
            unit: *unit_visual.unit,
            faction: *unit_visual.faction,
            flip_x: unit_visual.sprite.flip_x,
        };
        restore_unit_visual_state(
            &mut commands,
            unit_visual.entity,
            &mut unit_visual.sprite,
            unit_visual.animation,
            visual_state,
            unit_visual.has_active,
        );
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
        get_unit_animation_frames(awbrn_core::GraphicalMovement::Idle, unit.0, faction.0);

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

    commands.entity(entity).insert((sprite, Anchor::default()));
}

pub struct UnitRenderingPlugin;

impl Plugin for UnitRenderingPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_capturing_remove)
            .add_observer(on_cargo_remove)
            .add_observer(on_capturing_insert)
            .add_observer(on_cargo_insert)
            .add_observer(on_health_insert)
            .add_observer(on_health_remove)
            .add_observer(on_unit_active_remove)
            .add_observer(on_unit_active_insert)
            .add_observer(handle_unit_spawn)
            .add_systems(
                Update,
                sync_unit_animation
                    .after(crate::features::navigation::animate_unit_paths)
                    .before(crate::render::animation::animate_units)
                    .run_if(in_state(crate::core::AppState::InGame)),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use awbrn_core::{GraphicalMovement, PlayerFaction};

    fn unit_render_test_app() -> App {
        let mut app = App::new();
        app.insert_resource(UnitAtlasResource {
            texture: Handle::default(),
            layout: Handle::default(),
        });
        app.add_observer(on_unit_active_remove)
            .add_observer(on_unit_active_insert)
            .add_observer(handle_unit_spawn)
            .add_systems(Update, sync_unit_animation);
        app
    }

    fn spawn_test_unit(app: &mut App, faction: PlayerFaction, active: bool) -> Entity {
        let entity = app
            .world_mut()
            .spawn((Unit(awbrn_core::Unit::Infantry), Faction(faction)))
            .id();

        if active {
            app.world_mut().entity_mut(entity).insert(UnitActive);
        }

        entity
    }

    #[test]
    fn active_unit_spawn_inserts_idle_animation() {
        let mut app = unit_render_test_app();
        let entity = spawn_test_unit(&mut app, PlayerFaction::GreenEarth, true);

        app.update();

        let expected = get_unit_animation_frames(
            GraphicalMovement::Idle,
            awbrn_core::Unit::Infantry,
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
            awbrn_core::Unit::Infantry,
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
            awbrn_core::Unit::Infantry,
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
            awbrn_core::Unit::Infantry,
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
}
