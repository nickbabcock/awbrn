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
    /// The same strike before it is limited by what the target still has.
    ///
    /// Nothing in the reducer uses this: a unit cannot lose more health than it
    /// has, so [`Hit::damage`] is what lands. It is reported because the margin
    /// is real information to a player choosing an attacker — 101 and 160
    /// against the same target are a bare kill and an overkill worth spending
    /// something cheaper on, and clamping both to 100 hides the difference.
    pub raw_damage: u16,
    pub weapon: SelectedWeapon,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Combat {
    pub attack: Hit,
    pub counter: Option<Hit>,
}

/// The damage one strike lands, from its unluckiest roll to its luckiest.
///
/// Values are percentage points of a unit at full health, reported the way
/// [`Hit::raw_damage`] is: not limited by what the target still has, so the
/// overkill margin survives and a bare kill is distinguishable from a rout.
/// `low` equals `high` whenever no commander in the exchange grants luck.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DamageRange {
    pub low: u16,
    pub high: u16,
}

/// What an exchange can cost both sides, before a single roll is drawn.
///
/// The two ranges are not independent. A counter is scored from what the
/// initiating strike left standing. The attacker's best surviving roll bounds
/// the reply's worst, and its worst roll bounds the reply's best. If the best
/// roll destroys the defender, the reply is not made at all and `counter.low`
/// is zero. Read that case from `attack.high` against `target_hp` rather than
/// from the zero itself, because a live defender too weak to scratch the
/// attacker also answers with nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Forecast {
    pub attack: DamageRange,
    /// `None` when nothing can answer: an indirect strike, a target with no
    /// weapon that bites back, a destructible tile, or a target that does not
    /// survive the attacker's weakest roll.
    pub counter: Option<DamageRange>,
    /// The counter range split by each possible visible health bar.
    ///
    /// These are the healths the defender answers from, so they cover survival
    /// alone: a strike that destroys it is the absent rung, not a rung of zero.
    /// This is empty when no counter occurs or when the counter occurs first.
    pub counter_steps: Vec<CounterStep>,
    /// Whether the defender's commander fires before the strike that provoked
    /// it, which is what makes the attacker's own damage depend on the reply.
    pub counter_first: bool,
    /// Both sides' health before the exchange, on the same 0-100 scale, so a
    /// caller can say whether a strike destroys without holding the board.
    pub attacker_hp: u8,
    pub target_hp: u8,
}

/// What a defender answers with, from one of the healths it may be left at.
///
/// A single counter range carries two different kinds of spread at once: how
/// much of the defender survives the strike, and the luck its reply is scored
/// with. A player reading `27 - 36%` cannot tell which part is which, and the
/// two are not equally useful — the surviving health is the part they can
/// reason about, because it follows from a roll they are about to watch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CounterStep {
    /// The health in points the defender is left standing at: the top of the
    /// bar the board would draw, so a caller converts it to bars exactly the
    /// way it converts every other health it holds.
    pub target_hp: u8,
    /// What it answers with from there, across the luck alone.
    pub counter: DamageRange,
}

/// A whole bar, in the points the reducer counts.
pub(crate) const HEALTH_STEP: u8 = 10;

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
    let raw = (attack_hp_factor * defense_numerator / 100 / 100).max(0);
    let points = raw.min(i64::from(defender.hp));
    Some(Hit {
        damage: points as u8,
        raw_damage: u16::try_from(raw).unwrap_or(u16::MAX),
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
