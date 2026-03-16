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

/// Observer that triggers when Faction is inserted - updates sprite and animation
pub(crate) fn on_faction_insert(
    trigger: On<Insert, Faction>,
    mut query: Query<(&Unit, &Faction, &mut Sprite, Option<&mut Animation>)>,
) {
    let entity = trigger.entity;
    let Ok((unit, faction, mut sprite, animation)) = query.get_mut(entity) else {
        return;
    };

    let animation_frames =
        get_unit_animation_frames(awbrn_core::GraphicalMovement::Idle, unit.0, faction.0);

    if let Some(atlas) = &mut sprite.texture_atlas {
        atlas.index = animation_frames.start_index() as usize;
    }

    if let Some(mut animation) = animation {
        animation.start_index = animation_frames.start_index();
        animation.frame_durations = animation_frames.raw();
        animation.current_frame = 0;
        animation.frame_timer = Timer::new(
            Duration::from_millis(animation_frames.raw()[0] as u64),
            TimerMode::Once,
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
            .add_observer(on_faction_insert);
    }
}
