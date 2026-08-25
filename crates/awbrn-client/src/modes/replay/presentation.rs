//! Replay and live presentation over typed AWVM observation transitions.
//!
//! Historical actions advance the recorded-outcome adapter. Both historical
//! and live presentation then consume only recipient-safe
//! `ObservedTransition` values.

use awbw_replay::turn_models::Action;
use awvm::semantic::{ObservedEvent, ObservedTransition, ObservedUnitRef, PlayerId};
use bevy::{log, prelude::*};

use crate::features::event_bus::{EventSink, NewDay as ExternalNewDay};
use crate::features::player_roster::{
    PlayerFunds, PlayerPowerMeters, PlayerRosterConfig, PlayerUnitCosts, active_power_meter,
    emit_player_roster_updated, player_roster_seed_from_replay, power_meters_from_observation,
};
use crate::modes::replay::navigation::PendingCourseArrows;
use crate::render::animation::UnitPathAnimation;
use awbrn_bevy::replay::{
    AwbwUnitId, NewDay, ReplayState, ReplayTerrainKnowledge, ReplayViewpoint, TransitionApplyError,
    apply_observed_transition, apply_observed_transitions,
};
use awbrn_bevy::world::{BoardIndex, CarriedBy, StrongIdMap};
use awbrn_types::UnitExt;

/// Historical replay source for the typed presentation boundary.
///
/// `RecordedAdapter` applies archived graphical outcomes and never executes an
/// AWVM command or reconstructs random tokens.
#[derive(Debug, Resource)]
pub struct ReplayTransitionSource {
    timeline: crate::replay_archive::ReplayTimeline,
    initial_terrain_knowledge: Option<ReplayTerrainKnowledge>,
}

impl ReplayTransitionSource {
    pub fn new(timeline: impl Into<crate::replay_archive::ReplayTimeline>) -> Self {
        Self {
            timeline: timeline.into(),
            initial_terrain_knowledge: None,
        }
    }

    pub(crate) fn set_initial_terrain_knowledge(&mut self, knowledge: ReplayTerrainKnowledge) {
        self.initial_terrain_knowledge = Some(knowledge);
    }

    /// Each player's view of the archive before any action is applied.
    ///
    /// Bootstrap spawns units from the replay's own roster, so the ECS is
    /// already populated; these projections are what a viewpoint switch made
    /// before the first action selects between.
    pub fn initial_observations(&self) -> Result<Vec<awvm::semantic::Observation>, String> {
        self.timeline.initial_observations()
    }

    fn rebuild_to(
        &mut self,
        replay: &crate::replay_archive::ReplayArchive,
        target_index: usize,
    ) -> Result<Vec<ObservedTransition>, String> {
        self.timeline.rebuild(replay, target_index)
    }
}

/// Terminal marker for a replay whose typed source can no longer be trusted.
///
/// A failed advance or projection desynchronizes the adapter from the archived
/// action stream, so every later action would compound the divergence. The
/// marker halts advancement instead of logging once per remaining action.
#[derive(Resource, Debug)]
pub struct ReplayTransitionFailed;

#[derive(Resource, Debug, Default)]
pub struct ReplayAdvanceLock {
    active_entity: Option<Entity>,
    deferred_transitions: Option<DeferredTransitions>,
}

impl ReplayAdvanceLock {
    pub fn is_active(&self) -> bool {
        self.active_entity.is_some()
    }

    fn activate_typed(&mut self, entity: Entity, transitions: Vec<ObservedTransition>) {
        self.active_entity = Some(entity);
        self.deferred_transitions = Some(DeferredTransitions::Complete(transitions));
    }

    fn activate_recipient(&mut self, entity: Entity, transition: ObservedTransition) {
        self.active_entity = Some(entity);
        self.deferred_transitions = Some(DeferredTransitions::Recipient(Box::new(transition)));
    }

    pub fn active_entity(&self) -> Option<Entity> {
        self.active_entity
    }

    pub fn release_for(&mut self, entity: Entity) -> Option<ReplayAnimationFollowup> {
        if self.active_entity != Some(entity) {
            return None;
        }

        self.active_entity = None;
        Some(ReplayAnimationFollowup {
            transitions: self.deferred_transitions.take(),
        })
    }
}

#[derive(Debug)]
pub struct ReplayAnimationFollowup {
    pub transitions: Option<DeferredTransitions>,
}

#[derive(Debug)]
pub struct ReplayFollowupCommand {
    pub transitions: Option<DeferredTransitions>,
}

impl Command for ReplayFollowupCommand {
    type Out = ();

    fn apply(self, world: &mut World) {
        if let Some(transitions) = &self.transitions {
            // The apply paths already emit the roster snapshot.
            apply_deferred_transitions(transitions, world);
            return;
        }
        emit_player_roster_updated(world);
    }
}

#[derive(Debug)]
pub enum DeferredTransitions {
    Complete(Vec<ObservedTransition>),
    Recipient(Box<ObservedTransition>),
}

/// Apply one live server transition through the same typed presentation and
/// animation path used by archived replay playback.
#[derive(Debug)]
pub struct LiveTransitionCommand {
    pub transition: ObservedTransition,
}

impl Command for LiveTransitionCommand {
    type Out = ();

    fn apply(self, world: &mut World) {
        if start_typed_movement_animation(
            std::slice::from_ref(&self.transition),
            DeferredTransitions::Recipient(Box::new(self.transition.clone())),
            world,
        ) {
            return;
        }
        apply_recipient_transition(&self.transition, world);
    }
}

/// A custom Command for processing replay turn actions.
#[derive(Debug)]
pub struct ReplayTurnCommand {
    pub index: usize,
}

impl Command for ReplayTurnCommand {
    type Out = ();

    fn apply(self, world: &mut World) {
        if !world.contains_resource::<ReplayTransitionSource>() {
            log::error!("Cannot advance replay without a typed transition source");
            return;
        }
        self.apply_typed(world);
    }
}

/// Restore one stable replay action boundary without presenting the actions
/// used to calculate it. The adapter is rebuilt in local memory first; only
/// the final projections are committed to the ECS.
#[derive(Debug)]
pub struct ReplayRewindCommand {
    pub target_index: u32,
}

impl Command for ReplayRewindCommand {
    type Out = ();

    fn apply(self, world: &mut World) {
        if world
            .get_resource::<ReplayAdvanceLock>()
            .is_some_and(ReplayAdvanceLock::is_active)
        {
            return;
        }

        let Some(replay) = world.get_resource::<crate::loading::LoadedReplay>() else {
            log::error!("Cannot rewind replay without a loaded replay");
            return;
        };
        let target_index = usize::try_from(self.target_index)
            .unwrap_or(usize::MAX)
            .min(replay.0.len());

        if !world.contains_resource::<ReplayTransitionSource>() {
            log::error!("Cannot rewind replay without a typed transition source");
            return;
        }
        let rebuilt = world.resource_scope(|world, mut source: Mut<ReplayTransitionSource>| {
            let replay = &world.resource::<crate::loading::LoadedReplay>().0;
            source.rebuild_to(replay, target_index)
        });
        let transitions = match rebuilt {
            Ok(rebuilt) => rebuilt,
            Err(error) => {
                log::error!("{error}");
                return;
            }
        };

        // Reset action-derived roster caches before applying the final AWVM
        // projections. All of this happens in one deferred command, between
        // rendered frames, so no intermediate replay state is observable.
        reset_player_roster_for_rewind(target_index, world);
        if let Some(knowledge) = world
            .resource::<ReplayTransitionSource>()
            .initial_terrain_knowledge
            .clone()
        {
            world.insert_resource(knowledge);
        }
        if let Err(error) = apply_typed_transitions(&transitions, world, true) {
            log::error!("Could not apply typed replay presentation transition: {error}");
            return;
        }
        world.resource_mut::<ReplayState>().next_action_index = target_index as u32;
        world.remove_resource::<ReplayTransitionFailed>();
    }
}

impl ReplayTurnCommand {
    fn apply_typed(self, world: &mut World) {
        if world.contains_resource::<ReplayTransitionFailed>() {
            return;
        }

        let observed = {
            world.resource_scope(|world, mut source: Mut<ReplayTransitionSource>| {
                let replay = &world.resource::<crate::loading::LoadedReplay>().0;
                source.timeline.advance(replay, self.index)
            })
        };
        let observed = match observed {
            Ok(observed) => observed,
            Err(error) => {
                log::error!("{error}");
                world.insert_resource(ReplayTransitionFailed);
                return;
            }
        };

        let cost_updates = world
            .resource::<crate::loading::LoadedReplay>()
            .0
            .awbw()
            .and_then(|replay| replay.turns.get(self.index))
            .map(collect_player_roster_unit_cost_updates)
            .unwrap_or_default();
        if let Some(mut unit_costs) = world.get_resource_mut::<PlayerUnitCosts>() {
            for (unit_id, cost) in cost_updates {
                unit_costs.set(unit_id, cost);
            }
        }

        if start_typed_movement_animation(
            &observed,
            DeferredTransitions::Complete(observed.clone()),
            world,
        ) {
            return;
        }

        if let Err(error) = apply_typed_transitions(&observed, world, true) {
            log::error!("Could not apply typed replay presentation transition: {error}");
            world.insert_resource(ReplayTransitionFailed);
        }
    }
}

fn apply_deferred_transitions(transitions: &DeferredTransitions, world: &mut World) {
    match transitions {
        DeferredTransitions::Complete(transitions) => {
            if let Err(error) = apply_typed_transitions(transitions, world, true) {
                log::error!("Could not apply typed replay presentation transition: {error}");
                world.insert_resource(ReplayTransitionFailed);
            }
        }
        DeferredTransitions::Recipient(transition) => {
            apply_recipient_transition(transition, world);
        }
    }
}

fn apply_typed_transitions(
    transitions: &[ObservedTransition],
    world: &mut World,
    emit_new_day: bool,
) -> Result<(), TransitionApplyError> {
    let previous_day = world
        .get_resource::<ReplayState>()
        .map_or(1, |state| state.day);
    apply_observed_transitions(world, transitions)?;
    sync_player_roster_from_transition(transitions, world);
    let current_day = world
        .get_resource::<ReplayState>()
        .map_or(previous_day, |state| state.day);
    if emit_new_day && current_day != previous_day {
        world.trigger(NewDay { day: current_day });
    }
    emit_player_roster_updated(world);
    Ok(())
}

fn reset_player_roster_for_rewind(target_index: usize, world: &mut World) {
    let rebuilt = {
        let replay = &world.resource::<crate::loading::LoadedReplay>().0;
        let Some(replay) = replay.awbw() else {
            return;
        };
        let Some((config, funds, mut unit_costs)) = player_roster_seed_from_replay(replay) else {
            return;
        };
        for action in replay.turns.iter().take(target_index) {
            for (unit_id, cost) in collect_player_roster_unit_cost_updates(action) {
                unit_costs.set(unit_id, cost);
            }
        }
        (config, funds, unit_costs)
    };
    let (config, funds, unit_costs) = rebuilt;
    world.insert_resource(config);
    world.insert_resource(funds);
    world.insert_resource(unit_costs);
    let power_meters = (target_index == 0)
        .then(|| {
            world
                .resource::<ReplayTransitionSource>()
                .initial_observations()
                .ok()
                .and_then(|observations| observations.first().map(power_meters_from_observation))
        })
        .flatten()
        .unwrap_or_default();
    world.insert_resource(power_meters);
}

fn apply_recipient_transition(transition: &ObservedTransition, world: &mut World) {
    let previous_day = world
        .get_resource::<ReplayState>()
        .map_or(1, |state| state.day);
    if let Err(error) = apply_observed_transition(world, transition) {
        log::error!("Could not apply live typed presentation transition: {error}");
        return;
    }
    sync_player_roster_from_transition(std::slice::from_ref(transition), world);
    let current_day = world
        .get_resource::<ReplayState>()
        .map_or(previous_day, |state| state.day);
    if current_day != previous_day {
        world.trigger(NewDay { day: current_day });
    }
    emit_player_roster_updated(world);
}

fn sync_player_roster_from_transition(transitions: &[ObservedTransition], world: &mut World) {
    let updates = transitions
        .iter()
        .flat_map(|transition| transition.post.players.iter())
        .filter_map(|player| {
            let awvm::semantic::ObservedPlayer::Private { id, funds, .. } = player else {
                return None;
            };
            Some((parse_player_id(id)?, u32::try_from(*funds).ok()?))
        })
        .collect::<Vec<_>>();
    if let Some(mut player_funds) = world.get_resource_mut::<PlayerFunds>() {
        for (player, funds) in updates {
            player_funds.set(player, funds);
        }
    }

    let statuses = transitions
        .first()
        .into_iter()
        .flat_map(|transition| transition.post.players.iter())
        .filter_map(|player| match player {
            awvm::semantic::ObservedPlayer::Private { id, status, .. }
            | awvm::semantic::ObservedPlayer::Public { id, status, .. } => {
                Some((parse_player_id(id)?, *status))
            }
        })
        .collect::<Vec<_>>();
    // Power meter inputs are public in AWVM, so every transition reports them
    // for every player and the first one is authoritative for all of them.
    let power_meters = transitions
        .first()
        .into_iter()
        .flat_map(|transition| transition.post.players.iter())
        .filter_map(|player| {
            let id = match player {
                awvm::semantic::ObservedPlayer::Private { id, .. }
                | awvm::semantic::ObservedPlayer::Public { id, .. } => id,
            };
            Some((parse_player_id(id)?, active_power_meter(player)?))
        })
        .collect::<Vec<_>>();
    if let Some(mut meters) = world.get_resource_mut::<PlayerPowerMeters>() {
        for (player, meter) in power_meters {
            meters.set(player, meter);
        }
    }

    if let Some(mut config) = world.get_resource_mut::<PlayerRosterConfig>() {
        for (player, status) in statuses {
            if let Some(entry) = config
                .players
                .iter_mut()
                .find(|entry| entry.player_id == player)
            {
                entry.eliminated = status != awvm::semantic::PlayerStatus::Active;
            }
        }
    }
}

fn collect_player_roster_unit_cost_updates(action: &Action) -> Vec<(AwbwUnitId, u32)> {
    match action {
        Action::Build { new_unit, .. } => new_unit
            .values()
            .find_map(|unit| unit.get_value())
            .map(|unit| {
                vec![(
                    AwbwUnitId(unit.units_id),
                    unit.units_cost.unwrap_or(unit.units_name.base_cost()),
                )]
            })
            .unwrap_or_default(),
        Action::Power(power_action) => power_action
            .unit_add
            .as_ref()
            .into_iter()
            .flat_map(|groups| groups.values())
            .flat_map(|group| {
                group
                    .units
                    .iter()
                    .map(|unit| (AwbwUnitId(unit.units_id), group.unit_name.base_cost()))
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Turn a projected movement path into course-arrow tiles.
///
/// `ObservedEvent::UnitMoved` already reports only the stretch the recipient
/// could watch (`spec/semantics/fog.md`), so every position that reaches here
/// is one the viewer is entitled to see. `unit_visible` stays on the tile
/// because the archived AWBW path in `replay_path_tiles` still carries it.
fn typed_path_for_current_view(
    path: &[awbrn_map::Pos],
) -> Vec<crate::modes::replay::navigation::ReplayPathTile> {
    use crate::modes::replay::navigation::ReplayPathTile;

    path.iter()
        .map(|position| ReplayPathTile {
            position: *position,
            unit_visible: true,
        })
        .collect()
}

fn start_typed_movement_animation(
    transitions: &[ObservedTransition],
    deferred: DeferredTransitions,
    world: &mut World,
) -> bool {
    let Some((unit, origin, path)) = movement_for_current_view(transitions, world) else {
        return false;
    };
    if path.len() < 2 {
        return false;
    }
    let Some(entity) = entity_for_observed_unit(unit, origin, world) else {
        return false;
    };
    let idle_flip_x = world
        .entity(entity)
        .get::<Sprite>()
        .map(|sprite| sprite.flip_x)
        .unwrap_or(false);
    let Some(animation) = UnitPathAnimation::new(path.clone(), idle_flip_x) else {
        return false;
    };
    // A move the viewer could watch nowhere reports an empty path and was
    // already rejected above, so the state change applies without motion.
    let arrow_path = typed_path_for_current_view(&path);
    world
        .entity_mut(entity)
        .insert((animation, PendingCourseArrows { path: arrow_path }));
    match deferred {
        DeferredTransitions::Complete(transitions) => world
            .resource_mut::<ReplayAdvanceLock>()
            .activate_typed(entity, transitions),
        DeferredTransitions::Recipient(transition) => world
            .resource_mut::<ReplayAdvanceLock>()
            .activate_recipient(entity, *transition),
    }
    true
}

fn movement_for_current_view<'a>(
    transitions: &'a [ObservedTransition],
    world: &World,
) -> Option<(ObservedUnitRef, awbrn_map::Pos, Vec<awbrn_map::Pos>)> {
    let selected = if transitions.len() == 1 {
        transitions.first()
    } else {
        match world.get_resource::<ReplayViewpoint>() {
            Some(ReplayViewpoint::Player(player)) => transition_for_player(transitions, *player),
            Some(ReplayViewpoint::ActivePlayer) => world
                .get_resource::<ReplayState>()
                .and_then(|state| state.active_player_id)
                .and_then(|player| transition_for_player(transitions, player)),
            Some(ReplayViewpoint::Spectator) | None => None,
        }
    };
    let mut candidates: Box<dyn Iterator<Item = &'a ObservedEvent> + 'a> =
        if let Some(transition) = selected {
            Box::new(transition.events.iter())
        } else {
            Box::new(
                transitions
                    .iter()
                    .flat_map(|transition| transition.events.iter())
                    .filter(|event| {
                        matches!(
                            event,
                            ObservedEvent::UnitMoved {
                                unit: ObservedUnitRef::Friendly { .. },
                                ..
                            }
                        )
                    }),
            )
        };
    candidates.find_map(|event| match event {
        ObservedEvent::UnitMoved {
            unit, from, path, ..
        } => Some((*unit, *from, path.to_vec())),
        ObservedEvent::UnitAppeared { .. }
        | ObservedEvent::UnitDisappeared { .. }
        | ObservedEvent::MovementStopped { .. }
        | ObservedEvent::CombatEngaged { .. }
        | ObservedEvent::UnitChanged { .. }
        | ObservedEvent::UnitRemoved { .. }
        | ObservedEvent::TileChanged { .. }
        | ObservedEvent::PlayerChanged { .. }
        | ObservedEvent::AreaStrikeResolved { .. }
        | ObservedEvent::PublicEvent { .. } => None,
    })
}

fn transition_for_player(
    transitions: &[ObservedTransition],
    player: awbrn_types::AwbwGamePlayerId,
) -> Option<&ObservedTransition> {
    transitions
        .iter()
        .find(|transition| parse_player_id(&transition.post.recipient) == Some(player))
}

fn parse_player_id(id: &PlayerId) -> Option<awbrn_types::AwbwGamePlayerId> {
    id.as_str()
        .parse()
        .ok()
        .map(awbrn_types::AwbwGamePlayerId::new)
}

fn entity_for_observed_unit(
    unit: ObservedUnitRef,
    origin: awbrn_map::Pos,
    world: &World,
) -> Option<Entity> {
    match unit {
        ObservedUnitRef::Friendly { unit } => {
            let id = awbrn_types::AwbwUnitId::new(unit.get());
            world
                .resource::<StrongIdMap<AwbwUnitId>>()
                .get(&AwbwUnitId(id))
        }
        ObservedUnitRef::Enemy { .. } => world
            .resource::<BoardIndex>()
            .unit_entity(origin)
            .ok()
            .flatten(),
    }
}

/// Observer: when `CarriedBy` is added to an entity, hide it visually.
pub(crate) fn on_carried_by_add(trigger: On<Insert, CarriedBy>, mut commands: Commands) {
    commands.entity(trigger.entity).insert(Visibility::Hidden);
}

/// Observer: when `CarriedBy` is removed, keep the entity hidden until the
/// projection pass decides whether it is visible.
pub(crate) fn on_carried_by_remove(trigger: On<Remove, CarriedBy>, mut commands: Commands) {
    commands.entity(trigger.entity).insert(Visibility::Hidden);
}

/// Forward semantic day changes to registered client sinks.
pub(crate) fn on_new_day(trigger: On<NewDay>, sink: If<Res<EventSink<ExternalNewDay>>>) {
    sink.emit(ExternalNewDay { day: trigger.day });
}
