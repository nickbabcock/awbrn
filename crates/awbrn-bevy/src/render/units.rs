use crate::core::{
    Capturing, Faction, GraphicalHp, HasCargo, INACTIVE_UNIT_COLOR, Unit, UnitActive,
};
use crate::render::animation::Animation;
use crate::render::{UiAtlas, UnitAtlasResource};
use awbrn_core::get_unit_animation_frames;
use bevy::sprite::Anchor;
use bevy::{log, prelude::*};
use std::time::Duration;

/// Component to track the capturing indicator sprite child entity
#[derive(Component, Debug)]
pub struct CapturingIndicator(pub Entity);

/// Component to track the cargo indicator sprite child entity
#[derive(Component, Debug)]
pub struct CargoIndicator(pub Entity);

/// Component to track the health indicator sprite child entity
#[derive(Component, Debug)]
pub struct HealthIndicator(pub Entity);

/// Observer that triggers when Capturing component is removed - cleans up the indicator
pub(crate) fn on_capturing_remove(
    trigger: On<Remove, Capturing>,
    mut commands: Commands,
    query: Query<&CapturingIndicator>,
) {
    let entity = trigger.entity;

    if let Ok(indicator) = query.get(entity) {
        commands.entity(indicator.0).despawn();
        log::info!("Removed capturing indicator from entity {:?}", entity);
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
        commands.entity(indicator.0).despawn();
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

    log::info!("Spawned capturing indicator for entity {:?}", entity);
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

    log::info!("Spawned cargo indicator for entity {:?}", entity);
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

    if hp.is_full_health() {
        return;
    }

    if hp.is_destroyed() {
        log::warn!("Unit {:?} has 0 HP", entity);
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
        commands.entity(indicator.0).despawn();
        commands.entity(entity).remove::<HealthIndicator>();
        log::info!("Removed health indicator from entity {:?}", entity);
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
    commands.entity(entity).remove::<Animation>();
}

/// Observer that triggers when UnitActive component is inserted - restores animation and color
pub(crate) fn on_unit_active_insert(
    trigger: On<Insert, UnitActive>,
    mut commands: Commands,
    mut query: Query<(&Unit, &Faction, &mut Sprite)>,
) {
    let entity = trigger.entity;

    let Ok((unit, faction, mut sprite)) = query.get_mut(entity) else {
        return;
    };

    sprite.color = Color::WHITE;

    let animation_frames =
        get_unit_animation_frames(awbrn_core::GraphicalMovement::Idle, unit.0, faction.0);

    let frame_durations = animation_frames.raw();
    let animation = Animation {
        start_index: animation_frames.start_index(),
        frame_durations,
        current_frame: 0,
        frame_timer: Timer::new(
            Duration::from_millis(frame_durations[0] as u64),
            TimerMode::Once,
        ),
    };

    commands.entity(entity).insert(animation);
}

/// System to automatically remove HasCargo component when it becomes empty.
pub(crate) fn cleanup_empty_cargo(
    mut commands: Commands,
    query: Query<(Entity, &HasCargo), Changed<HasCargo>>,
) {
    for (entity, has_cargo) in query.iter() {
        if has_cargo.is_empty() {
            commands.entity(entity).remove::<HasCargo>();
            log::info!(
                "Transport entity {:?} cargo is empty, removing HasCargo component",
                entity
            );
        }
    }
}

/// System to update health indicator when GraphicalHp value changes
pub(crate) fn update_health_indicator(
    mut commands: Commands,
    ui_atlas: UiAtlas,
    query: Query<(Entity, &GraphicalHp, Option<&HealthIndicator>), Changed<GraphicalHp>>,
) {
    for (entity, hp, indicator) in query.iter() {
        if let Some(indicator) = indicator {
            commands.entity(indicator.0).despawn();
        }

        if hp.is_full_health() {
            if indicator.is_some() {
                commands.entity(entity).remove::<HealthIndicator>();
                log::info!(
                    "Unit {:?} restored to full health, removing indicator",
                    entity
                );
            }
            continue;
        }

        if hp.is_destroyed() {
            if indicator.is_some() {
                commands.entity(entity).remove::<HealthIndicator>();
            }
            log::warn!("Unit {:?} destroyed (0 HP)", entity);
            continue;
        }

        let hp_value = hp.value();
        let sprite_name = format!("Healthv2/{}.png", hp_value);

        let new_indicator = commands
            .spawn((ui_atlas.health_sprite(&sprite_name), ChildOf(entity)))
            .id();

        commands
            .entity(entity)
            .insert(HealthIndicator(new_indicator));

        log::info!(
            "Updated health indicator for entity {:?} to HP {}",
            entity,
            hp_value
        );
    }
}

/// System to update unit sprite when Faction changes
pub(crate) fn update_unit_on_faction_change(
    mut query: Query<(&Unit, &Faction, &mut Sprite, &mut Animation), Changed<Faction>>,
) {
    for (unit, faction, mut sprite, mut animation) in query.iter_mut() {
        let animation_frames =
            get_unit_animation_frames(awbrn_core::GraphicalMovement::Idle, unit.0, faction.0);

        animation.start_index = animation_frames.start_index();
        animation.frame_durations = animation_frames.raw();
        animation.current_frame = 0;
        animation.frame_timer = Timer::new(
            Duration::from_millis(animation_frames.raw()[0] as u64),
            TimerMode::Once,
        );

        if let Some(atlas) = &mut sprite.texture_atlas {
            atlas.index = animation_frames.start_index() as usize;
        }

        log::info!(
            "Updated unit {:?} sprite to faction {:?}",
            unit.0,
            faction.0
        );
    }
}

/// System to update unit sprite when Unit type changes
pub(crate) fn update_unit_on_type_change(
    mut query: Query<(&Unit, &Faction, &mut Sprite, &mut Animation), Changed<Unit>>,
) {
    for (unit, faction, mut sprite, mut animation) in query.iter_mut() {
        let animation_frames =
            get_unit_animation_frames(awbrn_core::GraphicalMovement::Idle, unit.0, faction.0);

        animation.start_index = animation_frames.start_index();
        animation.frame_durations = animation_frames.raw();
        animation.current_frame = 0;
        animation.frame_timer = Timer::new(
            Duration::from_millis(animation_frames.raw()[0] as u64),
            TimerMode::Once,
        );

        if let Some(atlas) = &mut sprite.texture_atlas {
            atlas.index = animation_frames.start_index() as usize;
        }

        log::info!(
            "Updated sprite to unit type {:?} for faction {:?}",
            unit.0,
            faction.0
        );
    }
}

/// Observer that handles unit spawning - creates sprite and animation
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

    let (color, should_animate) = if has_active {
        (Color::WHITE, true)
    } else {
        (INACTIVE_UNIT_COLOR, false)
    };

    let mut sprite = Sprite::from_atlas_image(
        unit_atlas.texture.clone(),
        TextureAtlas {
            layout: unit_atlas.layout.clone(),
            index: animation_frames.start_index() as usize,
        },
    );
    sprite.color = color;

    let mut entity_commands = commands.entity(entity);
    entity_commands.insert((sprite, Anchor::default()));

    if should_animate {
        let frame_durations = animation_frames.raw();
        let animation = Animation {
            start_index: animation_frames.start_index(),
            frame_durations,
            current_frame: 0,
            frame_timer: Timer::new(
                Duration::from_millis(frame_durations[0] as u64),
                TimerMode::Once,
            ),
        };
        entity_commands.insert(animation);
    }
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
                (
                    cleanup_empty_cargo,
                    update_health_indicator,
                    update_unit_on_faction_change,
                    update_unit_on_type_change,
                )
                    .run_if(in_state(crate::core::AppState::InGame)),
            );
    }
}
