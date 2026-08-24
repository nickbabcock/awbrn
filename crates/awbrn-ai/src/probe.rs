//! What a commander is worth, in the units the evaluation already speaks.
//!
//! The agent prices an exchange in funds, and it reads one number to do it:
//! the share of a whole defender that one strike of an attacker removes. A
//! commander moves that number, and nothing in the agent knows it. Today the
//! whole of the agent's knowledge of a commander is a flat 200 for a power
//! whenever the position offers one, and every table it builds is written
//! against a commander with no combat rule at all.
//!
//! A probe is the other way around from a rule. Rather than teaching the
//! evaluation what twenty-nine commanders do — which is twenty-nine rules to
//! keep, and a rule for each one added later — it **measures** them: sweep the
//! ruleset's own calculator over every ordered pair of unit kinds, over the
//! terrain each kind fights on and over the weather, once with the commander
//! in command and once with nobody, and divide. What comes back is a matrix of
//! multipliers, and a multiplier is a thing the evaluation can already read.
//!
//! No agent then names a commander, which is what `awvm::commander` refuses to
//! allow anyway: the operators are the ruleset's, and a rule this file cannot
//! see is a rule the sweep reads all the same.
//!
//! Two readings for each commander and power state:
//!
//! - **Attack.** [`Probe::attack`] is what a kind of ours takes off the
//!   average defender, against what the same kind takes off it under no
//!   commander. Above one is a commander whose army hits harder.
//! - **Defense.** [`Probe::defense`] is what the average attacker takes off a
//!   kind of ours. **Below** one is the commander that is harder to hurt, so
//!   this is a cost and not a gain, and it is read the way the threat map is
//!   read.
//!
//! Both are means over the pairs the calculator can score, and the matrix
//! behind them is kept, so a table that wants one pair does not read the mean.
//!
//! **What the sweep holds still.** A commander whose damage reads the treasury
//! or the property count — Colin, Hawke, Kindle — is measured at
//! [`TREASURY`] funds and [`PROPERTIES`] properties, and reports what it does
//! *there*. That is a choice of operating point rather than a rule, which is
//! the honest shape for a probe: change the constant and the report changes
//! with it.

use awvm::calculator::{self, BattleRequest, Fighter, SideContext};
use awvm::commander::PowerLevel;
use awvm::ruleset::{self, CommanderKind, Domain, Terrain, UnitKind, WeatherKind};

/// The funds each side is measured holding.
///
/// A commander that reads the treasury is one number at nothing and another at
/// a full purse. This is a middle game purse: enough that a treasury rule is
/// on the board, not so much that it is the whole board.
pub const TREASURY: u64 = 10_000;

/// The properties each side is measured holding, none of them a com tower.
pub const PROPERTIES: u64 = 10;

/// The ground a kind of each domain is measured on.
///
/// Terrain reaches the damage through the defender's stars, so the sweep asks
/// about the ground each kind actually stands on: a tank is not measured at
/// sea and a battleship is not measured on a mountain. The air domain takes no
/// stars from anything, so one tile answers for it.
fn terrains(domain: Domain) -> &'static [Terrain] {
    const GROUND: [Terrain; 5] = [
        Terrain::Plain,
        Terrain::Road,
        Terrain::Wood,
        Terrain::Mountain,
        Terrain::City,
    ];
    const SEA: [Terrain; 2] = [Terrain::Sea, Terrain::Reef];
    const AIR: [Terrain; 1] = [Terrain::Plain];
    match domain {
        Domain::Ground => &GROUND,
        Domain::Sea => &SEA,
        Domain::Air => &AIR,
    }
}

/// The weather the sweep runs under.
const WEATHERS: [WeatherKind; 3] = [WeatherKind::Clear, WeatherKind::Rain, WeatherKind::Snow];

/// Mean damage for each ordered pair of kinds, in points of health.
///
/// One entry for each `attacker * defender`, and `None` for a pair the
/// calculator cannot score: an anti-air does not reach a submarine, and an APC
/// never fires at anything.
#[derive(Clone, Debug, PartialEq)]
pub struct DamageMatrix {
    rows: Vec<Option<f64>>,
}

impl DamageMatrix {
    /// Sweep every pair, with `attacker` and `defender` in command of their
    /// own side.
    ///
    /// The damage is averaged over the terrain the defender stands on and over
    /// the weather, because a commander is not a rule about one tile. The
    /// attacker stands on a plain: its own terrain reaches its damage through
    /// nothing, and reaches the reply, which this does not read.
    pub fn sweep(attacker: SideContext, defender: SideContext) -> Self {
        let mut rows = vec![None; UnitKind::COUNT * UnitKind::COUNT];
        for shooter in UnitKind::ALL {
            let profile = ruleset::profile(shooter);
            for weather in WEATHERS {
                // One request for every target at once: the calculator scores
                // an attacker against a list, which is what it is for.
                let mut targets = Vec::new();
                for target in UnitKind::ALL {
                    for terrain in terrains(ruleset::profile(target).domain) {
                        targets.push(Fighter {
                            unit: target,
                            hp: 100,
                            ammo: None,
                            terrain: *terrain,
                        });
                    }
                }
                let request = BattleRequest {
                    weather,
                    attacker,
                    attacking_unit: Fighter {
                        unit: shooter,
                        hp: 100,
                        ammo: Some(profile.max_ammo),
                        terrain: Terrain::Plain,
                    },
                    defender,
                    defending_units: targets,
                };
                let Ok(report) = calculator::forecast(&request) else {
                    continue;
                };
                let mut count = [0u32; UnitKind::COUNT];
                let mut total = [0.0f64; UnitKind::COUNT];
                for outcome in &report.outcomes {
                    let Ok(engagement) = &outcome.engagement else {
                        continue;
                    };
                    let mean = f64::from(engagement.damage.low + engagement.damage.high) / 2.0;
                    total[outcome.target.unit.index()] += mean;
                    count[outcome.target.unit.index()] += 1;
                }
                for target in UnitKind::ALL {
                    let index = target.index();
                    if count[index] == 0 {
                        continue;
                    }
                    let mean = total[index] / f64::from(count[index]);
                    let cell = &mut rows[shooter.index() * UnitKind::COUNT + index];
                    // Each weather is one reading of the same pair, so the
                    // pair's entry is the mean over the weathers that scored.
                    *cell = Some(match *cell {
                        Some(held) => held + mean,
                        None => mean,
                    });
                }
            }
        }
        // Three weathers went in, so what came out is three readings deep.
        for cell in rows.iter_mut().flatten() {
            *cell /= WEATHERS.len() as f64;
        }
        Self { rows }
    }

    /// Mean damage of one pair, or `None` where the pairing cannot be scored.
    pub fn get(&self, attacker: UnitKind, defender: UnitKind) -> Option<f64> {
        self.rows[attacker.index() * UnitKind::COUNT + defender.index()]
    }
}

/// One side under no commander at all, which is what every ratio is against.
///
/// `None` is the ruleset's own neutral commander rather than the absence of
/// one: com tower and treasury effects are part of the commander algebra, and
/// a side outside it would report a tower as worth nothing.
pub fn neutral() -> SideContext {
    SideContext {
        commander: None,
        power: None,
        funds: TREASURY,
        properties: PROPERTIES,
        com_towers: 0,
    }
}

/// One side under `commander`, at `power`.
pub fn commanding(commander: CommanderKind, power: Option<PowerLevel>) -> SideContext {
    SideContext {
        commander: Some(commander),
        power,
        ..neutral()
    }
}

/// What one commander does to the numbers the evaluation reads.
#[derive(Clone, Debug, PartialEq)]
pub struct Probe {
    pub commander: CommanderKind,
    pub power: Option<PowerLevel>,
    /// Damage dealt with this commander attacking, over the same pair under
    /// no commander.
    dealt: Vec<Option<f64>>,
    /// Damage taken with this commander defending, over the same.
    taken: Vec<Option<f64>>,
}

impl Probe {
    /// Sweep one commander at one power state.
    ///
    /// Two sweeps and a division: the commander attacking a neutral defender,
    /// and a neutral attacker against the commander defending. A commander
    /// that reads both sides of an exchange — most of them do, through the
    /// defense operators — is therefore read twice, once from each end, which
    /// is how the evaluation reads it as well.
    pub fn of(commander: CommanderKind, power: Option<PowerLevel>) -> Self {
        let base = DamageMatrix::sweep(neutral(), neutral());
        let attacking = DamageMatrix::sweep(commanding(commander, power), neutral());
        let defending = DamageMatrix::sweep(neutral(), commanding(commander, power));
        let ratio = |over: &DamageMatrix| {
            let mut rows = Vec::with_capacity(UnitKind::COUNT * UnitKind::COUNT);
            for attacker in UnitKind::ALL {
                for defender in UnitKind::ALL {
                    let cell = match (over.get(attacker, defender), base.get(attacker, defender)) {
                        // A pair the neutral commander lands nothing on says
                        // nothing about a multiplier, whatever the commander
                        // does to it.
                        (Some(theirs), Some(neutral)) if neutral > 0.0 => Some(theirs / neutral),
                        _ => None,
                    };
                    rows.push(cell);
                }
            }
            rows
        };
        Self {
            commander,
            power,
            dealt: ratio(&attacking),
            taken: ratio(&defending),
        }
    }

    fn cell(rows: &[Option<f64>], attacker: UnitKind, defender: UnitKind) -> Option<f64> {
        rows[attacker.index() * UnitKind::COUNT + defender.index()]
    }

    /// The multiplier on what `attacker` of ours takes off `defender`.
    pub fn strike(&self, attacker: UnitKind, defender: UnitKind) -> Option<f64> {
        Self::cell(&self.dealt, attacker, defender)
    }

    /// The multiplier on what `attacker` takes off `defender` of ours.
    pub fn reply(&self, attacker: UnitKind, defender: UnitKind) -> Option<f64> {
        Self::cell(&self.taken, attacker, defender)
    }

    /// What a kind of ours hits for, over the defenders it can hit.
    ///
    /// Above one is a commander whose army hits harder than nobody's.
    pub fn attack(&self, kind: UnitKind) -> f64 {
        mean(
            UnitKind::ALL
                .into_iter()
                .filter_map(|defender| self.strike(kind, defender)),
        )
    }

    /// What a kind of ours is hit for, over the attackers that can hit it.
    ///
    /// **Below** one is the commander that is harder to hurt.
    pub fn defense(&self, kind: UnitKind) -> f64 {
        mean(
            UnitKind::ALL
                .into_iter()
                .filter_map(|attacker| self.reply(attacker, kind)),
        )
    }

    /// The army as a whole, hitting.
    pub fn offense(&self) -> f64 {
        mean(UnitKind::ALL.into_iter().map(|kind| self.attack(kind)))
    }

    /// The army as a whole, being hit.
    pub fn resilience(&self) -> f64 {
        mean(UnitKind::ALL.into_iter().map(|kind| self.defense(kind)))
    }
}

/// The mean of what is there, and one for nothing at all.
///
/// One rather than zero: this is a multiplier, and a commander with nothing to
/// say about a kind leaves that kind where the ruleset put it.
fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let mut total = 0.0;
    let mut count = 0u32;
    for value in values {
        if !value.is_finite() {
            continue;
        }
        total += value;
        count += 1;
    }
    if count == 0 {
        return 1.0;
    }
    total / f64::from(count)
}

/// Whether a kind's weapon reaches past the tile beside it.
///
/// The probe reports by kind and not by role, and the two readings a commander
/// most often splits are the direct one and the indirect one, so the report
/// groups the kinds the way the commanders do.
pub fn is_indirect(kind: UnitKind) -> bool {
    ruleset::profile(kind).indirect_range.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mean multiplier over the kinds a predicate names.
    fn over(probe: &Probe, keep: impl Fn(UnitKind) -> bool, attacking: bool) -> f64 {
        mean(
            UnitKind::ALL
                .into_iter()
                .filter(|kind| keep(*kind))
                .map(|kind| {
                    if attacking {
                        probe.attack(kind)
                    } else {
                        probe.defense(kind)
                    }
                }),
        )
    }

    /// A kind that fires at the tile beside it, which is every armed kind
    /// that is not an indirect one.
    fn direct(kind: UnitKind) -> bool {
        let profile = ruleset::profile(kind);
        !is_indirect(kind) && (profile.ammo_weapon.is_some() || profile.unlimited_weapon.is_some())
    }

    /// The commander the arena plays has no combat rule day to day, so the
    /// probe must find nothing at all. This is the control: a probe that
    /// reported a number here would be measuring its own scaffolding.
    #[test]
    fn the_commander_with_no_combat_rule_reads_one_everywhere() {
        let probe = Probe::of(CommanderKind::Andy, None);
        for attacker in UnitKind::ALL {
            for defender in UnitKind::ALL {
                for reading in [
                    probe.strike(attacker, defender),
                    probe.reply(attacker, defender),
                ] {
                    let Some(value) = reading else { continue };
                    assert!(
                        (value - 1.0).abs() < 1e-9,
                        "{attacker:?} against {defender:?} reads {value}"
                    );
                }
            }
        }
    }

    /// Max hits harder with what shoots the tile beside it and worse with
    /// what shoots over one, and Grit is the other way around. That is the
    /// pair of rules every commander table in the game opens with, and it is
    /// what says the sweep is reading the ruleset rather than a constant.
    #[test]
    fn the_probe_finds_the_two_rules_every_commander_table_opens_with() {
        let max = Probe::of(CommanderKind::Max, None);
        assert!(
            over(&max, direct, true) > 1.0,
            "Max reads {} attacking with direct fire",
            over(&max, direct, true)
        );
        assert!(
            over(&max, is_indirect, true) < 1.0,
            "Max reads {} attacking with indirect fire",
            over(&max, is_indirect, true)
        );

        let grit = Probe::of(CommanderKind::Grit, None);
        assert!(
            over(&grit, is_indirect, true) > 1.0,
            "Grit reads {} attacking with indirect fire",
            over(&grit, is_indirect, true)
        );
        assert!(
            over(&grit, direct, true) < 1.0,
            "Grit reads {} attacking with direct fire",
            over(&grit, direct, true)
        );
    }

    /// A power is worth more than the day it is called on.
    #[test]
    fn a_power_moves_the_numbers_the_day_to_day_reading_holds() {
        let day = Probe::of(CommanderKind::Max, None);
        let cop = Probe::of(CommanderKind::Max, Some(PowerLevel::Cop));
        let scop = Probe::of(CommanderKind::Max, Some(PowerLevel::Scop));
        assert!(
            cop.offense() > day.offense(),
            "a power that does not hit harder than the day is not a power: {} against {}",
            cop.offense(),
            day.offense()
        );
        assert!(scop.offense() > day.offense());
    }

    /// Kanbei's army is dearer, hits harder and is harder to hit, which is
    /// the one commander that moves both readings in the good direction.
    #[test]
    fn a_commander_that_reads_both_ends_of_an_exchange_is_read_from_both() {
        let kanbei = Probe::of(CommanderKind::Kanbei, None);
        assert!(
            kanbei.offense() > 1.0,
            "Kanbei attacks at {}",
            kanbei.offense()
        );
        assert!(
            kanbei.resilience() < 1.0,
            "Kanbei is hit for {}",
            kanbei.resilience()
        );
    }
}
