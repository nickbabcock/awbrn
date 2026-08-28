//! Combat: choosing a target, resolving the exchange, and its side effects.
//!
//! Normative source:
//! * `spec/semantics/combat.md`

use super::ReducerError as ExecuteError;
use super::*;
use crate::combat::{self, CounterStep, DamageRange, Forecast, HEALTH_STEP, Hit, Side};
use crate::commander::{self, CombatContext, Combatant, Strike};
use crate::ruleset::{self, FireMode, TerrainTrait};
use crate::semantic::{PowerState, UnitAction};
use crate::violation::Action;
use std::collections::HashSet;
use std::sync::LazyLock;

#[derive(Debug)]
pub(super) struct Attack(pub(super) AttackTarget);

#[derive(Debug)]
pub(super) struct AttackProof {
    target: PreparedAttackTarget,
    destination: Option<AvailableDestination>,
}

/// Commander combat predicates take a capability set per combatant. No path
/// through this module supplies one, so every combatant shares this empty set
/// instead of allocating one per strike.
static NO_CAPABILITIES: LazyLock<HashSet<String>> = LazyLock::new(HashSet::new);

/// One side of an engagement: a unit, the seat its owner holds, and the tile
/// the strike is scored from.
///
/// The position is carried rather than read back off the unit because
/// move-and-attack scores the initiating strike from the destination it just
/// resolved. The seat is carried because every commander question the exchange
/// asks names the same two players, and resolving a name walked the roster
/// comparing strings each time. `None` is an owner off the roster, whom no
/// commander answers for.
#[derive(Clone, Copy)]
struct Fighter<'a> {
    unit: &'a Unit,
    seat: Option<PlayerIdx>,
    position: Pos,
}

impl<'a> Fighter<'a> {
    fn new(state: &State, unit: &'a Unit, position: Pos) -> Self {
        Self {
            unit,
            seat: state.players.get(unit.owner.get()).map(|_| unit.owner),
            position,
        }
    }
}

fn is_property(terrain: TerrainId) -> bool {
    ruleset::terrain_has(terrain, TerrainTrait::Capturable)
}

/// The board- and treasury-wide values a commander's combat rules read, for
/// `unit` firing from or standing on `position`.
fn combat_context(
    state: &State,
    holdings: &Holdings<'_>,
    owner: Option<PlayerIdx>,
    unit: UnitKindId,
    position: Pos,
) -> CombatContext {
    debug_assert!(holdings.counted(state), "holdings tallied another state");
    let (held, funds) = holdings.of(owner);
    let base_terrain_stars = match ruleset::profile(unit).domain {
        Domain::Air => 0,
        Domain::Ground | Domain::Sea => {
            i64::from(ruleset::defense_stars(state.board.tile(position).terrain))
        }
    };
    CombatContext {
        tower_count: held.tower_count,
        funds,
        owned_properties: held.owned_properties,
        base_terrain_stars,
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

    /// The worst and best signed luck modifiers this strike can draw.
    ///
    /// The two domains are drawn independently and subtracted, so the extremes
    /// are the extremes of the difference rather than of either domain.
    fn luck_bounds(&self) -> (i64, i64) {
        (
            self.good_luck.minimum - self.bad_luck.maximum,
            self.good_luck.maximum - self.bad_luck.minimum,
        )
    }

    /// The damage this strike lands at one end of its luck, as the pair
    /// `(landed, raw)`, or zeroes when the striker is not standing to fire it.
    ///
    /// Both halves are needed and they are not interchangeable. The exchange
    /// runs on what lands, because a counter is scored from the health the
    /// strike actually left; the forecast reports the raw figure, because the
    /// overkill it hides is what a player is choosing between.
    fn scored(
        &self,
        striker: &Unit,
        striker_hp: u8,
        target: &Unit,
        target_hp: u8,
        luck: i64,
    ) -> (u8, u16) {
        if striker_hp == 0 {
            return (0, 0);
        }
        combat::damage(
            self.striker_side(striker, striker_hp),
            self.target_side(target, target_hp),
            luck,
        )
        .map_or((0, 0), |hit| (hit.damage, hit.raw_damage))
    }
}

/// Score `striker` hitting `target` through both commanders' combat rules.
fn resolve_strike(
    state: &State,
    holdings: &Holdings<'_>,
    striker: Fighter<'_>,
    target: Fighter<'_>,
    strike: Strike,
) -> Result<StrikeValues, ExecuteError> {
    debug_assert!(holdings.counted(state), "holdings tallied another state");
    let overflow = || ExecuteError::InvalidState("commander combat overflow".into());
    let fire_mode = ruleset::profile(striker.unit.kind).fire_mode;
    let striker_context = combatant(state, striker.unit.kind, striker.position, fire_mode);
    let target_context = combatant(state, target.unit.kind, target.position, fire_mode);
    let striking = commander::effective_combat(
        state,
        striker.seat,
        striker_context,
        strike,
        combat_context(
            state,
            holdings,
            striker.seat,
            striker.unit.kind,
            striker.position,
        ),
    )
    .ok_or_else(overflow)?;
    let defending = commander::effective_combat(
        state,
        target.seat,
        target_context,
        strike,
        combat_context(
            state,
            holdings,
            target.seat,
            target.unit.kind,
            target.position,
        ),
    )
    .ok_or_else(overflow)?;
    let stars = commander::effective_enemy_terrain_stars(
        state,
        striker.seat,
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
        striker.owner,
        target.owner,
        target.kind,
        target.hp,
        target_hp_after,
    )?;
    apply_strike_power_charge(
        state,
        next,
        events,
        striker.owner,
        target.owner,
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
    striker: PlayerIdx,
    target_owner: PlayerIdx,
    target_kind: UnitKindId,
    from_hp: u8,
    to_hp: u8,
) -> Result<(), ExecuteError> {
    let base_value = ruleset::profile(target_kind).cost;
    let target_value = commander::effective_build_cost(state, Some(target_owner), base_value)
        .ok_or_else(|| ExecuteError::InvalidState("strike target value overflow".into()))?;
    let gain = commander::strike_funds_gain(
        state,
        Some(striker),
        Some(target_owner),
        from_hp,
        to_hp,
        target_value,
    )
    .ok_or_else(|| {
        ExecuteError::InvalidState("strike funds profile or arithmetic is invalid".into())
    })?;
    if gain == 0 {
        return Ok(());
    }
    let striker_id = next
        .try_player_id(striker)
        .ok_or_else(|| ExecuteError::InvalidState("strike owner is absent".into()))?
        .clone();
    let player = next.player_mut(striker);
    let from = player.funds;
    let to = from
        .checked_add(gain)
        .ok_or_else(|| ExecuteError::InvalidState("strike funds overflow".into()))?;
    player.funds = to;
    events.push(Event::FundsChanged {
        player: striker_id,
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
    striker: PlayerIdx,
    target_owner: PlayerIdx,
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
    let target_value = commander::effective_build_cost(state, Some(target_owner), base_value)
        .ok_or_else(|| ExecuteError::InvalidState("power charge unit value overflow".into()))?;
    let dealt_gain = target_value
        .checked_mul(visual_damage)
        .and_then(|value| value.checked_div(20))
        .ok_or_else(|| ExecuteError::InvalidState("dealt power charge overflow".into()))?;
    let received_gain = target_value
        .checked_mul(visual_damage)
        .and_then(|value| value.checked_div(10))
        .ok_or_else(|| ExecuteError::InvalidState("received power charge overflow".into()))?;
    for (player_index, gain) in [(striker, dealt_gain), (target_owner, received_gain)] {
        if gain == 0 {
            continue;
        }
        if next.players.get(player_index.get()).is_none() {
            return Err(ExecuteError::InvalidState("combat owner is absent".into()));
        }
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
                player: next.player_id(player_index).clone(),
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
/// Score one strike against a destructible tile.
///
/// A tile answers nothing and no commander grants luck against one, so the
/// number is exact. `None` means the attacker holds no weapon that bites this
/// tile, which the reducer treats as an invalid target and a forecast treats as
/// nothing to report.
fn score_tile_strike(
    state: &State,
    holdings: &Holdings<'_>,
    player: &PlayerId,
    attacker: &Unit,
    origin: Pos,
    target_kind: UnitKindId,
    target_hp: u8,
) -> Result<Option<Hit>, ExecuteError> {
    let fire_mode = ruleset::profile(attacker.kind).fire_mode;
    let seat = state.player_index(player);
    let striking = commander::effective_combat(
        state,
        seat,
        combatant(state, attacker.kind, origin, fire_mode),
        Strike::Initial,
        combat_context(state, holdings, seat, attacker.kind, origin),
    )
    .ok_or_else(|| ExecuteError::InvalidState("commander combat overflow".into()))?;
    Ok(combat::damage(
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
            hp: target_hp,
            ammo: 0,
            attack: 100,
            defense: 100,
            terrain_stars: 0,
        },
        0,
    ))
}

struct ValidatedTileAttack {
    hp: u8,
    kind: UnitKindId,
    destruction_replacement: TerrainId,
}

fn validate_tile_attack(
    state: &State,
    attacker: &Unit,
    origin: Pos,
    position: Pos,
    view: &AwbwView<'_>,
) -> Result<ValidatedTileAttack, ExecuteError> {
    let tile = state.board.get(position).ok_or_else(|| {
        violation(Violation::InvalidTarget {
            target: Some(position.into()),
        })
    })?;
    if state.units.iter().any(|unit| {
        board_position(unit) == Some(position)
            && !(unit.id == attacker.id && board_position(attacker) != Some(origin))
    }) {
        return Err(violation(Violation::InvalidTarget {
            target: Some(position.into()),
        }));
    }

    let Some(destructible) = ruleset::terrain(tile.terrain).destructible else {
        return Err(violation(Violation::InvalidTarget {
            target: Some(position.into()),
        }));
    };
    let from_hp = state
        .board
        .destructible_hp(position)
        .ok_or_else(|| ExecuteError::InvalidState("destructible tile has no HP".into()))?;
    if from_hp > destructible.maximum_hp {
        return Err(ExecuteError::InvalidState(
            "destructible tile HP exceeds its maximum".into(),
        ));
    }
    let hp = u8::try_from(from_hp)
        .map_err(|_| ExecuteError::InvalidState("destructible tile HP overflow".into()))?;
    let kind = destructible.target_kind;

    if state.settings.fog && !view.position(position) {
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
    if combat::select_weapon(attacker.kind, kind, attacker.ammo).is_none() {
        return Err(violation(Violation::InvalidTarget {
            target: Some(position.into()),
        }));
    }
    Ok(ValidatedTileAttack {
        hp,
        kind,
        destruction_replacement: destructible.destruction_replacement,
    })
}

fn attack_view<'a>(state: &'a State, player: &PlayerId) -> Result<AwbwView<'a>, ExecuteError> {
    let team = state
        .find_player(player)
        .map(|candidate| &candidate.team)
        .ok_or_else(|| ExecuteError::InvalidState("active player is absent".into()))?;
    Ok(AwbwView::new(state, team))
}

fn execute_tile_attack(
    state: &State,
    holdings: &Holdings<'_>,
    player: &PlayerId,
    unit_id: UnitId,
    attacker_index: usize,
    origin: Pos,
    position: Pos,
) -> Result<Execution, ExecuteError> {
    let attacker = &state.units[attacker_index];
    let view = attack_view(state, player)?;
    let target = validate_tile_attack(state, attacker, origin, position, &view)?;
    let from_hp = target.hp;
    let target_kind = target.kind;
    let destruction_replacement = target.destruction_replacement;

    let hit = score_tile_strike(
        state,
        holdings,
        player,
        attacker,
        origin,
        target_kind,
        from_hp,
    )?
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
        next.board.set_destructible_hp(position, None);
        events.push(Event::TileTerrainChanged {
            position,
            from: state.board.tile(position).terrain,
            to: destruction_replacement,
            reason: KnownReason::Combat.into(),
        });
    } else {
        next.board
            .set_destructible_hp(position, Some(u64::from(to_hp)));
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

impl<'a> DestinationAction<'a> for Attack {
    type Proof = AttackProof;

    fn validate<M>(&self, at: &PreparedDestination<'a, M>) -> Result<Self::Proof, ExecuteError>
    where
        M: std::borrow::Borrow<crate::query::TurnMaps<'a>>,
    {
        let target = self.0;
        let movement = at.movement();
        let state = movement.state();
        let plan = movement.plan();
        let ai = plan.unit_index();
        let attacker = &state.units[ai];

        let (prepared_target, available_destination) = if plan.path_len() > 1 {
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

            let prepared_target =
                prepare_attack_target(state, plan.actor_team(), at.view(), target)?;
            let attack_origin = plan.destination();
            let available_destination = at.available_destination()?;

            if at.trap().is_none() {
                validate_attack_target(
                    state,
                    at.holdings(),
                    ai,
                    attack_origin,
                    prepared_target,
                    at.view(),
                )?;
            }
            (prepared_target, Some(available_destination))
        } else {
            let prepared_target =
                prepare_attack_target(state, plan.actor_team(), at.view(), target)?;
            validate_attack_target(
                state,
                at.holdings(),
                ai,
                plan.origin(),
                prepared_target,
                at.view(),
            )?;
            (prepared_target, None)
        };

        Ok(AttackProof {
            target: prepared_target,
            destination: available_destination,
        })
    }

    fn into_kind(bound: MovementAction<'a, Self::Proof>) -> PreparedCommandKind<'a> {
        PreparedCommandKind::Attack(bound)
    }
}

fn validate_attack_target(
    state: &State,
    holdings: &Holdings<'_>,
    attacker_index: usize,
    origin: Pos,
    target: PreparedAttackTarget,
    view: &AwbwView<'_>,
) -> Result<(), ExecuteError> {
    let attacker = &state.units[attacker_index];
    match target {
        PreparedAttackTarget::Unit(disclosed) => {
            Engagement::open(state, holdings, attacker_index, origin, disclosed)?;
        }
        PreparedAttackTarget::Tile(position) => {
            validate_tile_attack(state, attacker, origin, position, view)?;
        }
    }
    Ok(())
}

pub(super) fn execute_prepared_attack(
    prepared: MovementAction<'_, AttackProof>,
    draws: &mut Draws<'_>,
) -> Result<Execution, ExecuteError> {
    let MovementAction {
        movement,
        trap,
        action:
            AttackProof {
                target: prepared_target,
                destination: _destination,
            },
    } = prepared;
    let state = movement.state();
    let player = &state.turn.active_player;
    let unit_id = movement.unit();
    let plan = movement.plan();
    let ai = plan.unit_index();
    let origin = plan.origin();

    if plan.path().len() > 1 {
        let destination = plan.destination();

        let mut outcome = execute_planned_movement(state, unit_id, plan, trap);
        if outcome.trapped {
            return Ok(Execution {
                state: outcome.state,
                events: outcome.events,
                random_consumed: 0,
            });
        }

        // Movement spends the unit for movement-only actions. Restore readiness
        // internally so the atomic follow-up can resolve and emit the single
        // attack action transition.
        outcome.state.units[plan.unit_index()].action = UnitAction::Ready;
        let mut combat = execute_stationary_attack(
            &outcome.state,
            player,
            unit_id,
            plan.unit_index(),
            destination,
            prepared_target,
            draws,
        )?;
        outcome.events.append(&mut combat.events);
        combat.events = outcome.events;
        return Ok(combat);
    }
    execute_stationary_attack(state, player, unit_id, ai, origin, prepared_target, draws)
}

/// A unit target that is visible to the acting team in the command input state.
///
/// Movement can change visibility. It cannot change which enemy identifier the
/// player can submit. Carry this result into the movement state. Combat can
/// then use the resolved destination without a second visibility decision.
#[derive(Clone, Copy, Debug)]
struct DisclosedUnitTarget(UnitId);

#[derive(Clone, Copy, Debug)]
enum PreparedAttackTarget {
    Unit(DisclosedUnitTarget),
    Tile(Pos),
}

fn prepare_attack_target(
    state: &State,
    actor_team: &crate::semantic::TeamId,
    view: &AwbwView<'_>,
    target: AttackTarget,
) -> Result<PreparedAttackTarget, ExecuteError> {
    match target {
        AttackTarget::Unit { unit } => Ok(PreparedAttackTarget::Unit(disclose_unit_target(
            state, actor_team, view, unit,
        )?)),
        AttackTarget::Tile { position } => Ok(PreparedAttackTarget::Tile(position)),
    }
}

fn disclose_unit_target(
    state: &State,
    actor_team: &crate::semantic::TeamId,
    view: &AwbwView<'_>,
    target_id: UnitId,
) -> Result<DisclosedUnitTarget, ExecuteError> {
    let invalid = || {
        violation(Violation::InvalidTarget {
            target: Some(target_id.into()),
        })
    };
    let defender = state.units.get(target_id).ok_or_else(invalid)?;
    let defender_team = state
        .players
        .get(defender.owner.get())
        .map(|candidate| &candidate.team)
        .ok_or_else(|| ExecuteError::InvalidState("target owner is absent".into()))?;
    if defender_team == actor_team
        || !matches!(defender.location, Location::Board { .. })
        || !view.unit(defender)
    {
        return Err(invalid());
    }
    Ok(DisclosedUnitTarget(target_id))
}

/// Resolve an attack after movement validation has established the attacker.
///
/// Move-and-attack reaches this with a derived state, so it cannot reuse the
/// [`Turn`] tied to the command's input state. The movement reducer is
/// the only caller on that path and preserves the active-turn invariants.
fn execute_stationary_attack(
    state: &State,
    player: &PlayerId,
    unit_id: UnitId,
    ai: usize,
    origin: Pos,
    target: PreparedAttackTarget,
    draws: &mut Draws<'_>,
) -> Result<Execution, ExecuteError> {
    let holdings = Holdings::tally(state);
    let disclosed = match target {
        PreparedAttackTarget::Unit(disclosed) => disclosed,
        PreparedAttackTarget::Tile(position) => {
            return execute_tile_attack(state, &holdings, player, unit_id, ai, origin, position);
        }
    };
    let engagement = Engagement::open(state, &holdings, ai, origin, disclosed)?;
    let counter_first = engagement.counter_comes_first();
    let scored = engagement.scored()?;
    if counter_first {
        resolve_counter_first(&scored, draws)
    } else {
        resolve_exchange(&scored, draws)
    }
}

/// A validated unit-versus-unit engagement.
///
/// Opening one establishes everything the exchange needs and nothing about the
/// order it happens in: both fighters, their indices in the authoritative
/// state, and whether the defender can answer at all. Which side fires first is
/// then a question the commander layer answers, not a branch that re-derives
/// the engagement.
///
/// This is a proof of legality and holds no numbers. Enumeration opens one per
/// candidate target only to ask whether the attack is allowed, and scoring the
/// strike is the expensive half: two commander combat resolutions and a board
/// lookup per side. Ask [`Engagement::scored`] for the numbers, which is also
/// the only place the commander algebra can overflow.
struct Engagement<'a> {
    state: &'a State,
    holdings: &'a Holdings<'a>,
    attacker: Fighter<'a>,
    defender: Fighter<'a>,
    attacker_index: usize,
    defender_index: usize,
    /// Adjacent, direct-fire, and holding a weapon that bites — the three
    /// conditions a counter needs before a commander is consulted.
    counter_armed: bool,
}

/// An [`Engagement`] with the initiating strike scored.
///
/// Everything that resolves or forecasts a shot needs these numbers; nothing
/// that only validates one does.
struct ScoredEngagement<'a> {
    engagement: Engagement<'a>,
    initial: StrikeValues,
}

impl<'a> Engagement<'a> {
    /// Check the target and score the initiating strike.
    fn open(
        state: &'a State,
        holdings: &'a Holdings<'a>,
        attacker_index: usize,
        origin: Pos,
        disclosed: DisclosedUnitTarget,
    ) -> Result<Self, ExecuteError> {
        let target_id = disclosed.0;
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

        let attacker = Fighter::new(state, attacker, origin);
        let defender = Fighter::new(state, defender, defender_position);
        Ok(Self {
            state,
            holdings,
            attacker,
            defender,
            attacker_index,
            defender_index,
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

    /// Score the initiating strike.
    fn scored(self) -> Result<ScoredEngagement<'a>, ExecuteError> {
        let initial = resolve_strike(
            self.state,
            self.holdings,
            self.attacker,
            self.defender,
            Strike::Initial,
        )?;
        Ok(ScoredEngagement {
            engagement: self,
            initial,
        })
    }

    /// The numbers the defender's counter is scored with.
    fn counter_values(&self) -> Result<StrikeValues, ExecuteError> {
        resolve_strike(
            self.state,
            self.holdings,
            self.defender,
            self.attacker,
            Strike::Counter,
        )
    }

    /// Whether the defender's commander turns the exchange around and fires
    /// before the strike that provoked it.
    fn counter_comes_first(&self) -> bool {
        let defender = self.defender.unit;
        self.counter_armed
            && commander::counter_first(
                self.state,
                self.defender.seat,
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

/// Break out a counter at each health the defender may be left standing at.
///
/// The aggregate range and the steps use the same counter values. If the
/// strongest roll destroys the defender, both readings use one health point
/// as the least survivor.
fn counter_steps(
    values: &StrikeValues,
    attacker: &Unit,
    defender: &Unit,
    least: u8,
    most: u8,
    worst: i64,
    best: i64,
) -> Vec<CounterStep> {
    let mut steps = Vec::new();
    for bar in bars_of(least)..=bars_of(most) {
        // The healths inside this bar that the exchange can actually leave.
        let top = (bar * HEALTH_STEP).min(most);
        let bottom = ((bar - 1) * HEALTH_STEP + 1).max(least);
        if bottom > top {
            continue;
        }
        steps.push(CounterStep {
            target_hp: top,
            counter: DamageRange {
                // Least of it standing, worst of its luck; most of it
                // standing, best of its luck.
                low: values
                    .scored(defender, bottom, attacker, attacker.hp, worst)
                    .1,
                high: values.scored(defender, top, attacker, attacker.hp, best).1,
            },
        });
    }
    steps
}

/// The bar the board draws a health in, which is the one a player reads it by.
fn bars_of(points: u8) -> u8 {
    points.div_ceil(HEALTH_STEP).clamp(1, 10)
}

/// What an exchange would cost both sides, without drawing a single roll.
///
/// This is [`resolve_exchange`] and [`resolve_counter_first`] scored twice —
/// once with the luck of every commander involved at its worst for the striker
/// and once at its best — and it opens the same [`Engagement`] they do. That is
/// the point rather than a convenience: counter eligibility, effective attack
/// range, concealment compatibility, both commanders' attack and defense
/// modifiers, effective enemy terrain stars and the `counter-first` inversion
/// all live in the engagement, and a forecast that recomputed any of them would
/// be a second combat model free to drift from the one that resolves the shot.
///
/// The error cases are the reducer's own: an out-of-range, invisible, or
/// unarmed engagement reports the violation it would report at execution time,
/// so a caller cannot forecast an attack that could not be made.
pub(crate) fn forecast_unit_attack(
    state: &State,
    holdings: &Holdings<'_>,
    player: &PlayerId,
    attacker_index: usize,
    origin: Pos,
    target_id: UnitId,
) -> Result<Forecast, ExecuteError> {
    let actor_team = state
        .find_player(player)
        .map(|candidate| &candidate.team)
        .ok_or_else(|| ExecuteError::InvalidState("active player is absent".into()))?;
    let view = AwbwView::new(state, actor_team);
    let disclosed = disclose_unit_target(state, actor_team, &view, target_id)?;
    let scored = Engagement::open(state, holdings, attacker_index, origin, disclosed)?.scored()?;
    let engagement = &scored.engagement;
    let attacker = engagement.attacker.unit;
    let defender = engagement.defender.unit;
    let initial = &scored.initial;
    let (attack_worst, attack_best) = initial.luck_bounds();

    if engagement.counter_comes_first() {
        // The pre-emptive strike lands at the defender's full health, and what
        // it leaves is what the attacker fires from. So the attacker's weakest
        // roll is the one paired with the reply that hurt most.
        let values = engagement.counter_values()?;
        let (counter_worst, counter_best) = values.luck_bounds();
        let (counter_low, counter_low_raw) =
            values.scored(defender, defender.hp, attacker, attacker.hp, counter_worst);
        let (counter_high, counter_high_raw) =
            values.scored(defender, defender.hp, attacker, attacker.hp, counter_best);
        let (_, attack_low_raw) = initial.scored(
            attacker,
            attacker.hp.saturating_sub(counter_high),
            defender,
            defender.hp,
            attack_worst,
        );
        let (_, attack_high_raw) = initial.scored(
            attacker,
            attacker.hp.saturating_sub(counter_low),
            defender,
            defender.hp,
            attack_best,
        );
        return Ok(Forecast {
            attack: DamageRange {
                low: attack_low_raw,
                high: attack_high_raw,
            },
            counter: Some(DamageRange {
                low: counter_low_raw,
                high: counter_high_raw,
            }),
            counter_steps: Vec::new(),
            counter_first: true,
            attacker_hp: attacker.hp,
            target_hp: defender.hp,
        });
    }

    let (attack_low, attack_low_raw) =
        initial.scored(attacker, attacker.hp, defender, defender.hp, attack_worst);
    let (attack_high, attack_high_raw) =
        initial.scored(attacker, attacker.hp, defender, defender.hp, attack_best);
    // Nothing answers when nothing is armed to, and nothing survives to answer
    // when even the weakest roll finishes the defender.
    let (counter, counter_steps) =
        if !engagement.counter_armed || defender.hp.saturating_sub(attack_low) == 0 {
            (None, Vec::new())
        } else {
            let values = engagement.counter_values()?;
            let (worst, best) = values.luck_bounds();
            let least = defender.hp.saturating_sub(attack_high);
            let most = defender.hp.saturating_sub(attack_low);
            // The luckiest strike can still destroy a defender the weakest one
            // leaves standing. Then the reply may not be made at all, and the
            // floor of what the attacker can take is nothing. The rungs speak
            // only for the healths the defender is alive at, so they start one
            // point up.
            let counter = DamageRange {
                low: if least == 0 {
                    0
                } else {
                    values
                        .scored(defender, least, attacker, attacker.hp, worst)
                        .1
                },
                high: values.scored(defender, most, attacker, attacker.hp, best).1,
            };
            let steps = counter_steps(&values, attacker, defender, least.max(1), most, worst, best);
            (Some(counter), steps)
        };

    Ok(Forecast {
        attack: DamageRange {
            low: attack_low_raw,
            high: attack_high_raw,
        },
        counter,
        counter_steps,
        counter_first: false,
        attacker_hp: attacker.hp,
        target_hp: defender.hp,
    })
}

/// What one strike against a destructible tile would take off it.
///
/// A pipe seam has no commander, no luck and no reply, so both ends of the
/// range are the same number and the counter is always absent. `None` means the
/// tile is not destructible or the attacker holds nothing that bites it.
pub(crate) fn forecast_tile_attack(
    state: &State,
    holdings: &Holdings<'_>,
    player: &PlayerId,
    attacker: &Unit,
    origin: Pos,
    position: Pos,
) -> Result<Option<Forecast>, ExecuteError> {
    let view = attack_view(state, player)?;
    let target = match validate_tile_attack(state, attacker, origin, position, &view) {
        Ok(target) => target,
        Err(ExecuteError::Violation(_)) => return Ok(None),
        Err(error) => return Err(error),
    };
    let Some(hit) = score_tile_strike(
        state,
        holdings,
        player,
        attacker,
        origin,
        target.kind,
        target.hp,
    )?
    else {
        return Ok(None);
    };
    Ok(Some(Forecast {
        attack: DamageRange {
            low: hit.raw_damage,
            high: hit.raw_damage,
        },
        counter: None,
        counter_steps: Vec::new(),
        counter_first: false,
        attacker_hp: attacker.hp,
        target_hp: target.hp,
    }))
}

/// The ordinary exchange: the initiating strike lands, then the defender
/// answers if it survived and can.
fn resolve_exchange(
    scored: &ScoredEngagement<'_>,
    draws: &mut Draws<'_>,
) -> Result<Execution, ExecuteError> {
    let engagement = &scored.engagement;
    let state = engagement.state;
    let attacker = engagement.attacker.unit;
    let defender = engagement.defender.unit;
    let attacker_index = engagement.attacker_index;
    let defender_index = engagement.defender_index;

    let attack_luck = scored.initial.luck(draws)?;
    let first = combat::damage(
        scored.initial.striker_side(attacker, attacker.hp),
        scored.initial.target_side(defender, defender.hp),
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
            remove_unit_and_cargo(
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
        remove_unit_and_cargo(&mut next, defender.id, KnownReason::Combat, &mut events);
    }
    if !attacker_removed {
        spend_attacker(&mut next, &mut events, attacker.id);
    } else {
        rout_if_last_unit(&mut next, attacker.owner, &mut events)?;
    }
    if defender_remaining == 0 {
        rout_if_last_unit(&mut next, defender.owner, &mut events)?;
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
    scored: &ScoredEngagement<'_>,
    draws: &mut Draws<'_>,
) -> Result<Execution, ExecuteError> {
    let engagement = &scored.engagement;
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
        let attack_luck = scored.initial.luck(draws)?;
        Some(
            combat::damage(
                scored.initial.striker_side(attacker, attacker_remaining),
                scored.initial.target_side(defender, defender.hp),
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
        remove_unit_and_cargo(
            &mut next,
            attacker.id,
            KnownReason::CombatCounter,
            &mut events,
        );
        rout_if_last_unit(&mut next, attacker.owner, &mut events)?;
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
        remove_unit_and_cargo(&mut next, defender.id, KnownReason::Combat, &mut events);
    } else {
        let defender_index = next
            .units
            .index_of(defender.id)
            .expect("the target remains present until it is removed");
        next.units[defender_index].hp = defender_remaining;
    }
    spend_attacker(&mut next, &mut events, attacker.id);
    if defender_remaining == 0 {
        rout_if_last_unit(&mut next, defender.owner, &mut events)?;
    }
    Ok(Execution {
        state: next,
        events,
        random_consumed: draws.drawn(),
    })
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
    owner: PlayerIdx,
    events: &mut Vec<Event>,
) -> Result<(), ExecuteError> {
    if next.units.iter().any(|unit| unit.owner == owner) {
        return Ok(());
    }
    let owner = next
        .try_player_id(owner)
        .ok_or_else(|| ExecuteError::InvalidState("routed owner is absent".into()))?
        .clone();
    eliminate_player(
        next,
        &owner,
        ruleset::VictoryReason::Rout,
        None,
        None,
        events,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_forecasts_reject_targets_that_execution_rejects() {
        for fixture in ["tile-invalid-targets.json", "tile-occupied-seam.json"] {
            let text = std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../spec/fixtures/combat")
                    .join(fixture),
            )
            .expect("read combat fixture");
            let case: serde_json::Value = serde_json::from_str(&text).expect("parse fixture");
            let state: State =
                serde_json::from_value(case["initial_state"].clone()).expect("parse state");

            for step in case["steps"].as_array().expect("fixture steps") {
                let command: Command =
                    serde_json::from_value(step["command"].clone()).expect("parse command");
                let Command::MoveAttack {
                    player,
                    unit,
                    path,
                    target: AttackTarget::Tile { position },
                } = command
                else {
                    continue;
                };
                let attacker = state.units.get(unit).expect("attacker exists");
                let origin = *path.last().expect("attack path has an origin");

                assert_eq!(
                    forecast_tile_attack(
                        &state,
                        &Holdings::tally(&state),
                        &player,
                        attacker,
                        origin,
                        position
                    )
                    .unwrap(),
                    None,
                    "{fixture} forecasted a reducer-invalid tile"
                );
            }
        }
    }
}
