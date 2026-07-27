//! Pure AWBW weapon selection and exact-HP combat arithmetic.

use crate::ruleset::{self, UnitKind};

/// Which of a unit's weapons fired. The vocabulary is the ruleset's, so the
/// name this serializes under cannot drift from the table it was selected from.
pub use crate::ruleset::WeaponSlot as Weapon;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectedWeapon {
    pub weapon: Weapon,
    pub base_damage: u64,
    pub ammo_cost: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Side {
    pub kind: UnitKind,
    pub hp: u8,
    pub ammo: u64,
    pub attack: i64,
    pub defense: i64,
    pub terrain_stars: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    pub damage: u8,
    pub weapon: SelectedWeapon,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Combat {
    pub attack: Hit,
    pub counter: Option<Hit>,
}

/// The weapon this attacker would fire at this defender, in the order
/// `weapons.json` mandates.
///
/// `None` means the engagement is impossible: an unknown unit kind, or no
/// weapon that both has a damage entry against the defender and is affordable.
pub fn select_weapon(attacker: UnitKind, defender: UnitKind, ammo: u64) -> Option<SelectedWeapon> {
    let profile = ruleset::profile(attacker);
    for slot in ruleset::WEAPON_SELECTION_ORDER {
        let Some(weapon) = profile.weapon(slot) else {
            continue;
        };
        if ruleset::WEAPON_SELECTION_REQUIRES_AVAILABLE_AMMO && weapon.ammo_cost > ammo {
            continue;
        }
        if let Some(base_damage) = weapon.damage(defender) {
            return Some(SelectedWeapon {
                weapon: slot,
                base_damage: u64::from(base_damage),
                ammo_cost: weapon.ammo_cost,
            });
        }
    }
    None
}

/// AWBW integer damage formula. Luck is the already-mapped signed modifier.
pub fn damage(attacker: Side, defender: Side, luck: i64) -> Option<Hit> {
    let weapon = select_weapon(attacker.kind, defender.kind, attacker.ammo)?;
    let visual_defender = u64::from(defender.hp).div_ceil(10);
    let attack_factor = ((weapon.base_damage as i64 * attacker.attack) / 100 + luck).max(0);
    let attack_hp_factor = attack_factor * i64::from(attacker.hp);
    let defense_numerator =
        200 - (defender.defense + i64::from(defender.terrain_stars) * visual_defender as i64);
    let points = (attack_hp_factor * defense_numerator / 100 / 100)
        .max(0)
        .min(i64::from(defender.hp));
    Some(Hit {
        damage: points as u8,
        weapon,
    })
}

pub fn resolve(
    attacker: Side,
    defender: Side,
    attacker_luck: i64,
    counter_luck: Option<i64>,
    direct: bool,
) -> Option<Combat> {
    let attack = damage(attacker, defender, attacker_luck)?;
    let remaining = defender.hp - attack.damage;
    let counter = if direct && remaining > 0 {
        counter_luck.and_then(|luck| {
            damage(
                Side {
                    hp: remaining,
                    ..defender
                },
                attacker,
                luck,
            )
        })
    } else {
        None
    };
    Some(Combat { attack, counter })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn infantry(hp: u8, ammo: u64, stars: u8) -> Side {
        Side {
            kind: UnitKind::Infantry,
            hp,
            ammo,
            attack: 100,
            defense: 100,
            terrain_stars: stars,
        }
    }
    #[test]
    fn infantry_plain_zero_luck() {
        let hit = damage(infantry(100, 0, 0), infantry(100, 0, 1), 0).unwrap();
        assert_eq!(hit.damage, 49);
        assert_eq!(hit.weapon.weapon, Weapon::Unlimited);
        assert_eq!(hit.weapon.ammo_cost, 0)
    }
    #[test]
    fn counter_uses_damaged_exact_hp() {
        let c = resolve(infantry(100, 0, 0), infantry(100, 0, 0), 0, Some(0), true).unwrap();
        assert_eq!(c.attack.damage, 55);
        assert_eq!(c.counter.unwrap().damage, 24)
    }
    #[test]
    fn tank_falls_back_at_zero_ammo() {
        assert_eq!(
            select_weapon(UnitKind::Tank, UnitKind::Infantry, 0).unwrap(),
            SelectedWeapon {
                weapon: Weapon::Unlimited,
                base_damage: 75,
                ammo_cost: 0
            }
        );
        assert_eq!(
            select_weapon(UnitKind::Tank, UnitKind::Tank, 1)
                .unwrap()
                .weapon,
            Weapon::Ammo
        );
        assert_eq!(
            select_weapon(UnitKind::Tank, UnitKind::Infantry, 9).unwrap(),
            SelectedWeapon {
                weapon: Weapon::Unlimited,
                base_damage: 75,
                ammo_cost: 0
            }
        );
        assert_eq!(
            select_weapon(UnitKind::Tank, UnitKind::Tank, 0)
                .unwrap()
                .weapon,
            Weapon::Unlimited
        );
    }
    #[test]
    fn lethal_hit_has_no_counter() {
        let c = resolve(
            Side {
                kind: UnitKind::Bomber,
                hp: 100,
                ammo: 9,
                attack: 100,
                defense: 100,
                terrain_stars: 0,
            },
            infantry(12, 0, 0),
            0,
            Some(0),
            true,
        )
        .unwrap();
        assert_eq!(c.attack.damage, 12);
        assert!(c.counter.is_none())
    }
}
