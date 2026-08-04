//! Revisioned, commander-neutral effective-value queries.
//!
//! This module is the only place that interprets commander profile operators.
//! Transition reducers provide context; they never branch on commander names.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::ruleset::{Domain as UnitDomain, FireMode, PropertyKind, Terrain, UnitKind};
use crate::semantic::{
    CommanderId as CommanderKind, Player, PlayerId, PowerState, State, Unit, WeatherKind,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strike {
    Initial,
    Counter,
}

/// The finer unit-domain vocabulary used by commander combat predicates.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CombatDomain {
    Foot,
    GroundVehicle,
    Air,
    Naval,
    Transport,
}

/// One side of an engagement, described in the vocabulary
/// `commander-combat.json` predicates are written in.
#[derive(Clone, Copy, Debug)]
pub struct Combatant<'a> {
    pub kind: UnitKind,
    pub domain: CombatDomain,
    pub fire_mode: FireMode,
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

/// A table the commander documents key by identifier, indexed by the ruleset's
/// own commander vocabulary instead.
///
/// The documents are JSON objects keyed by commander id, and keeping that shape
/// at runtime meant every effective-value query hashed a string — which the fog
/// projection does once per unit per tile. Lowered once, when the document is
/// decoded, so a query is an array index. A key outside the vocabulary fails to
/// decode rather than becoming a silently unreachable entry.
#[derive(Clone, Debug)]
struct ByCommander<T>([Option<T>; CommanderKind::COUNT]);

impl<T> ByCommander<T> {
    fn get(&self, commander: CommanderKind) -> Option<&T> {
        self.0[commander.index()].as_ref()
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for ByCommander<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut named = HashMap::<String, T>::deserialize(deserializer)?;
        let mut slots: [Option<T>; CommanderKind::COUNT] = std::array::from_fn(|_| None);
        for commander in CommanderKind::ALL {
            slots[commander.index()] = named.remove(commander.as_str());
        }
        if let Some(unknown) = named.keys().next() {
            return Err(serde::de::Error::custom(format!(
                "commander table names {unknown}, which this ruleset's vocabulary lacks"
            )));
        }
        Ok(Self(slots))
    }
}

#[derive(Clone, Debug, Deserialize)]
struct CombatTable {
    generic_power_bonus: PowerBonuses,
    commanders: ByCommander<CombatProfile>,
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
    unit_kinds: Vec<UnitKind>,
    #[serde(default)]
    capabilities_all: Vec<String>,
    #[serde(default)]
    domains: Vec<CombatDomain>,
    #[serde(default)]
    fire_modes: Vec<FireMode>,
    #[serde(default)]
    terrain_kinds: Vec<Terrain>,
    #[serde(default)]
    weather_kinds: Vec<WeatherKind>,
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
    commanders: ByCommander<EffectiveProfile>,
}

#[derive(Clone, Debug, Deserialize)]
struct PowerTable {
    base_star_charge: u64,
    use_cost_scaling: UseCostScaling,
    commanders: ByCommander<PowerProfile>,
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
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
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
    movement: UnitStateValues,
    #[serde(default)]
    movement_cost: MovementCostStates,
    #[serde(default)]
    vision: UnitStateValues,
    #[serde(default)]
    reveals_concealing_terrain: BooleanStates,
    #[serde(default)]
    attack_range: AttackRangeStateValues,
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
struct UnitStateValues {
    #[serde(default)]
    day_to_day: Vec<UnitValueRule>,
    #[serde(default)]
    cop: Vec<UnitValueRule>,
    #[serde(default)]
    scop: Vec<UnitValueRule>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct AttackRangeStateValues {
    #[serde(default)]
    day_to_day: Vec<AttackRangeRule>,
    #[serde(default)]
    cop: Vec<AttackRangeRule>,
    #[serde(default)]
    scop: Vec<AttackRangeRule>,
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
#[serde(deny_unknown_fields)]
struct UnitValueRule {
    #[serde(default)]
    unit_kinds: Vec<UnitKind>,
    #[serde(default)]
    domains: Vec<UnitDomain>,
    add: i64,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackRangeRule {
    #[serde(default)]
    unit_kinds: Vec<UnitKind>,
    #[serde(default)]
    domains: Vec<UnitDomain>,
    #[serde(default)]
    fire_modes: Vec<FireMode>,
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
    terrain_kinds: Vec<Terrain>,
    domains: Vec<UnitDomain>,
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
    let Some(profile) = table.commanders.get(commander) else {
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

/// Return the current charge required to activate one commander-power level.
///
/// This is the presentation-safe cost query: callers can render a power meter
/// using the same revisioned scaling rules as command validation without
/// interpreting the embedded commander table themselves. `None` means that
/// the commander does not support the requested level.
pub fn power_activation_cost(
    commander: CommanderKind,
    level: PowerLevel,
    power_uses: u64,
) -> Result<Option<u64>, PowerActivationError> {
    power_activation(commander, level, power_uses)
        .map(|activation| activation.map(|activation| activation.cost))
}

/// Return the charge that one power star is worth at the given use count.
///
/// A power meter is drawn in stars, so a renderer needs the value of one
/// segment as well as the level costs. This is the same revisioned scaling that
/// [`power_activation_cost`] applies, so a cost divided by this value always
/// gives that level's whole star count.
pub fn power_star_charge(power_uses: u64) -> Result<u64, PowerActivationError> {
    scaled_power_charge(power_table(), 1, power_uses)
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
    let Some(profile) = table.commanders.get(commander) else {
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
    player: &PlayerId,
    target_owner: &PlayerId,
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
    let Some(profile) = table.commanders.get(commander) else {
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

fn active<'a>(
    state: &'a State,
    player_id: &PlayerId,
) -> Option<(&'a Player, CommanderKind, Power)> {
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
    (predicate.unit_kinds.is_empty() || predicate.unit_kinds.contains(&unit.kind))
        && (predicate.domains.is_empty() || predicate.domains.contains(&unit.domain))
        && (predicate.fire_modes.is_empty() || predicate.fire_modes.contains(&unit.fire_mode))
        && (predicate.terrain_kinds.is_empty() || predicate.terrain_kinds.contains(&unit.terrain))
        && (predicate.weather_kinds.is_empty() || predicate.weather_kinds.contains(&unit.weather))
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

/// One side's effective combat values, after the commander algebra.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectiveCombat {
    /// Attack percentage, at or above zero.
    pub attack: i64,
    /// Defense percentage, at or above zero.
    pub defense: i64,
    /// Terrain stars under the defender, after commander modification.
    pub terrain_stars: i64,
    /// The inclusive range a good-luck roll must fall in.
    pub good_luck: Domain,
    /// The inclusive range a bad-luck roll must fall in. Empty (`0..=0`) unless
    /// a commander grants bad luck.
    pub bad_luck: Domain,
}

impl EffectiveCombat {
    /// The values a unit fights with when no commander modifies it.
    fn unmodified(base_terrain_stars: i64) -> Self {
        Self {
            attack: 100,
            defense: 100,
            terrain_stars: base_terrain_stars,
            good_luck: Domain {
                minimum: 0,
                maximum: 9,
            },
            bad_luck: Domain {
                minimum: 0,
                maximum: 0,
            },
        }
    }
}

pub fn effective_combat(
    state: &State,
    owner: &PlayerId,
    unit: Combatant<'_>,
    strike: Strike,
    context: CombatContext,
) -> Option<EffectiveCombat> {
    let table = combat_table();
    let Some((_, commander, power)) = active(state, owner) else {
        return Some(EffectiveCombat::unmodified(context.base_terrain_stars));
    };
    let Some(profile) = table.commanders.get(commander) else {
        return Some(EffectiveCombat::unmodified(context.base_terrain_stars));
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
    Some(EffectiveCombat {
        attack: attack.max(0),
        defense: defense.max(0),
        terrain_stars: stars.max(0),
        good_luck: good,
        bad_luck: bad,
    })
}

pub fn effective_enemy_terrain_stars(
    state: &State,
    owner: &PlayerId,
    unit: Combatant<'_>,
    strike: Strike,
    base: i64,
) -> Option<i64> {
    let table = combat_table();
    let Some((_, commander, power)) = active(state, owner) else {
        return Some(base.max(0));
    };
    let Some(profile) = table.commanders.get(commander) else {
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

pub fn counter_first(state: &State, owner: &PlayerId, unit: Combatant<'_>, strike: Strike) -> bool {
    let table = combat_table();
    let Some((_, commander, power)) = active(state, owner) else {
        return false;
    };
    table.commanders.get(commander).is_some_and(|profile| {
        applicable_rules(profile, power).any(|rule| {
            predicate_matches(&rule.when, unit, strike)
                && matches!(rule.effect, Effect::CounterFirst)
        })
    })
}

fn sum_unit_additions(
    rules: &UnitStateValues,
    power: Power,
    kind: UnitKind,
    domain: UnitDomain,
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
            (rule.unit_kinds.is_empty() || rule.unit_kinds.contains(&kind))
                && (rule.domains.is_empty() || rule.domains.contains(&domain))
        })
        .map(|rule| rule.add)
        .sum()
}

fn sum_attack_range_additions(
    rules: &AttackRangeStateValues,
    power: Power,
    kind: UnitKind,
    domain: UnitDomain,
    fire_mode: FireMode,
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
            (rule.unit_kinds.is_empty() || rule.unit_kinds.contains(&kind))
                && (rule.domains.is_empty() || rule.domains.contains(&domain))
                && (rule.fire_modes.is_empty() || rule.fire_modes.contains(&fire_mode))
        })
        .map(|rule| rule.add)
        .sum()
}

fn effective_profile(
    state: &State,
    owner: &PlayerId,
) -> Option<(&'static EffectiveProfile, Power)> {
    let (_, commander, power) = active(state, owner)?;
    Some((profile_table().commanders.get(commander)?, power))
}

pub fn effective_move(state: &State, unit: &Unit, base: u64, domain: UnitDomain) -> u64 {
    let Some((profile, power)) = effective_profile(state, &unit.owner) else {
        return base;
    };
    base.saturating_add_signed(sum_unit_additions(
        &profile.movement,
        power,
        unit.kind,
        domain,
    ))
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

pub fn effective_vision(state: &State, unit: &Unit, base: i64, domain: UnitDomain) -> i64 {
    let Some((profile, power)) = effective_profile(state, &unit.owner) else {
        return base;
    };
    (base + sum_unit_additions(&profile.vision, power, unit.kind, domain)).max(0)
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
    domain: UnitDomain,
    fire_mode: FireMode,
) -> u64 {
    let Some((profile, power)) = effective_profile(state, &unit.owner) else {
        return base;
    };
    base.saturating_add_signed(sum_attack_range_additions(
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

pub fn effective_build_cost(state: &State, player: &PlayerId, base: u64) -> Option<u64> {
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
    player: &PlayerId,
    terrain: Terrain,
    domain: UnitDomain,
) -> bool {
    let Some((profile, power)) = effective_profile(state, player) else {
        return false;
    };
    production_rules(&profile.production, power)
        .any(|rule| rule.terrain_kinds.contains(&terrain) && rule.domains.contains(&domain))
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

pub fn effective_income_per_property(state: &State, player: &PlayerId) -> u64 {
    let add =
        effective_profile(state, player).map_or(0, |(profile, _)| profile.income_per_property_add);
    state
        .settings
        .income_per_property
        .saturating_add_signed(add)
}

pub fn effective_repair_bars(state: &State, player: &PlayerId) -> u64 {
    2 + effective_profile(state, player).map_or(0, |(profile, _)| profile.repair_bars_add)
}

pub fn effective_upkeep(state: &State, unit: &Unit, base: u64, domain: UnitDomain) -> u64 {
    let add = effective_profile(state, &unit.owner)
        .filter(|_| domain == UnitDomain::Air)
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
        power_activation, power_activation_cost, power_star_charge,
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

    #[test]
    fn a_star_is_worth_a_whole_division_of_every_power_cost() {
        for uses in 0..=12 {
            let star = power_star_charge(uses).unwrap();
            for level in [PowerLevel::Cop, PowerLevel::Scop] {
                let Some(cost) = power_activation_cost(CommanderKind::Hawke, level, uses).unwrap()
                else {
                    continue;
                };
                assert_eq!(cost % star, 0, "level {level:?} at {uses} uses");
            }
        }

        // The price rises 20% of the base for every power used, and stops
        // rising after ten.
        assert_eq!(power_star_charge(0).unwrap(), 9_000);
        assert_eq!(power_star_charge(1).unwrap(), 10_800);
        assert_eq!(power_star_charge(10).unwrap(), 27_000);
        assert_eq!(power_star_charge(50).unwrap(), 27_000);
    }
}
