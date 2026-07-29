//! Combat: choosing a target, resolving the exchange, and its side effects.
//!
//! Normative source:
//! * `spec/semantics/combat.md`

use super::ReducerError as ExecuteError;
use super::*;
use crate::combat::{self, Hit, Side};
use crate::commander::{self, CombatContext, Combatant, Strike};
use crate::event::{AttackTarget, Event};
use crate::random::Luck;
use crate::ruleset::{self, FireMode, TerrainTrait};
use crate::semantic::{
    AwbwVisibility, Concealment, KnownReason, Location, PlayerId, Pos, PowerState, State,
    TerrainId, Unit, UnitAction, UnitId, UnitKindId, Visibility,
};
use crate::violation::{Action, Violation};
use std::collections::HashSet;
use std::sync::LazyLock;

/// Commander combat predicates take a capability set per combatant. No path
/// through this module supplies one, so every combatant shares this empty set
/// instead of allocating one per strike.
static NO_CAPABILITIES: LazyLock<HashSet<String>> = LazyLock::new(HashSet::new);

/// One side of an engagement: a unit and the tile the strike is scored from.
///
/// The position is carried rather than read back off the unit because
/// move-and-attack scores the initiating strike from the destination it just
/// resolved.
#[derive(Clone, Copy)]
struct Fighter<'a> {
    unit: &'a Unit,
    position: Pos,
}

fn is_property(terrain: TerrainId) -> bool {
    ruleset::terrain_has(terrain, TerrainTrait::Capturable)
}

/// The board- and treasury-wide values a commander's combat rules read, for
/// `owner` firing from or standing on `position`.
fn combat_context(state: &State, owner: &PlayerId, position: Pos) -> CombatContext {
    let mut tower_count = 0_i64;
    let mut owned_properties = 0_u64;
    for tile in state.board.tiles() {
        if !tile.owner.player().is_some_and(|value| value == owner) {
            continue;
        }
        if ruleset::terrain_has(tile.terrain, TerrainTrait::CommunicationBonus) {
            tower_count += 1;
        }
        if is_property(tile.terrain) {
            owned_properties += 1;
        }
    }
    CombatContext {
        tower_count,
        funds: state.find_player(owner).map_or(0, |player| player.funds),
        owned_properties,
        base_terrain_stars: i64::from(ruleset::defense_stars(state.board.tile(position).terrain)),
    }
}

/// The combatant a unit of `kind` presents while standing on `position`.
///
/// `fire_mode` is the *striker's* in both halves of a strike: a commander rule
/// that reads it is asking how the shot was fired, not what the unit hit by it
/// carries.
fn combatant(
    state: &State,
    kind: UnitKindId,
    position: Pos,
    fire_mode: FireMode,
) -> Combatant<'static> {
    let terrain = state.board.tile(position).terrain;
    Combatant {
        kind,
        domain: combat_domain(ruleset::profile(kind)),
        fire_mode,
        terrain,
        weather: state.weather.kind,
        property: is_property(terrain),
        capabilities: &NO_CAPABILITIES,
    }
}

/// The commander-adjusted numbers one strike is scored with.
///
/// Every strike resolves the same way: the striker supplies the attack modifier
/// and the two luck domains, the target supplies defense and terrain stars, and
/// the striker's commander then gets to modify the target's stars. Three sites
/// spelled that out identically — the initiating hit, an ordinary counter, and
/// the counter a `counter-first` commander fires first.
struct StrikeValues {
    attack: i64,
    defense: i64,
    terrain_stars: u8,
    good_luck: commander::Domain,
    bad_luck: commander::Domain,
}

impl StrikeValues {
    /// The striker's half of [`combat::damage`].
    ///
    /// `hp` is a parameter because a counter is scored from what the initiating
    /// hit left behind. The striker's own `defense` and `terrain_stars` never
    /// enter the formula, which reads both only from the target.
    fn striker_side(&self, unit: &Unit, hp: u8) -> Side {
        Side {
            kind: unit.kind,
            hp,
            ammo: unit.ammo,
            attack: self.attack,
            defense: 100,
            terrain_stars: 0,
        }
    }

    /// The target's half of [`combat::damage`]. Its `attack` and `ammo` are
    /// inert for the same reason: only the striker fires.
    fn target_side(&self, unit: &Unit, hp: u8) -> Side {
        Side {
            kind: unit.kind,
            hp,
            ammo: unit.ammo,
            attack: 100,
            defense: self.defense,
            terrain_stars: self.terrain_stars,
        }
    }

    /// Draw this strike's signed luck modifier off the tape, good roll first.
    fn luck(&self, draws: &mut Draws<'_>) -> Result<i64, ExecuteError> {
        Ok(draw(draws, Luck::Good, self.good_luck)? - draw(draws, Luck::Bad, self.bad_luck)?)
    }
}

/// Score `striker` hitting `target` through both commanders' combat rules.
fn resolve_strike(
    state: &State,
    striker: Fighter<'_>,
    target: Fighter<'_>,
    strike: Strike,
) -> Result<StrikeValues, ExecuteError> {
    let overflow = || ExecuteError::InvalidState("commander combat overflow".into());
    let fire_mode = ruleset::profile(striker.unit.kind).fire_mode;
    let striker_context = combatant(state, striker.unit.kind, striker.position, fire_mode);
    let target_context = combatant(state, target.unit.kind, target.position, fire_mode);
    let striking = commander::effective_combat(
        state,
        &striker.unit.owner,
        striker_context,
        strike,
        combat_context(state, &striker.unit.owner, striker.position),
    )
    .ok_or_else(overflow)?;
    let defending = commander::effective_combat(
        state,
        &target.unit.owner,
        target_context,
        strike,
        combat_context(state, &target.unit.owner, target.position),
    )
    .ok_or_else(overflow)?;
    let stars = commander::effective_enemy_terrain_stars(
        state,
        &striker.unit.owner,
        striker_context,
        strike,
        defending.terrain_stars,
    )
    .ok_or_else(overflow)?;
    Ok(StrikeValues {
        attack: striking.attack,
        defense: defending.defense,
        terrain_stars: u8::try_from(stars)
            .map_err(|_| ExecuteError::InvalidState("terrain stars overflow".into()))?,
        good_luck: striking.good_luck,
        bad_luck: striking.bad_luck,
    })
}

/// Everything one landed strike does: the ammo it spends, the resolution and
/// damage it reports, and the funds and power charge the exchange transfers.
///
/// `striker` and `target` are read from the authoritative pre-state, so
/// `target_hp_after` is passed rather than derived. All four strikes an
/// engagement can contain spelled this sequence out identically.
#[allow(clippy::too_many_arguments)]
fn apply_hit(
    state: &State,
    next: &mut State,
    events: &mut Vec<Event>,
    hit: &Hit,
    striker: &Unit,
    target: &Unit,
    target_hp_after: u8,
    reason: KnownReason,
) -> Result<(), ExecuteError> {
    if hit.weapon.ammo_cost > 0 {
        let index = next
            .units
            .index_of(striker.id)
            .expect("a striker is present when its strike lands");
        let ammo_before = next.units[index].ammo;
        next.units[index].ammo -= hit.weapon.ammo_cost;
        events.push(Event::UnitResourced {
            unit: striker.id,
            fuel_before: striker.fuel,
            fuel_after: striker.fuel,
            ammo_before,
            ammo_after: next.units[index].ammo,
            reason: reason.into(),
        });
    }
    events.push(Event::AttackResolved {
        attacker: striker.id,
        weapon: hit.weapon.weapon,
        target: AttackTarget::Unit { unit: target.id },
    });
    events.push(Event::UnitDamaged {
        unit: target.id,
        from_hp: target.hp,
        to_hp: target_hp_after,
        reason: reason.into(),
    });
    apply_strike_funds(
        state,
        next,
        events,
        &striker.owner,
        &target.owner,
        target.kind,
        target.hp,
        target_hp_after,
    )?;
    apply_strike_power_charge(
        state,
        next,
        events,
        &striker.owner,
        &target.owner,
        target.kind,
        target.hp,
        target_hp_after,
        reason,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_strike_funds(
    state: &State,
    next: &mut State,
    events: &mut Vec<Event>,
    striker: &PlayerId,
    target_owner: &PlayerId,
    target_kind: UnitKindId,
    from_hp: u8,
    to_hp: u8,
) -> Result<(), ExecuteError> {
    let base_value = ruleset::profile(target_kind).cost;
    let target_value = commander::effective_build_cost(state, target_owner, base_value)
        .ok_or_else(|| ExecuteError::InvalidState("strike target value overflow".into()))?;
    let gain =
        commander::strike_funds_gain(state, striker, target_owner, from_hp, to_hp, target_value)
            .ok_or_else(|| {
                ExecuteError::InvalidState("strike funds profile or arithmetic is invalid".into())
            })?;
    if gain == 0 {
        return Ok(());
    }
    let player = next
        .find_player_mut(striker)
        .ok_or_else(|| ExecuteError::InvalidState("strike owner is absent".into()))?;
    let from = player.funds;
    let to = from
        .checked_add(gain)
        .ok_or_else(|| ExecuteError::InvalidState("strike funds overflow".into()))?;
    player.funds = to;
    events.push(Event::FundsChanged {
        player: striker.clone(),
        from,
        to,
        reason: KnownReason::CommanderPower.into(),
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_strike_power_charge(
    state: &State,
    next: &mut State,
    events: &mut Vec<Event>,
    striker: &PlayerId,
    target_owner: &PlayerId,
    target_kind: UnitKindId,
    from_hp: u8,
    to_hp: u8,
    reason: KnownReason,
) -> Result<(), ExecuteError> {
    let visual_damage = u64::from(from_hp.div_ceil(10).saturating_sub(to_hp.div_ceil(10)));
    if visual_damage == 0 {
        return Ok(());
    }
    let base_value = ruleset::profile(target_kind).cost;
    let target_value = commander::effective_build_cost(state, target_owner, base_value)
        .ok_or_else(|| ExecuteError::InvalidState("power charge unit value overflow".into()))?;
    let dealt_gain = target_value
        .checked_mul(visual_damage)
        .and_then(|value| value.checked_div(20))
        .ok_or_else(|| ExecuteError::InvalidState("dealt power charge overflow".into()))?;
    let received_gain = target_value
        .checked_mul(visual_damage)
        .and_then(|value| value.checked_div(10))
        .ok_or_else(|| ExecuteError::InvalidState("received power charge overflow".into()))?;
    for (player_id, gain) in [(striker, dealt_gain), (target_owner, received_gain)] {
        if gain == 0 {
            continue;
        }
        let player_index = next
            .player_index(player_id)
            .ok_or_else(|| ExecuteError::InvalidState("combat owner is absent".into()))?;
        if !matches!(next.player_mut(player_index).power_state, PowerState::None) {
            continue;
        }
        let active_slot = next
            .player_mut(player_index)
            .commanders
            .iter()
            .position(|commander| commander.active)
            .ok_or_else(|| ExecuteError::InvalidState("active commander is absent".into()))?;
        let commander_slots = if state.settings.tags {
            if next.player_mut(player_index).commanders.len() != 2 {
                return Err(ExecuteError::InvalidState(
                    "tag player does not have two commander slots".into(),
                ));
            }
            vec![active_slot, 1 - active_slot]
        } else {
            vec![active_slot]
        };
        for commander_slot in commander_slots {
            let slot_gain = if commander_slot == active_slot {
                gain
            } else {
                gain / 2
            };
            if slot_gain == 0 {
                continue;
            }
            let commander = &next.player_mut(player_index).commanders[commander_slot];
            let Some(maximum) = commander::maximum_power_charge(commander.id, commander.power_uses)
                .map_err(|_| ExecuteError::InvalidState("maximum power charge overflow".into()))?
            else {
                continue;
            };
            let from = commander.power_charge;
            if from >= maximum {
                continue;
            }
            let to = from
                .checked_add(slot_gain)
                .ok_or_else(|| ExecuteError::InvalidState("power charge overflow".into()))?
                .min(maximum);
            if to == from {
                continue;
            }
            next.player_mut(player_index).commanders[commander_slot].power_charge = to;
            events.push(Event::PowerChargeChanged {
                player: player_id.clone(),
                commander_slot,
                from,
                to,
                reason: reason.into(),
            });
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_tile_attack(
    state: &State,
    player: &PlayerId,
    unit_id: UnitId,
    attacker_index: usize,
    attacker: &Unit,
    origin: Pos,
    position: Pos,
) -> Result<Execution, ExecuteError> {
    let tile = state.board.get(position).ok_or_else(|| {
        violation(Violation::InvalidTarget {
            target: Some(position.into()),
        })
    })?;
    if state
        .units
        .iter()
        .any(|unit| board_position(unit) == Some(position))
    {
        return Err(violation(Violation::InvalidTarget {
            target: Some(position.into()),
        }));
    }

    let Some(destructible) = ruleset::terrain(tile.terrain).destructible else {
        return Err(violation(Violation::InvalidTarget {
            target: Some(position.into()),
        }));
    };
    let from_hp = tile
        .destructible_hp()
        .ok_or_else(|| ExecuteError::InvalidState("destructible tile has no HP".into()))?;
    if from_hp > destructible.maximum_hp {
        return Err(ExecuteError::InvalidState(
            "destructible tile HP exceeds its maximum".into(),
        ));
    }
    let from_hp = u8::try_from(from_hp)
        .map_err(|_| ExecuteError::InvalidState("destructible tile HP overflow".into()))?;
    let target_kind = destructible.target_kind;
    let destruction_replacement = destructible.destruction_replacement;

    let actor_team = state
        .find_player(player)
        .map(|candidate| &candidate.team)
        .ok_or_else(|| ExecuteError::InvalidState("active player is absent".into()))?;
    if state.settings.fog && !AwbwVisibility.view(state, actor_team).position(position) {
        return Err(violation(Violation::InvalidTarget {
            target: Some(position.into()),
        }));
    }

    let profile = ruleset::profile(attacker.kind);
    let fire_mode = profile.fire_mode;
    if fire_mode == FireMode::None {
        return Err(violation(Violation::ActionNotSupported {
            action: Action::Attack,
        }));
    }
    let distance = origin.distance(position);
    if let Some(range) = profile.indirect_range {
        let minimum = range.minimum;
        let maximum = commander::effective_attack_range(
            state,
            attacker,
            range.maximum,
            profile.domain,
            FireMode::Indirect,
        );
        if distance < minimum || distance > maximum {
            return Err(violation(Violation::TargetOutOfRange {
                target: Some(position.into()),
            }));
        }
    } else if distance != 1 {
        return Err(violation(Violation::TargetOutOfRange {
            target: Some(position.into()),
        }));
    }
    if combat::select_weapon(attacker.kind, target_kind, attacker.ammo).is_none() {
        return Err(violation(Violation::InvalidTarget {
            target: Some(position.into()),
        }));
    }

    let striking = commander::effective_combat(
        state,
        player,
        combatant(state, attacker.kind, origin, fire_mode),
        Strike::Initial,
        combat_context(state, player, origin),
    )
    .ok_or_else(|| ExecuteError::InvalidState("commander combat overflow".into()))?;
    let hit = combat::damage(
        Side {
            kind: attacker.kind,
            hp: attacker.hp,
            ammo: attacker.ammo,
            attack: striking.attack,
            defense: 100,
            // The formula reads defense and terrain stars only from the target.
            terrain_stars: 0,
        },
        Side {
            kind: target_kind,
            hp: from_hp,
            ammo: 0,
            attack: 100,
            defense: 100,
            terrain_stars: 0,
        },
        0,
    )
    .expect("tile weapon was validated");
    let to_hp = from_hp.saturating_sub(hit.damage);
    let mut next = state.clone();
    let mut events = Vec::new();
    if hit.weapon.ammo_cost > 0 {
        let before = next.units[attacker_index].ammo;
        next.units[attacker_index].ammo -= hit.weapon.ammo_cost;
        events.push(Event::UnitResourced {
            unit: unit_id,
            fuel_before: attacker.fuel,
            fuel_after: attacker.fuel,
            ammo_before: before,
            ammo_after: next.units[attacker_index].ammo,
            reason: KnownReason::Combat.into(),
        });
    }
    events.push(Event::AttackResolved {
        attacker: unit_id,
        weapon: hit.weapon.weapon,
        target: AttackTarget::Tile { position },
    });
    events.push(Event::DestructibleDamaged {
        position,
        from_hp,
        to_hp,
    });
    if to_hp == 0 {
        next.board.tile_mut(position).terrain = destruction_replacement;
        next.board.tile_mut(position).set_destructible_hp(None);
        events.push(Event::TileTerrainChanged {
            position,
            from: tile.terrain,
            to: destruction_replacement,
            reason: KnownReason::Combat.into(),
        });
    } else {
        next.board
            .tile_mut(position)
            .set_destructible_hp(Some(u64::from(to_hp)));
    }
    next.units[attacker_index].action = UnitAction::Spent;
    events.push(Event::UnitActionChanged {
        unit: unit_id,
        from: UnitAction::Ready,
        to: UnitAction::Spent,
        reason: KnownReason::Attack.into(),
    });
    Ok(Execution {
        state: next,
        events,
        random_consumed: 0,
    })
}

pub(crate) fn execute_move_attack(
    turn: &ActiveTurn<'_>,
    unit_id: UnitId,
    path: Vec<Pos>,
    target: AttackTarget,
    draws: &mut Draws<'_>,
) -> Result<Execution, ExecuteError> {
    let state = turn.state();
    let player = turn.player();
    let plan = turn.plan_move(unit_id, path)?;
    let ai = plan.unit_index();
    let attacker = &state.units[ai];
    let origin = plan.origin();

    if plan.path().len() > 1 {
        match ruleset::profile(attacker.kind).fire_mode {
            FireMode::Indirect => {
                return Err(violation(Violation::ActionNotSupported {
                    action: Action::MoveAndFire,
                }));
            }
            FireMode::None => {
                return Err(violation(Violation::ActionNotSupported {
                    action: Action::Attack,
                }));
            }
            FireMode::Direct => {}
        }

        let destination = plan.destination();
        let view = AwbwVisibility.view(state, plan.actor_team());
        if state.units.iter().any(|other| {
            other.id != unit_id
                && board_position(other) == Some(destination)
                && occupancy_is_disclosed(&view, other)
        }) {
            return Err(violation(Violation::DestinationOccupied {
                position: destination,
            }));
        }

        let mut movement = execute_planned_movement(state, unit_id, &plan);
        if movement.trapped {
            return Ok(Execution {
                state: movement.state,
                events: movement.events,
                random_consumed: 0,
            });
        }

        // Movement spends the unit for movement-only actions. Restore readiness
        // internally so the atomic follow-up can resolve and emit the single
        // attack action transition.
        movement.state.units[plan.unit_index()].action = UnitAction::Ready;
        let mut combat = execute_stationary_attack(
            &movement.state,
            player,
            unit_id,
            plan.unit_index(),
            destination,
            target,
            draws,
        )?;
        movement.events.append(&mut combat.events);
        combat.events = movement.events;
        return Ok(combat);
    }
    execute_stationary_attack(state, player, unit_id, ai, origin, target, draws)
}

/// Resolve an attack after movement validation has established the attacker.
///
/// Move-and-attack reaches this with a derived state, so it cannot reuse the
/// [`ActiveTurn`] tied to the command's input state. The movement reducer is
/// the only caller on that path and preserves the active-turn invariants.
fn execute_stationary_attack(
    state: &State,
    player: &PlayerId,
    unit_id: UnitId,
    ai: usize,
    origin: Pos,
    target: AttackTarget,
    draws: &mut Draws<'_>,
) -> Result<Execution, ExecuteError> {
    let attacker = &state.units[ai];
    let target_id = match target {
        AttackTarget::Unit { unit } => unit,
        AttackTarget::Tile { position } => {
            return execute_tile_attack(state, player, unit_id, ai, attacker, origin, position);
        }
    };
    let engagement = Engagement::open(state, player, ai, origin, target_id)?;
    if engagement.counter_comes_first() {
        resolve_counter_first(&engagement, draws)
    } else {
        resolve_exchange(&engagement, draws)
    }
}

/// A validated unit-versus-unit engagement.
///
/// Opening one establishes everything the exchange needs and nothing about the
/// order it happens in: both fighters, their indices in the authoritative
/// state, whether the defender can answer at all, and the numbers the
/// initiating strike is scored with. Which side fires first is then a question
/// the commander layer answers, not a branch that re-derives the engagement.
struct Engagement<'a> {
    state: &'a State,
    attacker: Fighter<'a>,
    defender: Fighter<'a>,
    attacker_index: usize,
    defender_index: usize,
    initial: StrikeValues,
    /// Adjacent, direct-fire, and holding a weapon that bites — the three
    /// conditions a counter needs before a commander is consulted.
    counter_armed: bool,
}

impl<'a> Engagement<'a> {
    /// Check the target and score the initiating strike.
    fn open(
        state: &'a State,
        player: &PlayerId,
        attacker_index: usize,
        origin: Pos,
        target_id: UnitId,
    ) -> Result<Self, ExecuteError> {
        let invalid = || {
            violation(Violation::InvalidTarget {
                target: Some(target_id.into()),
            })
        };
        let attacker = &state.units[attacker_index];
        let defender_index = state.units.index_of(target_id).ok_or_else(invalid)?;
        let defender = &state.units[defender_index];
        let Location::Board {
            position: defender_position,
        } = defender.location
        else {
            return Err(invalid());
        };
        if defender.owner == player {
            return Err(invalid());
        }
        let actor_team = state
            .find_player(player)
            .map(|candidate| &candidate.team)
            .ok_or_else(|| ExecuteError::InvalidState("active player is absent".into()))?;
        if !AwbwVisibility.view(state, actor_team).unit(defender) {
            return Err(invalid());
        }
        let concealed_target_compatible = match (defender.concealment, defender.kind, attacker.kind)
        {
            (Concealment::Hidden, UnitKindId::Sub, UnitKindId::Sub | UnitKindId::Cruiser)
            | (
                Concealment::Hidden,
                UnitKindId::Stealth,
                UnitKindId::Fighter | UnitKindId::Stealth,
            ) => true,
            (Concealment::Hidden, UnitKindId::Sub | UnitKindId::Stealth, _) => false,
            _ => true,
        };
        if !concealed_target_compatible {
            return Err(invalid());
        }
        let profile = ruleset::profile(attacker.kind);
        if profile.fire_mode == FireMode::None {
            return Err(violation(Violation::ActionNotSupported {
                action: Action::Attack,
            }));
        }
        let distance = origin.distance(defender_position);
        if let Some(range) = profile.indirect_range {
            let maximum = commander::effective_attack_range(
                state,
                attacker,
                range.maximum,
                profile.domain,
                FireMode::Indirect,
            );
            if distance < range.minimum || distance > maximum {
                return Err(violation(Violation::TargetOutOfRange {
                    target: Some(target_id.into()),
                }));
            }
        } else if distance != 1 {
            return Err(violation(Violation::TargetOutOfRange {
                target: Some(target_id.into()),
            }));
        }
        if combat::select_weapon(attacker.kind, defender.kind, attacker.ammo).is_none() {
            return Err(invalid());
        }

        let attacker = Fighter {
            unit: attacker,
            position: origin,
        };
        let defender = Fighter {
            unit: defender,
            position: defender_position,
        };
        Ok(Self {
            state,
            attacker,
            defender,
            attacker_index,
            defender_index,
            initial: resolve_strike(state, attacker, defender, Strike::Initial)?,
            counter_armed: distance == 1
                && ruleset::profile(defender.unit.kind).fire_mode == FireMode::Direct
                && combat::select_weapon(
                    defender.unit.kind,
                    attacker.unit.kind,
                    defender.unit.ammo,
                )
                .is_some(),
        })
    }

    /// The numbers the defender's counter is scored with.
    fn counter_values(&self) -> Result<StrikeValues, ExecuteError> {
        resolve_strike(self.state, self.defender, self.attacker, Strike::Counter)
    }

    /// Whether the defender's commander turns the exchange around and fires
    /// before the strike that provoked it.
    fn counter_comes_first(&self) -> bool {
        let defender = self.defender.unit;
        self.counter_armed
            && commander::counter_first(
                self.state,
                &defender.owner,
                combatant(
                    self.state,
                    defender.kind,
                    self.defender.position,
                    ruleset::profile(defender.kind).fire_mode,
                ),
                Strike::Counter,
            )
    }
}

/// The ordinary exchange: the initiating strike lands, then the defender
/// answers if it survived and can.
fn resolve_exchange(
    engagement: &Engagement<'_>,
    draws: &mut Draws<'_>,
) -> Result<Execution, ExecuteError> {
    let state = engagement.state;
    let attacker = engagement.attacker.unit;
    let defender = engagement.defender.unit;
    let attacker_index = engagement.attacker_index;
    let defender_index = engagement.defender_index;

    let attack_luck = engagement.initial.luck(draws)?;
    let first = combat::damage(
        engagement.initial.striker_side(attacker, attacker.hp),
        engagement.initial.target_side(defender, defender.hp),
        attack_luck,
    )
    .ok_or_else(|| {
        violation(Violation::InvalidTarget {
            target: Some(defender.id.into()),
        })
    })?;
    let defender_remaining = defender.hp.saturating_sub(first.damage);
    let counter = if defender_remaining > 0 && engagement.counter_armed {
        let values = engagement.counter_values()?;
        let luck = values.luck(draws)?;
        combat::damage(
            values.striker_side(defender, defender_remaining),
            values.target_side(attacker, attacker.hp),
            luck,
        )
    } else {
        None
    };

    let mut next = state.clone();
    let mut events = Vec::new();
    apply_hit(
        state,
        &mut next,
        &mut events,
        &first,
        attacker,
        defender,
        defender_remaining,
        KnownReason::Combat,
    )?;
    if defender_remaining > 0 {
        next.units[defender_index].hp = defender_remaining;
    }
    let mut attacker_removed = false;
    if let Some(hit) = counter {
        let attacker_remaining = attacker.hp.saturating_sub(hit.damage);
        apply_hit(
            state,
            &mut next,
            &mut events,
            &hit,
            defender,
            attacker,
            attacker_remaining,
            KnownReason::CombatCounter,
        )?;
        if attacker_remaining == 0 {
            let Location::Board { position } = attacker.location else {
                unreachable!("an attacking unit is on the board");
            };
            movement::reset_capture_on_removal(&mut next, position, &mut events);
            remove_combatant_and_cargo(
                &mut next,
                attacker.id,
                KnownReason::CombatCounter,
                &mut events,
            );
            attacker_removed = true;
        } else {
            next.units[attacker_index].hp = attacker_remaining;
        }
    }
    if defender_remaining == 0 {
        let Location::Board { position } = defender.location else {
            unreachable!("a defending unit is on the board");
        };
        movement::reset_capture_on_removal(&mut next, position, &mut events);
        remove_combatant_and_cargo(&mut next, defender.id, KnownReason::Combat, &mut events);
    }
    if !attacker_removed {
        spend_attacker(&mut next, &mut events, attacker.id);
    } else {
        rout_if_last_unit(&mut next, &attacker.owner, &mut events)?;
    }
    if defender_remaining == 0 {
        rout_if_last_unit(&mut next, &defender.owner, &mut events)?;
    }
    Ok(Execution {
        state: next,
        events,
        random_consumed: draws.drawn(),
    })
}

/// The exchange a `counter-first` commander inverts: the defender fires before
/// the strike that provoked it, and an attacker that does not survive never
/// fires at all.
fn resolve_counter_first(
    engagement: &Engagement<'_>,
    draws: &mut Draws<'_>,
) -> Result<Execution, ExecuteError> {
    let state = engagement.state;
    let attacker = engagement.attacker.unit;
    let defender = engagement.defender.unit;

    let counter = engagement.counter_values()?;
    let counter_luck = counter.luck(draws)?;
    let preemptive = combat::damage(
        counter.striker_side(defender, defender.hp),
        counter.target_side(attacker, attacker.hp),
        counter_luck,
    )
    .expect("counter-first eligibility selected a weapon");
    let attacker_remaining = attacker.hp.saturating_sub(preemptive.damage);
    let initiating = if attacker_remaining > 0 {
        let attack_luck = engagement.initial.luck(draws)?;
        Some(
            combat::damage(
                engagement
                    .initial
                    .striker_side(attacker, attacker_remaining),
                engagement.initial.target_side(defender, defender.hp),
                attack_luck,
            )
            .expect("initiating weapon was validated"),
        )
    } else {
        None
    };

    let mut next = state.clone();
    let mut events = Vec::new();
    apply_hit(
        state,
        &mut next,
        &mut events,
        &preemptive,
        defender,
        attacker,
        attacker_remaining,
        KnownReason::CombatCounter,
    )?;
    if attacker_remaining == 0 {
        let Location::Board { position } = attacker.location else {
            unreachable!("an attacking unit is on the board");
        };
        movement::reset_capture_on_removal(&mut next, position, &mut events);
        remove_combatant_and_cargo(
            &mut next,
            attacker.id,
            KnownReason::CombatCounter,
            &mut events,
        );
        rout_if_last_unit(&mut next, &attacker.owner, &mut events)?;
        return Ok(Execution {
            state: next,
            events,
            random_consumed: draws.drawn(),
        });
    }
    next.units[engagement.attacker_index].hp = attacker_remaining;

    let hit = initiating.expect("surviving attacker performs initiating strike");
    let defender_remaining = defender.hp.saturating_sub(hit.damage);
    apply_hit(
        state,
        &mut next,
        &mut events,
        &hit,
        attacker,
        defender,
        defender_remaining,
        KnownReason::Combat,
    )?;
    if defender_remaining == 0 {
        let Location::Board { position } = defender.location else {
            unreachable!("a defending unit is on the board");
        };
        movement::reset_capture_on_removal(&mut next, position, &mut events);
        remove_combatant_and_cargo(&mut next, defender.id, KnownReason::Combat, &mut events);
    } else {
        let defender_index = next
            .units
            .index_of(defender.id)
            .expect("the target remains present until it is removed");
        next.units[defender_index].hp = defender_remaining;
    }
    spend_attacker(&mut next, &mut events, attacker.id);
    if defender_remaining == 0 {
        rout_if_last_unit(&mut next, &defender.owner, &mut events)?;
    }
    Ok(Execution {
        state: next,
        events,
        random_consumed: draws.drawn(),
    })
}

/// Remove a destroyed board unit and any units it carried.
///
/// Cargo loss is a consequence of losing the carrier, not another combat
/// strike, so it emits removal facts but earns no combat power charge.
fn remove_combatant_and_cargo(
    next: &mut State,
    unit: UnitId,
    reason: KnownReason,
    events: &mut Vec<Event>,
) {
    let mut cargo: Vec<_> = next
        .units
        .iter()
        .filter_map(|candidate| match candidate.location {
            Location::Cargo { transport, slot } if transport == unit => Some((slot, candidate.id)),
            _ => None,
        })
        .collect();
    cargo.sort();
    next.units.retain(|candidate| {
        candidate.id != unit
            && !matches!(
                candidate.location,
                Location::Cargo { transport, .. } if transport == unit
            )
    });
    events.push(Event::UnitRemoved {
        unit,
        reason: reason.into(),
    });
    for (_, cargo) in cargo {
        events.push(Event::UnitRemoved {
            unit: cargo,
            reason: KnownReason::CarrierLost.into(),
        });
    }
}

/// Mark the acting unit spent. Both exchange orders end here, and both reach it
/// only once the attacker is known to have survived.
fn spend_attacker(next: &mut State, events: &mut Vec<Event>, attacker: UnitId) {
    let index = next
        .units
        .index_of(attacker)
        .expect("a spent attacker survived its own engagement");
    next.units[index].action = UnitAction::Spent;
    events.push(Event::UnitActionChanged {
        unit: attacker,
        from: UnitAction::Ready,
        to: UnitAction::Spent,
        reason: KnownReason::Attack.into(),
    });
}

/// Eliminate `owner` if the strike just removed the last unit they had.
fn rout_if_last_unit(
    next: &mut State,
    owner: &PlayerId,
    events: &mut Vec<Event>,
) -> Result<(), ExecuteError> {
    if next.units.iter().any(|unit| unit.owner == *owner) {
        return Ok(());
    }
    eliminate_player(
        next,
        owner,
        ruleset::VictoryReason::Rout,
        None,
        None,
        events,
    )?;
    Ok(())
}
