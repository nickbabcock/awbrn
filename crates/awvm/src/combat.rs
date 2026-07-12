//! Pure AWBW weapon selection and exact-HP combat arithmetic.

use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weapon {
    Ammo,
    Unlimited,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectedWeapon {
    pub weapon: Weapon,
    pub base_damage: u64,
    pub ammo_cost: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Side<'a> {
    pub kind: &'a str,
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

pub fn select_weapon(attacker: &str, defender: &str, ammo: u64) -> Option<SelectedWeapon> {
    let table: Value = serde_json::from_str(include_str!(
        "../../../spec/rulesets/awbw/2026-07-10/weapons.json"
    ))
    .expect("embedded weapons table");
    let unit = &table["units"][attacker];
    for (name, weapon) in [("ammo", Weapon::Ammo), ("unlimited", Weapon::Unlimited)] {
        let entry = &unit[name];
        let Some(cost) = entry["ammo_cost"].as_u64() else {
            continue;
        };
        if cost <= ammo
            && let Some(base_damage) = entry["damage"][defender].as_u64()
        {
            return Some(SelectedWeapon {
                weapon,
                base_damage,
                ammo_cost: cost,
            });
        }
    }
    None
}

/// AWBW integer damage formula. Luck is the already-mapped signed modifier.
pub fn damage(attacker: Side<'_>, defender: Side<'_>, luck: i64) -> Option<Hit> {
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
    attacker: Side<'_>,
    defender: Side<'_>,
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
    fn infantry(hp: u8, ammo: u64, stars: u8) -> Side<'static> {
        Side {
            kind: "infantry",
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
            select_weapon("tank", "infantry", 0).unwrap(),
            SelectedWeapon {
                weapon: Weapon::Unlimited,
                base_damage: 75,
                ammo_cost: 0
            }
        );
        assert_eq!(
            select_weapon("tank", "tank", 1).unwrap().weapon,
            Weapon::Ammo
        );
        assert_eq!(
            select_weapon("tank", "infantry", 9).unwrap(),
            SelectedWeapon {
                weapon: Weapon::Unlimited,
                base_damage: 75,
                ammo_cost: 0
            }
        );
        assert_eq!(
            select_weapon("tank", "tank", 0).unwrap().weapon,
            Weapon::Unlimited
        );
    }
    #[test]
    fn lethal_hit_has_no_counter() {
        let c = resolve(
            Side {
                kind: "bomber",
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
