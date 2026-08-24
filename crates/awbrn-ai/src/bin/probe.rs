//! What every commander is worth, printed.
//!
//! The evaluation prices an exchange in funds, off the share of a defender one
//! strike removes. A commander moves that share and no agent reads which
//! commander is in command, so this reports each of them as a multiplier on
//! the number the evaluation already speaks — measured off the ruleset's own
//! calculator rather than written down as a rule. See [`awbrn_ai::probe`].
//!
//! With no arguments it prints one row for each commander and power state: the
//! army as a whole, and the direct and indirect halves that most commander
//! rules split on. `--commander NAME` prints that commander by unit kind, and
//! the pairs it moves furthest.

use awbrn_ai::probe::{PROPERTIES, Probe, TREASURY, is_indirect};
use awvm::commander::PowerLevel;
use awvm::ruleset::{self, CommanderKind, UnitKind};

const USAGE: &str = "\
usage: probe [--commander NAME] [--power day|cop|scop]

  --commander NAME  Report one commander by unit kind rather than every
                    commander as a summary.
  --power LEVEL     Which power state to report a named commander at.
                    Default day, which is day to day.
";

fn main() {
    let mut commander: Option<CommanderKind> = None;
    let mut power: Option<PowerLevel> = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--commander" => {
                let name = args.next().unwrap_or_default();
                commander = CommanderKind::ALL
                    .into_iter()
                    .find(|known| known.as_str() == name);
                if commander.is_none() {
                    eprintln!("unknown commander {name:?}\n\n{USAGE}");
                    std::process::exit(2);
                }
            }
            "--power" => {
                power = match args.next().unwrap_or_default().as_str() {
                    "day" => None,
                    "cop" => Some(PowerLevel::Cop),
                    "scop" => Some(PowerLevel::Scop),
                    other => {
                        eprintln!("unknown power state {other:?}\n\n{USAGE}");
                        std::process::exit(2);
                    }
                };
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                return;
            }
            other => {
                eprintln!("unexpected argument {other:?}\n\n{USAGE}");
                std::process::exit(2);
            }
        }
    }

    header();
    match commander {
        Some(commander) => one(commander, power),
        None => every(),
    }
}

fn header() {
    println!("commander probe   funds {TREASURY}  properties {PROPERTIES}  no com towers");
    println!(
        "every ordered pair of unit kinds, over the ground each kind stands on\n\
         and over clear, rain and snow, against the ruleset's neutral commander\n"
    );
}

/// The name a power state answers to.
fn level(power: Option<PowerLevel>) -> &'static str {
    match power {
        None => "day",
        Some(PowerLevel::Cop) => "cop",
        Some(PowerLevel::Scop) => "scop",
    }
}

/// The mean over the kinds a predicate names.
fn over(reading: impl Fn(UnitKind) -> f64, keep: impl Fn(UnitKind) -> bool) -> f64 {
    let mut total = 0.0;
    let mut count = 0u32;
    for kind in UnitKind::ALL.into_iter().filter(|kind| keep(*kind)) {
        total += reading(kind);
        count += 1;
    }
    if count == 0 {
        return 1.0;
    }
    total / f64::from(count)
}

/// A kind that fires at the tile beside it.
fn direct(kind: UnitKind) -> bool {
    let profile = ruleset::profile(kind);
    !is_indirect(kind) && (profile.ammo_weapon.is_some() || profile.unlimited_weapon.is_some())
}

/// Every commander, at every power state, as one summary row each.
fn every() {
    println!(
        "{:<9} {:<5} {:>7} {:>7} {:>9} {:>7} {:>7} {:>9}",
        "", "", "hits", "direct", "indirect", "is hit", "direct", "indirect"
    );
    for commander in CommanderKind::ALL {
        for power in [None, Some(PowerLevel::Cop), Some(PowerLevel::Scop)] {
            let probe = Probe::of(commander, power);
            println!(
                "{:<9} {:<5} {:>7.3} {:>7.3} {:>9.3} {:>7.3} {:>7.3} {:>9.3}",
                commander.as_str(),
                level(power),
                probe.offense(),
                over(|kind| probe.attack(kind), direct),
                over(|kind| probe.attack(kind), is_indirect),
                probe.resilience(),
                over(|kind| probe.defense(kind), direct),
                over(|kind| probe.defense(kind), is_indirect),
            );
        }
    }
    println!(
        "\n`hits` is what this army takes off the average defender, and `is hit`\n\
         what the average attacker takes off it. Above one is more damage, so a\n\
         commander is stronger the higher the first and the lower the second."
    );
}

/// One commander, by unit kind, and the pairs it moves furthest.
fn one(commander: CommanderKind, power: Option<PowerLevel>) {
    let probe = Probe::of(commander, power);
    println!("{} at {}\n", commander.as_str(), level(power));
    println!("{:<12} {:>7} {:>7}", "kind", "hits", "is hit");
    for kind in UnitKind::ALL {
        let attack = probe.attack(kind);
        let defense = probe.defense(kind);
        if (attack - 1.0).abs() < 1e-9 && (defense - 1.0).abs() < 1e-9 {
            continue;
        }
        println!("{:<12} {attack:>7.3} {defense:>7.3}", kind.as_str());
    }

    let mut pairs: Vec<(f64, UnitKind, UnitKind)> = Vec::new();
    for attacker in UnitKind::ALL {
        for defender in UnitKind::ALL {
            if let Some(ratio) = probe.strike(attacker, defender) {
                pairs.push((ratio, attacker, defender));
            }
        }
    }
    pairs.sort_by(|left, right| {
        (right.0 - 1.0)
            .abs()
            .total_cmp(&(left.0 - 1.0).abs())
            .then_with(|| left.1.index().cmp(&right.1.index()))
    });
    if pairs
        .first()
        .is_some_and(|(ratio, _, _)| (ratio - 1.0).abs() < 1e-9)
    {
        println!("\nthis commander moves no pair: it holds no combat rule here");
        return;
    }
    println!("\nthe pairs it moves furthest, attacking");
    for (ratio, attacker, defender) in pairs.iter().take(10) {
        println!(
            "  {:<12} against {:<12} {ratio:>7.3}",
            attacker.as_str(),
            defender.as_str()
        );
    }
    println!(
        "\nnothing here is priced. A multiplier is what the evaluation reads;\n\
         what it is worth to hold is a weight, and no agent names a commander."
    );
}
