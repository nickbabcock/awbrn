//! Revisioned, commander-neutral effective-value queries.
//!
//! This module is the only place that interprets commander profile operators.
//! Transition reducers provide context; they never branch on commander names.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::ruleset::{PropertyKind, Terrain, UnitKind};
use crate::semantic::{CommanderId as CommanderKind, Player, PowerState, State, Unit, WeatherKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strike {
    Initial,
    Counter,
}

/// One side of an engagement, described in the vocabulary
/// `commander-combat.json` predicates are written in.
///
/// `domain` stays a string because that vocabulary is finer than the one
/// `units.json` defines: it separates foot soldiers and transports from other
/// ground units.
#[derive(Clone, Copy, Debug)]
pub struct Combatant<'a> {
    pub kind: UnitKind,
    pub domain: &'a str,
    pub fire_mode: &'a str,
    pub terrain: Terrain,
    pub weather: WeatherKind,
    pub property: bool,
    pub capabilities: &'a HashSet<String>,
}

#[derive(Clone, Copy, Debug)]
pub struct CombatContext {
    pub tower_count: i64,
    pub funds: u64,
    pub owned_properties: u64,
    pub base_terrain_stars: i64,
}

#[derive(Clone, Debug, Deserialize)]
struct CombatTable {
    generic_power_bonus: PowerBonuses,
    commanders: HashMap<String, CombatProfile>,
}

#[derive(Clone, Debug, Deserialize)]
struct PowerBonuses {
    cop: Bonus,
    scop: Bonus,
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct Bonus {
    attack: i64,
    defense: i64,
}

#[derive(Clone, Debug, Deserialize)]
struct CombatProfile {
    day_to_day: RuleState,
    cop: RuleState,
    scop: RuleState,
}

#[derive(Clone, Debug, Deserialize)]
struct RuleState {
    rules: Vec<Rule>,
}

#[derive(Clone, Debug, Deserialize)]
struct Rule {
    when: Predicate,
    effect: Effect,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct Predicate {
    #[serde(default)]
    unit_kinds: Vec<String>,
    #[serde(default)]
    capabilities_all: Vec<String>,
    #[serde(default)]
    domains: Vec<String>,
    #[serde(default)]
    fire_modes: Vec<String>,
    #[serde(default)]
    terrain_kinds: Vec<String>,
    #[serde(default)]
    weather_kinds: Vec<String>,
    property: Option<bool>,
    counterattack: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "operator", rename_all = "kebab-case")]
enum Effect {
    AttackAdd { value: i64 },
    DefenseAdd { value: i64 },
    TerrainStarsAdd { value: i64 },
    TerrainStarsMultiply { value: i64 },
    EnemyTerrainStarsAdd { value: i64 },
    CounterFirst,
    TowerAttackMultiply { value: i64 },
    TowerDefenseMultiply { value: i64 },
    AttackAddFundsDivide { divisor: u64 },
    AttackAddOwnedPropertiesMultiply { value: i64 },
    AttackAddTerrainStarsMultiply { value: i64 },
    CounterAttackMultiply { numerator: i64, denominator: i64 },
    GoodLuckSet { domain: Domain },
    BadLuckSet { domain: Domain },
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub struct Domain {
    pub minimum: i64,
    pub maximum: i64,
}

#[derive(Clone, Debug, Deserialize)]
struct ProfileTable {
    commanders: HashMap<String, EffectiveProfile>,
}

#[derive(Clone, Debug, Deserialize)]
struct PowerTable {
    base_star_charge: u64,
    use_cost_scaling: UseCostScaling,
    commanders: HashMap<String, PowerProfile>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct UseCostScaling {
    numerator: u64,
    denominator: u64,
    maximum_uses: u64,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct PowerProfile {
    cop: Option<PowerDefinition>,
    scop: Option<PowerDefinition>,
}

#[derive(Clone, Debug, Deserialize)]
struct PowerDefinition {
    stars: u64,
    instant_effects: Vec<InstantEffect>,
    #[serde(default)]
    strike_effects: Vec<StrikeEffect>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "operator", rename_all = "kebab-case")]
pub(crate) enum InstantEffect {
    HealVisualHp {
        target: UnitTarget,
        amount: u8,
    },
    HealExactHp {
        target: UnitTarget,
        amount: u8,
    },
    DamageExactHp {
        target: UnitTarget,
        amount: u8,
        minimum_hp: u8,
    },
    SetWeather {
        kind: WeatherEffectKind,
        duration: WeatherDuration,
    },
    DrainCurrentFuelRatio {
        target: UnitTarget,
        numerator: u64,
        denominator: u64,
    },
    FireAreaStrikes {
        target: UnitTarget,
        radius: usize,
        damage: u8,
        minimum_hp: u8,
        selection_policies: Vec<AreaStrikePolicy>,
        friendly_contribution: FriendlyContribution,
    },
    ReducePowerChargeByFundsRatio {
        target: CommanderSlotTarget,
        funds_per_full_bar: u64,
    },
    RefreshUnitAction {
        target: UnitTarget,
        #[serde(default)]
        exclude_unit_kinds: Vec<UnitKind>,
    },
    ResupplyUnits {
        target: UnitTarget,
    },
    SpawnUnitsOnOwnedProperties {
        target: PropertyTarget,
        property_kinds: Vec<PropertyKind>,
        unit_kind: UnitKind,
        hp: u8,
        resources: SpawnResources,
        action: SpawnAction,
        concealment: SpawnConcealment,
        occupied_tiles: OccupiedTileHandling,
        order: PropertyOrder,
        unit_limit: SpawnUnitLimit,
    },
    FireTargetedAreaStrike {
        target: AreaStrikeCenterTarget,
        radius: usize,
        damage: u8,
        minimum_hp: u8,
        selection_policy: TargetedAreaStrikePolicy,
        friendly_contribution: FriendlyContribution,
        unit_value: TargetedUnitValue,
    },
    FireImmobilizingAreaStrike {
        target: UnitTarget,
        radius: usize,
        damage: u8,
        minimum_hp: u8,
        selection_policy: TargetedAreaStrikePolicy,
        friendly_contribution: FriendlyContribution,
        unit_value: TargetedUnitValue,
        duration: ImmobilizationDuration,
    },
    MultiplyFundsRatio {
        target: PlayerTarget,
        numerator: u64,
        denominator: u64,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "operator", rename_all = "kebab-case")]
enum StrikeEffect {
    GainFundsFromVisualHpDamage {
        target: StrikeEffectTarget,
        numerator: u64,
        denominator: u64,
        unit_value: UnitValue,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum StrikeEffectTarget {
    EnemyUnit,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum UnitValue {
    EffectiveBuildCost,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub(crate) enum UnitTarget {
    #[serde(rename = "owned-units")]
    Owned,
    #[serde(rename = "enemy-units")]
    Enemy,
    #[serde(rename = "enemy-units-on-properties")]
    EnemyOnProperties,
    #[serde(rename = "all-board-units")]
    AllBoard,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum PropertyTarget {
    OwnedProperties,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SpawnResources {
    UnitMaxima,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SpawnAction {
    Ready,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SpawnConcealment {
    Exposed,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum OccupiedTileHandling {
    Skip,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum PropertyOrder {
    YThenX,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SpawnUnitLimit {
    Settings,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AreaStrikePolicy {
    InfantryHp,
    UnitValue,
    UnitHp,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum FriendlyContribution {
    Subtract,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum AreaStrikeCenterTarget {
    EnemyUnitCenters,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum TargetedAreaStrikePolicy {
    UnitValue,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum TargetedUnitValue {
    BaseBuildCost,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CommanderSlotTarget {
    EnemyCommanderSlots,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum WeatherEffectKind {
    Clear,
    Rain,
    Snow,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum WeatherDuration {
    UntilOwnerNextTurn,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ImmobilizationDuration {
    ThroughTargetNextTurn,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum PlayerTarget {
    ActivatingPlayer,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PowerLevel {
    Cop,
    Scop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PowerActivationError {
    #[error("ArithmeticOverflow")]
    ArithmeticOverflow,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PowerActivation {
    pub cost: u64,
    pub instant_effects: Vec<InstantEffect>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct EffectiveProfile {
    #[serde(default)]
    movement: StateValues,
    #[serde(default)]
    movement_cost: MovementCostStates,
    #[serde(default)]
    vision: StateValues,
    #[serde(default)]
    reveals_concealing_terrain: BooleanStates,
    #[serde(default)]
    attack_range: StateValues,
    #[serde(default)]
    build_cost: RationalStates,
    #[serde(default)]
    production: ProductionStates,
    #[serde(default)]
    capture: CaptureStates,
    #[serde(default)]
    income_per_property_add: i64,
    #[serde(default)]
    repair_bars_add: u64,
    #[serde(default)]
    air_upkeep_add: i64,
    #[serde(default)]
    ignores_snow_movement: bool,
    #[serde(default)]
    ignores_rain_movement: bool,
    #[serde(default)]
    rain_movement_as_snow: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct StateValues {
    #[serde(default)]
    day_to_day: Vec<ValueRule>,
    #[serde(default)]
    cop: Vec<ValueRule>,
    #[serde(default)]
    scop: Vec<ValueRule>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct MovementCostStates {
    #[serde(default)]
    day_to_day: Vec<MovementCostRule>,
    #[serde(default)]
    cop: Vec<MovementCostRule>,
    #[serde(default)]
    scop: Vec<MovementCostRule>,
}

#[derive(Clone, Debug, Deserialize)]
struct MovementCostRule {
    operator: MovementCostOperator,
    value: u64,
    #[serde(default)]
    except_weather_kinds: Vec<WeatherKind>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum MovementCostOperator {
    TraversableCostSet,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
struct BooleanStates {
    day_to_day: Option<bool>,
    cop: Option<bool>,
    scop: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct ValueRule {
    #[serde(default)]
    unit_kinds: Vec<String>,
    #[serde(default)]
    domains: Vec<String>,
    #[serde(default)]
    fire_modes: Vec<String>,
    add: i64,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
struct Rational {
    numerator: u64,
    denominator: u64,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct RationalStates {
    day_to_day: Option<Rational>,
    cop: Option<Rational>,
    scop: Option<Rational>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct ProductionRule {
    terrain_kinds: Vec<String>,
    domains: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct ProductionStates {
    #[serde(default)]
    day_to_day: Vec<ProductionRule>,
    #[serde(default)]
    cop: Vec<ProductionRule>,
    #[serde(default)]
    scop: Vec<ProductionRule>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct CaptureEffect {
    numerator: u64,
    denominator: u64,
    instant: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct CaptureStates {
    day_to_day: Option<CaptureEffect>,
    cop: Option<CaptureEffect>,
    scop: Option<CaptureEffect>,
}

// The commander documents describe rule *programs* — predicates paired with
// operators — rather than the dense value tables `xtask-ruleset` lowers. They
// stay parsed structures, but parsed once for the process rather than once per
// query: the reducer consults them several times per command, and re-parsing
// them there was the bulk of the cost of executing one.

fn combat_table() -> &'static CombatTable {
    static TABLE: LazyLock<CombatTable> = LazyLock::new(|| {
        serde_json::from_str(include_str!(
            "../../../spec/rulesets/awbw/2026-07-10/commander-combat.json"
        ))
        .expect("embedded commander combat table")
    });
    &TABLE
}

fn profile_table() -> &'static ProfileTable {
    static TABLE: LazyLock<ProfileTable> = LazyLock::new(|| {
        serde_json::from_str(include_str!(
            "../../../spec/rulesets/awbw/2026-07-10/commander-profiles.json"
        ))
        .expect("embedded commander profile table")
    });
    &TABLE
}

fn power_table() -> &'static PowerTable {
    static TABLE: LazyLock<PowerTable> = LazyLock::new(|| {
        serde_json::from_str(include_str!(
            "../../../spec/rulesets/awbw/2026-07-10/commander-powers.json"
        ))
        .expect("embedded commander power table")
    });
    &TABLE
}

pub(crate) fn power_activation(
    commander: CommanderKind,
    level: PowerLevel,
    power_uses: u64,
) -> Result<Option<PowerActivation>, PowerActivationError> {
    let table = power_table();
    let Some(profile) = table.commanders.get(commander.as_str()) else {
        return Ok(None);
    };
    let definition = match level {
        PowerLevel::Cop => profile.cop.as_ref(),
        PowerLevel::Scop => profile.scop.as_ref(),
    };
    let Some(definition) = definition else {
        return Ok(None);
    };
    let cost = scaled_power_charge(table, definition.stars, power_uses)?;
    Ok(Some(PowerActivation {
        cost,
        instant_effects: definition.instant_effects.clone(),
    }))
}

fn scaled_power_charge(
    table: &PowerTable,
    stars: u64,
    power_uses: u64,
) -> Result<u64, PowerActivationError> {
    let scaling = table.use_cost_scaling;
    let uses = power_uses.min(scaling.maximum_uses);
    let scaled_numerator = scaling
        .denominator
        .checked_add(
            scaling
                .numerator
                .checked_mul(uses)
                .ok_or(PowerActivationError::ArithmeticOverflow)?,
        )
        .ok_or(PowerActivationError::ArithmeticOverflow)?;
    table
        .base_star_charge
        .checked_mul(stars)
        .and_then(|value| value.checked_mul(scaled_numerator))
        .and_then(|value| value.checked_div(scaling.denominator))
        .ok_or(PowerActivationError::ArithmeticOverflow)
}

pub(crate) fn maximum_power_charge(
    commander: CommanderKind,
    power_uses: u64,
) -> Result<Option<u64>, PowerActivationError> {
    let table = power_table();
    let Some(profile) = table.commanders.get(commander.as_str()) else {
        return Ok(None);
    };
    let Some(stars) = profile
        .cop
        .iter()
        .chain(profile.scop.iter())
        .map(|power| power.stars)
        .max()
    else {
        return Ok(None);
    };
    scaled_power_charge(table, stars, power_uses).map(Some)
}

pub(crate) fn strike_funds_gain(
    state: &State,
    player: &str,
    target_owner: &str,
    from_hp: u8,
    to_hp: u8,
    target_value: u64,
) -> Option<u64> {
    let (actor, commander, power) = active(state, player)?;
    let target = state
        .players
        .iter()
        .find(|candidate| candidate.id == target_owner)?;
    if actor.team == target.team {
        return Some(0);
    }
    if matches!(power, Power::None) {
        return Some(0);
    }
    let table = power_table();
    let Some(profile) = table.commanders.get(commander.as_str()) else {
        return Some(0);
    };
    let definition = match power {
        Power::None => unreachable!("none returned above"),
        Power::Cop => match profile.cop.as_ref() {
            Some(definition) => definition,
            None => return Some(0),
        },
        Power::Scop => match profile.scop.as_ref() {
            Some(definition) => definition,
            None => return Some(0),
        },
    };
    let visual_before = u64::from(from_hp).div_ceil(10);
    let visual_after = u64::from(to_hp).div_ceil(10);
    let visual_damage = visual_before.saturating_sub(visual_after);
    definition
        .strike_effects
        .iter()
        .try_fold(0_u64, |total, effect| {
            let StrikeEffect::GainFundsFromVisualHpDamage {
                target: StrikeEffectTarget::EnemyUnit,
                numerator,
                denominator,
                unit_value: UnitValue::EffectiveBuildCost,
            } = effect;
            visual_damage
                .checked_mul(target_value)?
                .checked_mul(*numerator)?
                .checked_div(10_u64.checked_mul(*denominator)?)?
                .checked_add(total)
        })
}

fn active<'a>(state: &'a State, player_id: &str) -> Option<(&'a Player, CommanderKind, Power)> {
    let player = state.players.iter().find(|player| player.id == player_id)?;
    let (slot, power) = match player.power_state {
        PowerState::None => (
            player
                .commanders
                .iter()
                .position(|commander| commander.active)?,
            Power::None,
        ),
        PowerState::Cop { commander_slot } => (usize::from(commander_slot), Power::Cop),
        PowerState::Scop { commander_slot } => (usize::from(commander_slot), Power::Scop),
    };
    let commander = player.commanders.get(slot)?;
    commander.active.then_some((player, commander.id, power))
}

#[derive(Clone, Copy)]
enum Power {
    None,
    Cop,
    Scop,
}

fn predicate_matches(predicate: &Predicate, unit: Combatant<'_>, strike: Strike) -> bool {
    (predicate.unit_kinds.is_empty()
        || predicate.unit_kinds.iter().any(|v| v == unit.kind.as_str()))
        && (predicate.domains.is_empty() || predicate.domains.iter().any(|v| v == unit.domain))
        && (predicate.fire_modes.is_empty()
            || predicate.fire_modes.iter().any(|v| v == unit.fire_mode))
        && (predicate.terrain_kinds.is_empty()
            || predicate
                .terrain_kinds
                .iter()
                .any(|v| v == unit.terrain.as_str()))
        && (predicate.weather_kinds.is_empty()
            || predicate
                .weather_kinds
                .iter()
                .any(|v| v == unit.weather.as_str()))
        && predicate
            .property
            .is_none_or(|value| value == unit.property)
        && predicate
            .capabilities_all
            .iter()
            .all(|value| unit.capabilities.contains(value))
        && predicate
            .counterattack
            .is_none_or(|value| value == (strike == Strike::Counter))
}

fn applicable_rules(profile: &CombatProfile, power: Power) -> impl Iterator<Item = &Rule> {
    profile.day_to_day.rules.iter().chain(match power {
        Power::None => [].iter(),
        Power::Cop => profile.cop.rules.iter(),
        Power::Scop => profile.scop.rules.iter(),
    })
}

pub fn effective_combat(
    state: &State,
    owner: &str,
    unit: Combatant<'_>,
    strike: Strike,
    context: CombatContext,
) -> Option<(i64, i64, i64, Domain, Domain)> {
    let table = combat_table();
    let Some((_, commander, power)) = active(state, owner) else {
        return Some((
            100,
            100,
            context.base_terrain_stars,
            Domain {
                minimum: 0,
                maximum: 9,
            },
            Domain {
                minimum: 0,
                maximum: 0,
            },
        ));
    };
    let Some(profile) = table.commanders.get(commander.as_str()) else {
        return Some((
            100,
            100,
            context.base_terrain_stars,
            Domain {
                minimum: 0,
                maximum: 9,
            },
            Domain {
                minimum: 0,
                maximum: 0,
            },
        ));
    };
    let mut attack: i64 = 100;
    let mut defense: i64 = 100;
    let mut stars = context.base_terrain_stars;
    let mut good = Domain {
        minimum: 0,
        maximum: 9,
    };
    let mut bad = Domain {
        minimum: 0,
        maximum: 0,
    };
    let bonus = match power {
        Power::None => None,
        Power::Cop => Some(table.generic_power_bonus.cop),
        Power::Scop => Some(table.generic_power_bonus.scop),
    };
    let mut tower_attack_multiplier = 1;
    let mut tower_defense_multiplier = 0;
    for rule in applicable_rules(profile, power) {
        if !predicate_matches(&rule.when, unit, strike) {
            continue;
        }
        match rule.effect {
            Effect::AttackAdd { value } => attack = attack.checked_add(value)?,
            Effect::DefenseAdd { value } => defense = defense.checked_add(value)?,
            Effect::TerrainStarsAdd { value } => stars = stars.checked_add(value)?,
            Effect::TerrainStarsMultiply { value } => stars = stars.checked_mul(value)?,
            Effect::EnemyTerrainStarsAdd { .. } => {}
            Effect::CounterFirst => {}
            Effect::TowerAttackMultiply { value } => tower_attack_multiplier = value,
            Effect::TowerDefenseMultiply { value } => tower_defense_multiplier = value,
            Effect::AttackAddFundsDivide { divisor } => {
                attack = attack.checked_add(i64::try_from(context.funds / divisor).ok()?)?
            }
            Effect::AttackAddOwnedPropertiesMultiply { value } => {
                let count = i64::try_from(context.owned_properties).ok()?;
                attack = attack.checked_add(count.checked_mul(value)?)?
            }
            Effect::AttackAddTerrainStarsMultiply { value } => {
                attack = attack.checked_add(context.base_terrain_stars.checked_mul(value)?)?
            }
            Effect::CounterAttackMultiply { .. } => {}
            Effect::GoodLuckSet { domain } => good = domain,
            Effect::BadLuckSet { domain } => bad = domain,
        }
    }
    if let Some(bonus) = bonus {
        attack = attack.checked_add(bonus.attack)?;
        defense = defense.checked_add(bonus.defense)?;
    }
    let tower_attack = 10_i64
        .checked_mul(context.tower_count)?
        .checked_mul(tower_attack_multiplier)?;
    let tower_defense = 10_i64
        .checked_mul(context.tower_count)?
        .checked_mul(tower_defense_multiplier)?;
    attack = attack.checked_add(tower_attack)?;
    defense = defense.checked_add(tower_defense)?;
    for rule in applicable_rules(profile, power) {
        if predicate_matches(&rule.when, unit, strike)
            && let Effect::CounterAttackMultiply {
                numerator,
                denominator,
            } = rule.effect
        {
            attack = attack.checked_mul(numerator)?.checked_div(denominator)?;
        }
    }
    Some((attack.max(0), defense.max(0), stars.max(0), good, bad))
}

pub fn effective_enemy_terrain_stars(
    state: &State,
    owner: &str,
    unit: Combatant<'_>,
    strike: Strike,
    base: i64,
) -> Option<i64> {
    let table = combat_table();
    let Some((_, commander, power)) = active(state, owner) else {
        return Some(base.max(0));
    };
    let Some(profile) = table.commanders.get(commander.as_str()) else {
        return Some(base.max(0));
    };
    let mut stars = base;
    for rule in applicable_rules(profile, power) {
        if predicate_matches(&rule.when, unit, strike)
            && let Effect::EnemyTerrainStarsAdd { value } = rule.effect
        {
            stars = stars.checked_add(value)?;
        }
    }
    Some(stars.max(0))
}

pub fn counter_first(state: &State, owner: &str, unit: Combatant<'_>, strike: Strike) -> bool {
    let table = combat_table();
    let Some((_, commander, power)) = active(state, owner) else {
        return false;
    };
    table
        .commanders
        .get(commander.as_str())
        .is_some_and(|profile| {
            applicable_rules(profile, power).any(|rule| {
                predicate_matches(&rule.when, unit, strike)
                    && matches!(rule.effect, Effect::CounterFirst)
            })
        })
}

fn value_add(
    rules: &StateValues,
    power: Power,
    kind: UnitKind,
    domain: &str,
    fire_mode: &str,
) -> i64 {
    rules
        .day_to_day
        .iter()
        .chain(match power {
            Power::None => [].iter(),
            Power::Cop => rules.cop.iter(),
            Power::Scop => rules.scop.iter(),
        })
        .filter(|rule| {
            (rule.unit_kinds.is_empty() || rule.unit_kinds.iter().any(|v| v == kind.as_str()))
                && (rule.domains.is_empty() || rule.domains.iter().any(|v| v == domain))
                && (rule.fire_modes.is_empty() || rule.fire_modes.iter().any(|v| v == fire_mode))
        })
        .map(|rule| rule.add)
        .sum()
}

fn effective_profile(state: &State, owner: &str) -> Option<(&'static EffectiveProfile, Power)> {
    let (_, commander, power) = active(state, owner)?;
    Some((profile_table().commanders.get(commander.as_str())?, power))
}

pub fn effective_move(state: &State, unit: &Unit, base: u64, domain: &str) -> u64 {
    let Some((profile, power)) = effective_profile(state, &unit.owner) else {
        return base;
    };
    base.saturating_add_signed(value_add(&profile.movement, power, unit.kind, domain, ""))
}

pub fn effective_movement_cost(state: &State, unit: &Unit, base: Option<u64>) -> Option<u64> {
    let mut cost = base?;
    let Some((profile, power)) = effective_profile(state, &unit.owner) else {
        return Some(cost);
    };
    let rules = profile.movement_cost.day_to_day.iter().chain(match power {
        Power::None => [].iter(),
        Power::Cop => profile.movement_cost.cop.iter(),
        Power::Scop => profile.movement_cost.scop.iter(),
    });
    for rule in rules {
        if rule
            .except_weather_kinds
            .iter()
            .any(|weather| weather == &state.weather.kind)
        {
            continue;
        }
        match rule.operator {
            MovementCostOperator::TraversableCostSet => cost = rule.value,
        }
    }
    Some(cost)
}

pub fn effective_vision(state: &State, unit: &Unit, base: i64, domain: &str) -> i64 {
    let Some((profile, power)) = effective_profile(state, &unit.owner) else {
        return base;
    };
    (base + value_add(&profile.vision, power, unit.kind, domain, "")).max(0)
}

pub fn reveals_concealing_terrain(state: &State, unit: &Unit) -> bool {
    let Some((profile, power)) = effective_profile(state, &unit.owner) else {
        return false;
    };
    match power {
        Power::None => profile
            .reveals_concealing_terrain
            .day_to_day
            .unwrap_or(false),
        Power::Cop => profile
            .reveals_concealing_terrain
            .cop
            .or(profile.reveals_concealing_terrain.day_to_day)
            .unwrap_or(false),
        Power::Scop => profile
            .reveals_concealing_terrain
            .scop
            .or(profile.reveals_concealing_terrain.day_to_day)
            .unwrap_or(false),
    }
}

pub fn effective_attack_range(
    state: &State,
    unit: &Unit,
    base: u64,
    domain: &str,
    fire_mode: &str,
) -> u64 {
    let Some((profile, power)) = effective_profile(state, &unit.owner) else {
        return base;
    };
    base.saturating_add_signed(value_add(
        &profile.attack_range,
        power,
        unit.kind,
        domain,
        fire_mode,
    ))
}

fn selected_rational(states: &RationalStates, power: Power) -> Option<Rational> {
    match power {
        Power::None => states.day_to_day,
        Power::Cop => states.cop.or(states.day_to_day),
        Power::Scop => states.scop.or(states.day_to_day),
    }
}

pub fn effective_build_cost(state: &State, player: &str, base: u64) -> Option<u64> {
    let Some((profile, power)) = effective_profile(state, player) else {
        return Some(base);
    };
    let Some(ratio) = selected_rational(&profile.build_cost, power) else {
        return Some(base);
    };
    base.checked_mul(ratio.numerator)?
        .checked_div(ratio.denominator)
}

fn production_rules(
    states: &ProductionStates,
    power: Power,
) -> impl Iterator<Item = &ProductionRule> {
    states.day_to_day.iter().chain(match power {
        Power::None => [].iter(),
        Power::Cop => states.cop.iter(),
        Power::Scop => states.scop.iter(),
    })
}

pub fn commander_production_site(
    state: &State,
    player: &str,
    terrain: Terrain,
    domain: Option<&str>,
) -> bool {
    let Some((profile, power)) = effective_profile(state, player) else {
        return false;
    };
    production_rules(&profile.production, power).any(|rule| {
        rule.terrain_kinds
            .iter()
            .any(|value| value == terrain.as_str())
            && domain.is_none_or(|domain| rule.domains.iter().any(|value| value == domain))
    })
}

pub fn effective_capture_points(state: &State, unit: &Unit, visual_hp: u64) -> u64 {
    let Some((profile, power)) = effective_profile(state, &unit.owner) else {
        return visual_hp;
    };
    let effect = match power {
        Power::None => profile.capture.day_to_day,
        Power::Cop => profile.capture.cop.or(profile.capture.day_to_day),
        Power::Scop => profile.capture.scop.or(profile.capture.day_to_day),
    };
    let Some(effect) = effect else {
        return visual_hp;
    };
    if effect.instant {
        20
    } else {
        visual_hp
            .saturating_mul(effect.numerator)
            .checked_div(effect.denominator)
            .unwrap_or(visual_hp)
    }
}

pub fn effective_income_per_property(state: &State, player: &str) -> u64 {
    let add =
        effective_profile(state, player).map_or(0, |(profile, _)| profile.income_per_property_add);
    state
        .settings
        .income_per_property
        .saturating_add_signed(add)
}

pub fn effective_repair_bars(state: &State, player: &str) -> u64 {
    2 + effective_profile(state, player).map_or(0, |(profile, _)| profile.repair_bars_add)
}

pub fn effective_upkeep(state: &State, unit: &Unit, base: u64, domain: &str) -> u64 {
    let add = effective_profile(state, &unit.owner)
        .filter(|_| domain == "air")
        .map_or(0, |(profile, _)| profile.air_upkeep_add);
    base.saturating_add_signed(add)
}

/// The weather column this unit's movement costs are read from, after the
/// commander effects that let a unit ignore or reinterpret the real weather.
pub fn effective_weather(state: &State, unit: &Unit) -> WeatherKind {
    let profile = effective_profile(state, &unit.owner).map(|(profile, _)| profile);
    match state.weather.kind {
        WeatherKind::Snow if profile.is_some_and(|p| p.ignores_snow_movement) => WeatherKind::Clear,
        WeatherKind::Rain if profile.is_some_and(|p| p.ignores_rain_movement) => WeatherKind::Clear,
        WeatherKind::Rain if profile.is_some_and(|p| p.rain_movement_as_snow) => WeatherKind::Snow,
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CommanderKind, InstantEffect, PowerLevel, UnitTarget, maximum_power_charge,
        power_activation,
    };

    #[test]
    fn adder_power_costs_scale_from_pre_activation_uses_and_cap() {
        assert_eq!(
            power_activation(CommanderKind::Adder, PowerLevel::Cop, 0)
                .unwrap()
                .unwrap()
                .cost,
            18_000
        );
        assert_eq!(
            power_activation(CommanderKind::Adder, PowerLevel::Scop, 0)
                .unwrap()
                .unwrap()
                .cost,
            45_000
        );
        assert_eq!(
            power_activation(CommanderKind::Adder, PowerLevel::Cop, 1)
                .unwrap()
                .unwrap()
                .cost,
            21_600
        );
        assert_eq!(
            power_activation(CommanderKind::Adder, PowerLevel::Cop, 10)
                .unwrap()
                .unwrap()
                .cost,
            54_000
        );
        assert_eq!(
            power_activation(CommanderKind::Adder, PowerLevel::Cop, 100)
                .unwrap()
                .unwrap()
                .cost,
            54_000
        );
        assert_eq!(
            power_activation(CommanderKind::Neutral, PowerLevel::Cop, 0).unwrap(),
            None
        );
        assert_eq!(
            power_activation(CommanderKind::Andy, PowerLevel::Cop, 0)
                .unwrap()
                .unwrap()
                .instant_effects,
            vec![InstantEffect::HealVisualHp {
                target: UnitTarget::Owned,
                amount: 2,
            }]
        );
        assert_eq!(
            power_activation(CommanderKind::Hawke, PowerLevel::Scop, 0)
                .unwrap()
                .unwrap()
                .cost,
            81_000
        );
        assert_eq!(
            maximum_power_charge(CommanderKind::Hawke, 0).unwrap(),
            Some(81_000)
        );
        assert_eq!(
            maximum_power_charge(CommanderKind::Adder, 1).unwrap(),
            Some(54_000)
        );
    }
}
