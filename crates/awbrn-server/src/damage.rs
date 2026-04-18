#![allow(dead_code)]

use awbrn_types::{DamagePts, ExactHp, Unit, VisualHp};

/// Exact HP-point deltas from a combat engagement on the 0-100 HP scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CombatOutcome {
    /// Damage dealt to the defender.
    pub attacker_damage_pts: u8,
    /// Damage dealt to the attacker by counterattack.
    ///
    /// `None` if indirect attack, defender destroyed, or defender has no
    /// weapon against the attacker's unit type.
    pub defender_damage_pts: Option<u8>,
}

/// Absolute percentage modifier where 100 is neutral.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PercentMod(i32);

impl PercentMod {
    pub const fn new(value: i32) -> Self {
        Self(value)
    }

    pub const fn neutral() -> Self {
        Self(100)
    }

    pub const fn get(self) -> i32 {
        self.0
    }
}

/// Inclusive upper bound passed to the combat luck roll.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LuckCap(u8);

impl LuckCap {
    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    pub const fn standard_good() -> Self {
        Self(9)
    }

    pub const fn none() -> Self {
        Self(0)
    }

    pub const fn get(self) -> u8 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LuckDelta(i32);

impl LuckDelta {
    pub const fn new(value: i32) -> Self {
        Self(value)
    }

    pub const fn none() -> Self {
        Self(0)
    }

    pub const fn get(self) -> i32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TerrainStars(u8);

impl TerrainStars {
    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u8 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CombatSide {
    pub unit_type: Unit,
    pub exact_hp: ExactHp,
    pub attack_mod: PercentMod,
    pub defense_mod: PercentMod,
    pub max_good_luck: LuckCap,
    pub max_bad_luck: LuckCap,
    pub ammo: u32,
    pub terrain_stars: TerrainStars,
}

pub struct CombatInput {
    pub attacker: CombatSide,
    pub defender: CombatSide,
    pub is_direct_combat: bool,
}

/// Look up base damage for an engagement.
///
/// When `attacker_ammo > 0`, primary damage is preferred and secondary damage
/// is used as a fallback. When `attacker_ammo == 0`, only secondary damage is
/// available.
pub fn base_damage(attacker: Unit, defender: Unit, attacker_ammo: u32) -> Option<u8> {
    if attacker_ammo > 0 {
        primary_damage(attacker, defender).or_else(|| secondary_damage(attacker, defender))
    } else {
        secondary_damage(attacker, defender)
    }
}

/// Returns true if this engagement consumes one unit of ammo (i.e. the primary
/// weapon fires). Secondary-weapon attacks and zero-ammo fallbacks do not cost ammo.
pub fn uses_primary_weapon(attacker: Unit, defender: Unit, attacker_ammo: u32) -> bool {
    attacker_ammo > 0 && primary_damage(attacker, defender).is_some()
}

/// Pure single-hit calculation with pre-rolled luck.
///
/// All `*_mod` values are absolute percentages, where 100 is neutral.
pub fn calculate_single_hit(
    base_dmg: u8,
    attack_mod: PercentMod,
    luck: LuckDelta,
    visual_attacker_hp: VisualHp,
    defense_mod: PercentMod,
    terrain_stars: TerrainStars,
    visual_defender_hp: VisualHp,
) -> DamagePts {
    let attack_factor = (i32::from(base_dmg) * attack_mod.get() / 100 + luck.get()).max(0);
    let attack_hp_factor = attack_factor * i32::from(visual_attacker_hp.get()) / 10;
    let defense_numerator = 200
        - (defense_mod.get()
            + i32::from(terrain_stars.get()) * i32::from(visual_defender_hp.get()));
    let damage = (attack_hp_factor * defense_numerator / 100).max(0);
    DamagePts::new(damage.min(100) as u8)
}

/// Deterministic combat resolution. The caller provides pre-rolled luck values.
pub fn calculate_combat(
    input: &CombatInput,
    attacker_luck: LuckDelta,
    defender_luck: LuckDelta,
) -> Option<CombatOutcome> {
    let atk_dmg = calculate_hit(&input.attacker, &input.defender, attacker_luck)?;

    let def_dmg = if input.is_direct_combat && atk_dmg.get() < input.defender.exact_hp.get() {
        let mut damaged_defender = input.defender;
        damaged_defender.exact_hp = damaged_defender.exact_hp.saturating_sub(atk_dmg);
        calculate_hit(&damaged_defender, &input.attacker, defender_luck)
    } else {
        None
    };

    Some(CombatOutcome {
        attacker_damage_pts: atk_dmg.get(),
        defender_damage_pts: def_dmg.map(DamagePts::get),
    })
}

/// RNG-driven entry point for use from command application.
pub(crate) fn calculate_combat_rng(
    input: &CombatInput,
    rng: &mut crate::setup::GameRng,
) -> Option<CombatOutcome> {
    let attacker_luck = roll_luck(rng, &input.attacker);
    let defender_luck = roll_luck(rng, &input.defender);
    calculate_combat(input, attacker_luck, defender_luck)
}

fn roll_luck(rng: &mut crate::setup::GameRng, side: &CombatSide) -> LuckDelta {
    LuckDelta::new(
        i32::from(rng.roll(side.max_good_luck.get()))
            - i32::from(rng.roll(side.max_bad_luck.get())),
    )
}

fn calculate_hit(
    attacker: &CombatSide,
    defender: &CombatSide,
    luck: LuckDelta,
) -> Option<DamagePts> {
    let base = base_damage(attacker.unit_type, defender.unit_type, attacker.ammo)?;
    let damage = calculate_single_hit(
        base,
        attacker.attack_mod,
        luck,
        attacker.exact_hp.visual(),
        defender.defense_mod,
        defender.terrain_stars,
        defender.exact_hp.visual(),
    );

    Some(defender.exact_hp.clamp_damage(damage))
}

fn primary_damage(attacker: Unit, defender: Unit) -> Option<u8> {
    damage_from_table(&PRIMARY_DAMAGE, attacker, defender)
}

fn secondary_damage(attacker: Unit, defender: Unit) -> Option<u8> {
    damage_from_table(&SECONDARY_DAMAGE, attacker, defender)
}

fn damage_from_table(
    table: &[[u8; Unit::COUNT]; Unit::COUNT],
    attacker: Unit,
    defender: Unit,
) -> Option<u8> {
    match table[attacker.table_index()][defender.table_index()] {
        0 => None,
        damage => Some(damage),
    }
}

// AWDS base damage table in Unit enum order. Zero means no weapon entry.
const PRIMARY_DAMAGE: [[u8; Unit::COUNT]; Unit::COUNT] = [
    [
        45, 50, 50, 105, 0, 0, 120, 75, 0, 0, 65, 105, 0, 10, 105, 1, 55, 5, 25, 60, 45, 75, 0,
        105, 25,
    ],
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    [
        75, 70, 75, 0, 40, 55, 0, 0, 45, 50, 0, 90, 55, 45, 85, 15, 80, 40, 70, 80, 80, 0, 60, 0,
        70,
    ],
    [
        25, 60, 65, 0, 25, 25, 0, 0, 25, 25, 0, 0, 25, 25, 0, 10, 65, 20, 55, 55, 65, 0, 25, 0, 55,
    ],
    [
        85, 80, 80, 0, 50, 95, 0, 0, 60, 95, 0, 95, 95, 55, 90, 25, 90, 50, 80, 90, 85, 0, 95, 0,
        80,
    ],
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    [
        95, 105, 105, 0, 75, 105, 0, 0, 75, 50, 0, 110, 95, 95, 110, 35, 105, 90, 105, 105, 105, 0,
        95, 0, 105,
    ],
    [
        0, 0, 0, 115, 0, 0, 120, 100, 0, 0, 100, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 100, 0, 115, 0,
    ],
    [
        0, 0, 0, 0, 5, 25, 0, 0, 5, 25, 0, 0, 25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 90, 0, 0,
    ],
    [
        0, 0, 0, 120, 0, 0, 120, 100, 0, 0, 55, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 85, 0, 120, 0,
    ],
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    [
        105, 105, 105, 0, 10, 35, 0, 0, 10, 30, 0, 0, 35, 55, 0, 25, 105, 45, 85, 105, 105, 0, 10,
        0, 85,
    ],
    [
        65, 75, 70, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 15, 0, 5, 85, 15, 55, 85, 85, 0, 0, 0, 55,
    ],
    [
        195, 195, 195, 0, 45, 105, 0, 0, 45, 65, 0, 0, 75, 125, 0, 65, 195, 115, 180, 185, 195, 0,
        45, 0, 180,
    ],
    [
        0, 0, 0, 120, 0, 0, 120, 100, 0, 0, 100, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 100, 0, 120, 0,
    ],
    [
        115, 125, 115, 0, 15, 40, 0, 0, 15, 30, 0, 0, 40, 75, 0, 35, 125, 55, 105, 125, 125, 0, 15,
        0, 105,
    ],
    [
        85, 80, 80, 105, 55, 60, 105, 75, 60, 60, 65, 95, 60, 55, 90, 25, 90, 50, 80, 90, 85, 75,
        85, 105, 80,
    ],
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    [
        85, 80, 80, 0, 55, 60, 0, 0, 60, 60, 0, 95, 60, 55, 90, 25, 90, 50, 80, 90, 85, 0, 85, 0,
        80,
    ],
    [
        50, 85, 75, 85, 45, 65, 120, 70, 45, 35, 45, 90, 65, 70, 90, 15, 85, 60, 80, 85, 85, 55,
        55, 95, 75,
    ],
    [
        0, 0, 0, 0, 65, 95, 0, 0, 75, 25, 0, 0, 95, 0, 0, 0, 0, 0, 0, 0, 0, 0, 55, 0, 0,
    ],
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    [
        65, 75, 70, 0, 1, 10, 0, 0, 1, 5, 0, 0, 10, 15, 0, 10, 85, 15, 55, 85, 85, 0, 1, 0, 55,
    ],
];

const SECONDARY_DAMAGE: [[u8; Unit::COUNT]; Unit::COUNT] = [
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    [
        6, 20, 25, 65, 0, 0, 0, 0, 0, 0, 0, 75, 0, 1, 75, 1, 35, 1, 6, 30, 35, 0, 0, 95, 6,
    ],
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    [
        0, 0, 0, 105, 0, 0, 120, 100, 0, 0, 85, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 100, 0, 105, 0,
    ],
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    [
        5, 14, 15, 7, 0, 0, 0, 0, 0, 0, 0, 55, 0, 1, 45, 1, 25, 1, 5, 12, 25, 0, 0, 30, 5,
    ],
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    [
        7, 45, 45, 12, 0, 0, 0, 0, 0, 0, 0, 105, 0, 1, 95, 1, 35, 1, 7, 45, 55, 0, 0, 45, 8,
    ],
    [
        6, 20, 32, 9, 0, 0, 0, 0, 0, 0, 0, 65, 0, 1, 55, 1, 35, 1, 6, 18, 35, 0, 0, 35, 6,
    ],
    [
        17, 65, 65, 22, 0, 0, 0, 0, 0, 0, 0, 135, 0, 1, 125, 1, 55, 1, 17, 65, 75, 0, 0, 55, 10,
    ],
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    [
        17, 65, 65, 22, 0, 0, 0, 0, 0, 0, 0, 125, 0, 1, 115, 1, 55, 1, 17, 65, 75, 0, 0, 55, 10,
    ],
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    [
        4, 45, 45, 10, 0, 0, 0, 0, 0, 0, 0, 70, 0, 1, 65, 1, 28, 1, 6, 35, 55, 0, 0, 35, 6,
    ],
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    [
        5, 45, 45, 10, 0, 0, 0, 0, 0, 0, 0, 75, 0, 1, 70, 1, 30, 1, 6, 40, 55, 0, 0, 40, 6,
    ],
];

#[cfg(test)]
mod tests {
    use super::*;

    fn side(unit_type: Unit, exact_hp: u8, ammo: u32, terrain_stars: u8) -> CombatSide {
        CombatSide {
            unit_type,
            exact_hp: ExactHp::new(exact_hp),
            attack_mod: PercentMod::neutral(),
            defense_mod: PercentMod::neutral(),
            max_good_luck: LuckCap::standard_good(),
            max_bad_luck: LuckCap::none(),
            ammo,
            terrain_stars: TerrainStars::new(terrain_stars),
        }
    }

    fn single_hit(
        base_dmg: u8,
        attack_mod: i32,
        luck: i32,
        visual_attacker_hp: u8,
        defense_mod: i32,
        terrain_stars: u8,
        visual_defender_hp: u8,
    ) -> u8 {
        calculate_single_hit(
            base_dmg,
            PercentMod::new(attack_mod),
            LuckDelta::new(luck),
            VisualHp::new(visual_attacker_hp),
            PercentMod::new(defense_mod),
            TerrainStars::new(terrain_stars),
            VisualHp::new(visual_defender_hp),
        )
        .get()
    }

    #[test]
    fn infantry_vs_infantry_plains_zero_luck() {
        assert_eq!(single_hit(55, 100, 0, 10, 100, 1, 10), 49);
    }

    #[test]
    fn tank_vs_infantry_mountain_zero_luck() {
        assert_eq!(single_hit(75, 100, 0, 10, 100, 4, 10), 45);
    }

    #[test]
    fn indirect_attack_produces_no_counterattack() {
        let outcome = calculate_combat(
            &CombatInput {
                attacker: side(Unit::Artillery, 100, 9, 0),
                defender: side(Unit::Infantry, 100, 0, 1),
                is_direct_combat: false,
            },
            LuckDelta::none(),
            LuckDelta::none(),
        )
        .unwrap();

        assert_eq!(outcome.defender_damage_pts, None);
    }

    #[test]
    fn kanbei_attack_bonus_correct() {
        assert_eq!(single_hit(55, 130, 0, 10, 100, 1, 10), 63);
    }

    #[test]
    fn kanbei_defense_bonus_correct() {
        assert_eq!(single_hit(55, 100, 0, 10, 130, 1, 10), 33);
    }

    #[test]
    fn luck_zero_minimum() {
        assert_eq!(single_hit(55, 100, -80, 10, 100, 1, 10), 0);
    }

    #[test]
    fn luck_max() {
        assert_eq!(single_hit(55, 100, 9, 10, 100, 1, 10), 57);
    }

    #[test]
    fn counterattack_uses_post_damage_visual_hp() {
        let outcome = calculate_combat(
            &CombatInput {
                attacker: side(Unit::Infantry, 100, 0, 0),
                defender: side(Unit::Infantry, 100, 0, 0),
                is_direct_combat: true,
            },
            LuckDelta::none(),
            LuckDelta::none(),
        )
        .unwrap();

        assert_eq!(outcome.attacker_damage_pts, 55);
        assert_eq!(outcome.defender_damage_pts, Some(27));
    }

    #[test]
    fn tank_with_no_ammo_uses_secondary_weapon() {
        assert_eq!(base_damage(Unit::Tank, Unit::Infantry, 0), Some(75));
    }

    #[test]
    fn damage_is_clamped_to_remaining_exact_hp() {
        let outcome = calculate_combat(
            &CombatInput {
                attacker: side(Unit::Bomber, 100, 9, 0),
                defender: side(Unit::Infantry, 12, 0, 0),
                is_direct_combat: true,
            },
            LuckDelta::none(),
            LuckDelta::none(),
        )
        .unwrap();

        assert_eq!(outcome.attacker_damage_pts, 12);
        assert_eq!(outcome.defender_damage_pts, None);
    }

    #[test]
    fn counterattack_uses_defender_mods_and_attacker_terrain() {
        let mut attacker = side(Unit::Infantry, 100, 0, 2);
        attacker.defense_mod = PercentMod::new(130);

        let mut defender = side(Unit::Infantry, 100, 0, 0);
        defender.attack_mod = PercentMod::new(130);

        let outcome = calculate_combat(
            &CombatInput {
                attacker,
                defender,
                is_direct_combat: true,
            },
            LuckDelta::none(),
            LuckDelta::none(),
        )
        .unwrap();

        assert_eq!(outcome.attacker_damage_pts, 55);
        assert_eq!(outcome.defender_damage_pts, Some(17));
    }
}
