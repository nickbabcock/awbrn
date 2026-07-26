#![allow(dead_code)]

use awbrn_types::{DamagePts, ExactHp, Unit, VisualHp};
use awvm::combat::{Weapon, select_weapon};
use awvm::ruleset::UnitKind;

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

/// The weapon this engagement would fire, per the active ruleset.
///
/// The ammo weapon is preferred, and the unlimited weapon is the fallback both
/// when the ammo weapon has no entry against the defender and when the attacker
/// is out of ammo.
fn weapon(
    attacker: Unit,
    defender: Unit,
    attacker_ammo: u32,
) -> Option<awvm::combat::SelectedWeapon> {
    select_weapon(
        unit_kind(attacker),
        unit_kind(defender),
        u64::from(attacker_ammo),
    )
}

/// Look up base damage for an engagement.
pub fn base_damage(attacker: Unit, defender: Unit, attacker_ammo: u32) -> Option<u8> {
    weapon(attacker, defender, attacker_ammo).map(|w| w.base_damage as u8)
}

/// Returns true if this engagement consumes one unit of ammo (i.e. the primary
/// weapon fires). Secondary-weapon attacks and zero-ammo fallbacks do not cost ammo.
pub fn uses_primary_weapon(attacker: Unit, defender: Unit, attacker_ammo: u32) -> bool {
    weapon(attacker, defender, attacker_ammo).is_some_and(|w| w.weapon == Weapon::Ammo)
}

/// Bridge this crate's unit vocabulary to the ruleset's.
///
/// Spelled out rather than routed through the AWBW id so that adding a unit on
/// either side stops compiling here instead of failing a lookup at runtime.
const fn unit_kind(unit: Unit) -> UnitKind {
    match unit {
        Unit::AntiAir => UnitKind::AntiAir,
        Unit::APC => UnitKind::Apc,
        Unit::Artillery => UnitKind::Artillery,
        Unit::BCopter => UnitKind::BCopter,
        Unit::Battleship => UnitKind::Battleship,
        Unit::BlackBoat => UnitKind::BlackBoat,
        Unit::BlackBomb => UnitKind::BlackBomb,
        Unit::Bomber => UnitKind::Bomber,
        Unit::Carrier => UnitKind::Carrier,
        Unit::Cruiser => UnitKind::Cruiser,
        Unit::Fighter => UnitKind::Fighter,
        Unit::Infantry => UnitKind::Infantry,
        Unit::Lander => UnitKind::Lander,
        Unit::MdTank => UnitKind::MdTank,
        Unit::Mech => UnitKind::Mech,
        Unit::MegaTank => UnitKind::MegaTank,
        Unit::Missile => UnitKind::Missile,
        Unit::NeoTank => UnitKind::NeoTank,
        Unit::PipeRunner => UnitKind::Piperunner,
        Unit::Recon => UnitKind::Recon,
        Unit::Rocket => UnitKind::Rocket,
        Unit::Stealth => UnitKind::Stealth,
        Unit::Sub => UnitKind::Sub,
        Unit::TCopter => UnitKind::TCopter,
        Unit::Tank => UnitKind::Tank,
    }
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

    /// Base damage now comes from the AWBW ruleset tables rather than the AWDS
    /// chart this module used to carry. These pairs are ones where the two
    /// charts disagree, so they fail if the lookup ever drifts back.
    #[test]
    fn base_damage_follows_the_awbw_chart() {
        // AWDS gives anti-air 105 against helicopters; AWBW gives 120.
        assert_eq!(base_damage(Unit::AntiAir, Unit::BCopter, 9), Some(120));
        // AWDS lets a cruiser's missiles hit ships; AWBW's do not, so the
        // engagement falls back to the machine gun's naval entry — none.
        assert_eq!(base_damage(Unit::Cruiser, Unit::Battleship, 9), None);
        assert!(!uses_primary_weapon(Unit::Cruiser, Unit::Battleship, 9));
        // AWDS gives a sub 65 against a battleship; AWBW gives 55.
        assert_eq!(base_damage(Unit::Sub, Unit::Battleship, 9), Some(55));
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
