//! A single engagement, scored outside any match.
//!
//! The board answers "what does this attack cost" only for attacks a player is
//! actually able to make right now. A calculator asks the same question about an
//! engagement that does not exist: a unit the player has not built, standing on
//! terrain it has not reached, under a commander who is not theirs.
//!
//! Nothing here is a second combat model. The request is lowered into an
//! ordinary [`State`] — two players, a board, two units on it — and the same
//! [`transition::forecast_unit_attack`] that answers for a real order answers
//! for this one. Weapon selection, commander attack and defense algebra, com
//! tower and funds and property modifiers, effective enemy terrain stars, the
//! `counter-first` inversion and the correlation between the two damage ranges
//! all come from the code that resolves a real shot, because a calculator that
//! recomputed any of them would drift from the game it is advising on.
//!
//! What this module owns is the lowering, and one piece of arithmetic the board
//! has no use for: damage priced in funds. A player choosing between two
//! attacks is choosing between two trades, and a percentage cannot say whether
//! 60% of a Mega Tank is worth 90% of a Recon.

use crate::combat::{CounterStep, DamageRange, Weapon};
use crate::commander::{Holdings, PowerLevel};
use crate::ruleset::{self, CommanderKind, Terrain, UnitKind};
use crate::semantic::{
    Board, Commander, Concealment, Location, Match, Phase, Player, PlayerId, PlayerIdx, Pos,
    PowerState, Roster, RulesetRef, Settings, State, Team, TeamStatus, Tile, TileOwner, Turn, Unit,
    UnitAction, UnitId, UnitStore, Weather, WeatherKind,
};
use crate::transition;

/// The two seats, named. A calculator has no user accounts to borrow ids from.
const ATTACKING_PLAYER: &str = "attacker";
const DEFENDING_PLAYER: &str = "defender";
/// Where each side sits on the roster this module builds.
const ATTACKER_SEAT: PlayerIdx = PlayerIdx::from_seat(0);
const DEFENDER_SEAT: PlayerIdx = PlayerIdx::from_seat(1);

/// How many owned tiles one side may claim.
///
/// Both bounds exist to keep the synthetic board inside a `u8` coordinate
/// rather than to express a rule: the largest AWBW maps hold far fewer
/// properties than this, so a request at the limit is a malformed one.
const MAX_PROPERTIES: u64 = 200;

/// Everything one army brings to an engagement that is not the unit itself.
///
/// These are exactly the values the commander operators read: who is in
/// command, whether a power is running, the treasury, the property count, and
/// how many of those properties are com towers. Nothing else about a player
/// reaches the damage formula.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SideContext {
    /// `None` fights under the ruleset's own neutral commander, not under no
    /// commander at all. The distinction is not pedantic: a player with no
    /// commander is outside the commander algebra entirely, and com tower and
    /// treasury effects are part of that algebra rather than beside it, so a
    /// state with an absent commander would report a tower as worth nothing.
    pub commander: Option<CommanderKind>,
    /// `None` is day-to-day. A power the commander does not have still resolves;
    /// the profile simply has no rules to contribute at that level.
    pub power: Option<PowerLevel>,
    pub funds: u64,
    /// Every capturable tile this army holds, com towers included, the way AWBW
    /// counts a property. `com_towers` names how many of them are towers rather
    /// than adding to this figure.
    pub properties: u64,
    pub com_towers: u64,
}

/// One unit in the engagement, and the ground under it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fighter {
    pub unit: UnitKind,
    /// Health in points on the 0-100 scale the reducer works in, not the 1-10
    /// figure the board draws. A unit at 7 bars is 70 here, and a unit that took
    /// exact damage in a real match keeps the exact number it has.
    pub hp: u8,
    /// `None` takes the unit's full magazine, which is what an unspecified unit
    /// in a calculator means. It is not cosmetic: a Tank out of shells fires its
    /// machine gun instead, and against another Tank that is a different attack.
    pub ammo: Option<u64>,
    pub terrain: Terrain,
}

/// One attacker, its context, and every target it is being weighed against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BattleRequest {
    pub weather: WeatherKind,
    pub attacker: SideContext,
    pub attacking_unit: Fighter,
    pub defender: SideContext,
    /// Scored independently, one engagement each. They share the defending
    /// army's context because a calculator compares targets, not armies.
    pub defending_units: Vec<Fighter>,
}

/// Funds at one end of a damage range.
///
/// A range rather than a number for the same reason the damage is: luck moves
/// it. The two ends pair with the damage ends they were priced from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FundsRange {
    pub low: u64,
    pub high: u64,
}

/// The funds an exchange moves, from the attacking player's seat.
///
/// Signed, and paired honestly: `low` is the worst outcome for the attacker —
/// its weakest roll against the reply's strongest — and `high` is the best.
/// Pairing them any other way describes an exchange that cannot happen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NetFunds {
    pub low: i64,
    pub high: i64,
}

/// Why an engagement could not be scored.
///
/// It is a fact about the pairing rather than an error: an Anti-Air cannot
/// reach a Submarine, and a calculator that hid the row would be answering a
/// different question than the one asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unscorable {
    /// The attacker holds no weapon with a damage entry against this target.
    NoWeapon,
    /// The attacker has no weapon at all. An APC never fights.
    Unarmed,
}

/// One engagement, scored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Engagement {
    /// Which weapon fires, so a fallback to the secondary is visible rather
    /// than inferred from a number that came out lower than expected.
    pub weapon: Weapon,
    /// Damage in percentage points, uncapped, as AWBW reports it: 160 against a
    /// whole unit is an overkill and 101 is a bare kill, and clamping both to
    /// 100 would make them indistinguishable.
    pub damage: DamageRange,
    /// What comes back, when anything can. `None` is not a counter of zero: an
    /// indirect draws no reply at all, and that is a different fact from a reply
    /// that happens to land nothing. When [`Engagement::may_destroy`] is set the
    /// low end is zero, because the roll that finishes the target is the roll
    /// that answers with nothing.
    pub counter: Option<DamageRange>,
    /// Whether the defending commander answers before the shot that provoked
    /// it, which is what makes the attacker's own damage depend on the reply.
    pub counter_first: bool,
    /// Whether even the weakest roll finishes the target.
    pub destroys: bool,
    /// Whether the strongest roll finishes it, when the weakest does not.
    pub may_destroy: bool,
    /// Funds taken off the target. Priced on what lands, not on the raw figure:
    /// overkill is real information about the attack and no information at all
    /// about the trade, because a destroyed unit costs its owner what it was
    /// worth and not a coin more.
    pub value_dealt: FundsRange,
    /// Funds the reply takes off the attacker, absent when nothing replies.
    pub value_taken: Option<FundsRange>,
    /// The reply again, one rung per health the target may be left standing
    /// in, so the health spread and the luck spread can be told apart. Every
    /// rung is a health the target is alive at, so the outcome where it dies
    /// and answers with nothing is the missing rung rather than a rung of zero.
    /// Empty when there is no spread to break out: a reply that lands first is
    /// scored at full health, and a target that never survives never answers.
    pub counter_steps: Vec<CounterStep>,
    /// What the target is worth whole, at the health it currently has.
    pub target_value: u64,
    pub net: NetFunds,
}

/// One row of the report: the target, and what happens to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BattleOutcome {
    pub target: Fighter,
    pub engagement: Result<Engagement, Unscorable>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BattleReport {
    /// What the attacker is worth at the health it is fighting at, which is the
    /// figure every `value_taken` is a fraction of.
    pub attacker_value: u64,
    pub outcomes: Vec<BattleOutcome>,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CalculatorError {
    #[error("health must be between 1 and 100, got {0}")]
    Health(u8),
    #[error("a side may not hold more than {MAX_PROPERTIES} properties, got {0}")]
    Properties(u64),
    #[error("com towers ({towers}) exceed the properties that would hold them ({properties})")]
    ComTowers { towers: u64, properties: u64 },
    #[error("the engagement could not be laid out: {0}")]
    Layout(String),
}

/// Score one attacker against every target it is being weighed against.
pub fn forecast(request: &BattleRequest) -> Result<BattleReport, CalculatorError> {
    validate(request)?;

    let attacker_value = value_of(request.attacking_unit);
    let outcomes = request
        .defending_units
        .iter()
        .map(|target| {
            score(request, *target).map(|engagement| BattleOutcome {
                target: *target,
                engagement,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(BattleReport {
        attacker_value,
        outcomes,
    })
}

fn validate(request: &BattleRequest) -> Result<(), CalculatorError> {
    for fighter in std::iter::once(&request.attacking_unit).chain(&request.defending_units) {
        if fighter.hp == 0 || fighter.hp > 100 {
            return Err(CalculatorError::Health(fighter.hp));
        }
    }
    for side in [&request.attacker, &request.defender] {
        if side.properties > MAX_PROPERTIES {
            return Err(CalculatorError::Properties(side.properties));
        }
        if side.com_towers > side.properties {
            return Err(CalculatorError::ComTowers {
                towers: side.com_towers,
                properties: side.properties,
            });
        }
    }
    Ok(())
}

/// What a unit is worth at the health it is standing at.
///
/// AWBW prices a damaged unit down by its visible bars, so a half-dead Md Tank
/// is worth half a Md Tank. The exact-HP scale divides by 100 rather than by 10
/// because the reducer counts in points.
fn value_of(fighter: Fighter) -> u64 {
    ruleset::profile(fighter.unit)
        .cost
        .saturating_mul(u64::from(fighter.hp))
        / 100
}

/// The funds one strike removes, at both ends of its range.
///
/// Damage is clamped to what the target has before it is priced. The raw figure
/// says how decisively an attack lands; it does not say what the attack is
/// worth, because nothing is paid for the part of a shot that hits a unit that
/// has already been destroyed.
fn price(target: Fighter, damage: DamageRange) -> FundsRange {
    let cost = ruleset::profile(target.unit).cost;
    let landed = |points: u16| {
        let capped = u64::from(points).min(u64::from(target.hp));
        cost.saturating_mul(capped) / 100
    };
    FundsRange {
        low: landed(damage.low),
        high: landed(damage.high),
    }
}

fn score(
    request: &BattleRequest,
    target: Fighter,
) -> Result<Result<Engagement, Unscorable>, CalculatorError> {
    let attacker_profile = ruleset::profile(request.attacking_unit.unit);
    if attacker_profile.ammo_weapon.is_none() && attacker_profile.unlimited_weapon.is_none() {
        return Ok(Err(Unscorable::Unarmed));
    }
    let ammo = request
        .attacking_unit
        .ammo
        .unwrap_or(attacker_profile.max_ammo);
    let Some(weapon) = crate::combat::select_weapon(request.attacking_unit.unit, target.unit, ammo)
    else {
        return Ok(Err(Unscorable::NoWeapon));
    };

    let (state, attacker_id, defender_id) = lay_out(request, target)?;
    let attacker_index = state
        .units
        .index_of(attacker_id)
        .ok_or_else(|| CalculatorError::Layout("the attacker was not placed".into()))?;
    let origin = attacker_position(request);
    let forecast = transition::forecast_unit_attack(
        &state,
        &Holdings::tally(&state),
        &PlayerId::from(ATTACKING_PLAYER),
        attacker_index,
        origin,
        defender_id,
    )
    .map_err(|error| CalculatorError::Layout(error.to_string()))?;

    let value_dealt = price(target, forecast.attack);
    let value_taken = forecast
        .counter
        .map(|counter| price(request.attacking_unit, counter));
    // The good outcome for the attacker is its best roll against the weakest
    // reply; the bad one is the other pair. The two ranges are correlated, so
    // taking the two lows together would describe an exchange that cannot
    // happen and would read in the attacker's favour half the time.
    let net = NetFunds {
        low: i64::try_from(value_dealt.low).unwrap_or(i64::MAX)
            - value_taken.map_or(0, |taken| i64::try_from(taken.high).unwrap_or(i64::MAX)),
        high: i64::try_from(value_dealt.high).unwrap_or(i64::MAX)
            - value_taken.map_or(0, |taken| i64::try_from(taken.low).unwrap_or(i64::MAX)),
    };

    Ok(Ok(Engagement {
        weapon: weapon.weapon,
        damage: forecast.attack,
        counter: forecast.counter,
        counter_first: forecast.counter_first,
        destroys: u64::from(forecast.attack.low) >= u64::from(target.hp),
        may_destroy: u64::from(forecast.attack.low) < u64::from(target.hp)
            && u64::from(forecast.attack.high) >= u64::from(target.hp),
        value_dealt,
        value_taken,
        counter_steps: forecast.counter_steps,
        target_value: value_of(target),
        net,
    }))
}

/// Where the attacker stands. The origin is always the board's corner; only the
/// distance to the target changes.
fn attacker_position(_request: &BattleRequest) -> Pos {
    Pos::new(0, 0)
}

/// How far apart the two units are placed.
///
/// The reducer refuses an engagement outside the attacker's range, so the
/// layout has to satisfy it: a direct fires at one tile and an indirect at its
/// own minimum. The minimum rather than any other point in the band because it
/// is the one distance every indirect can reach, and because distance itself
/// changes nothing about the damage.
fn engagement_distance(unit: UnitKind) -> u8 {
    ruleset::profile(unit)
        .indirect_range
        .map_or(1, |range| u8::try_from(range.minimum).unwrap_or(1))
}

/// Lower a request into an ordinary state the reducer will accept.
///
/// The board is the smallest one that can hold the engagement and both armies'
/// property counts: row 0 carries the two combatants on the terrain the request
/// named, row 1 the attacking army's holdings, and row 2 the defending army's.
/// The holdings are real tiles because that is how the commander operators
/// count them — there is no field on a player saying how many towers it has,
/// and inventing one would put a second answer beside the reducer's.
fn lay_out(
    request: &BattleRequest,
    target: Fighter,
) -> Result<(State, UnitId, UnitId), CalculatorError> {
    let distance = engagement_distance(request.attacking_unit.unit);
    let holdings = request.attacker.properties.max(request.defender.properties);
    let width =
        u8::try_from(u64::from(distance).saturating_add(1).max(holdings).max(1)).map_err(|_| {
            CalculatorError::Layout("the engagement needs a board wider than 255".into())
        })?;
    let height = 3_u8;

    let mut tiles = vec![Tile::new(Terrain::Plain); usize::from(width) * usize::from(height)];
    let at = |x: u8, y: u8| usize::from(y) * usize::from(width) + usize::from(x);

    // The ground the two units fight on. Neither tile is owned: the request says
    // how many properties each army holds, and a combatant standing on a city
    // that also counted itself would answer that question twice.
    tiles[at(0, 0)] = Tile::new(request.attacking_unit.terrain);
    tiles[at(distance, 0)] = Tile::new(target.terrain);

    // The roster below seats the attacker first and the defender second, and a
    // held tile names a seat.
    place_holdings(&mut tiles, &at, 1, &request.attacker, ATTACKER_SEAT);
    place_holdings(&mut tiles, &at, 2, &request.defender, DEFENDER_SEAT);

    let board = Board::new(width, height, tiles)
        .map_err(|error| CalculatorError::Layout(format!("{error:?}")))?;

    let attacker_id = UnitId::new(1);
    let defender_id = UnitId::new(2);
    let units = UnitStore::new(vec![
        combatant(
            attacker_id,
            request.attacking_unit,
            ATTACKER_SEAT,
            Pos::new(0, 0),
        ),
        combatant(defender_id, target, DEFENDER_SEAT, Pos::new(distance, 0)),
    ])
    .map_err(|error| CalculatorError::Layout(format!("{error:?}")))?;

    let state = State {
        ruleset: RulesetRef {
            id: ruleset::RULESET_ID.into(),
            revision: ruleset::RULESET_REVISION.into(),
        },
        settings: settings(request.weather),
        board,
        teams: vec![
            Team {
                id: ATTACKING_PLAYER.into(),
                status: TeamStatus::Active,
            },
            Team {
                id: DEFENDING_PLAYER.into(),
                status: TeamStatus::Active,
            },
        ],
        players: Roster::new(vec![
            player(ATTACKING_PLAYER, &request.attacker),
            player(DEFENDING_PLAYER, &request.defender),
        ])
        .expect("two players fit a roster"),
        turn: Turn {
            day: 1,
            active_player: ATTACKING_PLAYER.into(),
            phase: Phase::UnitAction,
            order: vec![ATTACKING_PLAYER.into(), DEFENDING_PLAYER.into()],
            position: 0,
        },
        weather: Weather {
            kind: request.weather,
            remaining_turns: 1,
        },
        units,
        next_unit_id: Some(3),
        match_state: Match::Active {
            draw_offers: Vec::new(),
        },
    };

    Ok((state, attacker_id, defender_id))
}

/// Fill one army's row with the properties it holds, towers first.
///
/// Towers lead because they are the ones that have to be distinguishable; the
/// rest are cities, which carry no combat effect beyond being counted.
fn place_holdings(
    tiles: &mut [Tile],
    at: &impl Fn(u8, u8) -> usize,
    row: u8,
    side: &SideContext,
    owner: PlayerIdx,
) {
    for index in 0..side.properties {
        let Ok(x) = u8::try_from(index) else { break };
        let terrain = if index < side.com_towers {
            Terrain::ComTower
        } else {
            Terrain::City
        };
        let mut tile = Tile::new(terrain);
        tile.owner = TileOwner::Owned(owner);
        tiles[at(x, row)] = tile;
    }
}

fn combatant(id: UnitId, fighter: Fighter, owner: PlayerIdx, position: Pos) -> Unit {
    let profile = ruleset::profile(fighter.unit);
    Unit {
        id,
        kind: fighter.unit,
        owner,
        hp: fighter.hp,
        fuel: profile.max_fuel,
        ammo: fighter.ammo.unwrap_or(profile.max_ammo),
        action: UnitAction::Ready,
        // A concealed submarine cannot be attacked by most things, and a
        // calculator asked about that pairing is being asked what the attack
        // would do, not whether the target is currently hiding.
        concealment: Concealment::Exposed,
        location: Location::Board { position },
    }
}

fn player(id: &str, side: &SideContext) -> Player {
    Player::new(id.into(), id.into())
        .with_funds(side.funds)
        .with_commanders(vec![Commander {
            id: side.commander.unwrap_or(CommanderKind::Neutral),
            active: true,
            power_charge: 0,
            power_uses: 0,
        }])
        .with_power_state(match side.power {
            Some(PowerLevel::Cop) => PowerState::Cop { commander_slot: 0 },
            Some(PowerLevel::Scop) => PowerState::Scop { commander_slot: 0 },
            None => PowerState::None,
        })
}

/// Fog is off, and that is the one setting here that matters.
///
/// The reducer refuses an engagement whose target the acting team cannot see,
/// and a calculator is asked about an engagement the player is imagining rather
/// than one they can currently observe. Every other field is inert: nothing in
/// the damage formula reads income, starting funds, bans, or limits.
fn settings(weather: WeatherKind) -> Settings {
    Settings {
        fog: false,
        income_per_property: 1000,
        starting_funds: 0,
        powers: crate::semantic::Toggle::Enabled,
        tags: false,
        weather: match weather {
            WeatherKind::Clear => crate::semantic::WeatherSetting::Clear,
            WeatherKind::Rain => crate::semantic::WeatherSetting::Rain,
            WeatherKind::Snow => crate::semantic::WeatherSetting::Snow,
        },
        lab_units: Vec::new(),
        unit_bans: Vec::new(),
        commander_bans: crate::semantic::CommanderBans {
            lead: Vec::new(),
            backup: Vec::new(),
        },
        capture_limit: None,
        day_limit: None,
        unit_limit: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fighter(unit: UnitKind) -> Fighter {
        Fighter {
            unit,
            hp: 100,
            ammo: None,
            terrain: Terrain::Plain,
        }
    }

    fn request(attacker: UnitKind, defenders: &[UnitKind]) -> BattleRequest {
        BattleRequest {
            weather: WeatherKind::Clear,
            attacker: SideContext::default(),
            attacking_unit: fighter(attacker),
            defender: SideContext::default(),
            defending_units: defenders.iter().copied().map(fighter).collect(),
        }
    }

    fn only(report: &BattleReport) -> Engagement {
        report.outcomes[0].engagement.clone().expect("scorable")
    }

    #[test]
    fn matches_the_bare_formula_the_combat_module_documents() {
        let report = forecast(&request(UnitKind::Infantry, &[UnitKind::Infantry])).unwrap();
        let engagement = only(&report);
        // 49 at no luck over the one defense star a plain carries, and the AWBW
        // spread of eight points above it once the star scales the roll down.
        assert_eq!(engagement.damage, DamageRange { low: 49, high: 57 });
        assert!(engagement.counter.is_some());
        assert!(!engagement.destroys);
    }

    #[test]
    fn an_indirect_draws_no_reply() {
        let report = forecast(&request(UnitKind::Artillery, &[UnitKind::Infantry])).unwrap();
        assert_eq!(only(&report).counter, None);
    }

    #[test]
    fn a_pairing_with_no_weapon_is_reported_rather_than_dropped() {
        let report = forecast(&request(UnitKind::AntiAir, &[UnitKind::Battleship])).unwrap();
        assert_eq!(report.outcomes[0].engagement, Err(Unscorable::NoWeapon));
    }

    #[test]
    fn a_transport_never_fights() {
        let report = forecast(&request(UnitKind::Apc, &[UnitKind::Infantry])).unwrap();
        assert_eq!(report.outcomes[0].engagement, Err(Unscorable::Unarmed));
    }

    #[test]
    fn terrain_under_the_target_reduces_what_lands() {
        let mut plain = request(UnitKind::Infantry, &[UnitKind::Infantry]);
        let mut mountain = plain.clone();
        mountain.defending_units[0].terrain = Terrain::Mountain;
        plain.defending_units[0].terrain = Terrain::Plain;

        let on_plain = only(&forecast(&plain).unwrap()).damage.low;
        let on_mountain = only(&forecast(&mountain).unwrap()).damage.low;
        assert!(on_mountain < on_plain, "{on_mountain} < {on_plain}");
    }

    #[test]
    fn com_towers_raise_what_the_holder_deals() {
        let bare = forecast(&request(UnitKind::Infantry, &[UnitKind::Infantry])).unwrap();
        let mut towered = request(UnitKind::Infantry, &[UnitKind::Infantry]);
        towered.attacker.properties = 2;
        towered.attacker.com_towers = 2;
        let towered = forecast(&towered).unwrap();

        assert!(only(&towered).damage.low > only(&bare).damage.low);
    }

    #[test]
    fn a_commander_who_reads_the_treasury_reads_the_one_in_the_request() {
        // Colin reads the treasury on his super power and nowhere else, so the
        // power is part of what makes funds a combat input at all.
        let mut poor = request(UnitKind::Infantry, &[UnitKind::Infantry]);
        poor.attacker.commander = Some(CommanderKind::Colin);
        poor.attacker.power = Some(PowerLevel::Scop);
        let mut rich = poor.clone();
        rich.attacker.funds = 30_000;

        assert!(
            only(&forecast(&rich).unwrap()).damage.low > only(&forecast(&poor).unwrap()).damage.low
        );
    }

    #[test]
    fn damage_is_priced_on_what_lands_rather_than_on_the_overkill() {
        let mut overkill = request(UnitKind::Bomber, &[UnitKind::Infantry]);
        overkill.defending_units[0].hp = 10;
        let engagement = only(&forecast(&overkill).unwrap());

        assert!(engagement.destroys);
        // A tenth of an Infantry is destroyed, and the target was worth a tenth
        // of one. Nothing is paid for the part of the shot that hit air.
        assert_eq!(engagement.value_dealt.low, engagement.target_value);
        assert_eq!(
            engagement.target_value,
            ruleset::profile(UnitKind::Infantry).cost / 10
        );
    }

    #[test]
    fn the_net_pairs_the_worst_strike_with_the_strongest_reply() {
        let report = forecast(&request(UnitKind::Tank, &[UnitKind::Tank])).unwrap();
        let engagement = only(&report);
        let taken = engagement.value_taken.expect("a tank answers a tank");

        assert_eq!(
            engagement.net.low,
            engagement.value_dealt.low as i64 - taken.high as i64
        );
        assert_eq!(
            engagement.net.high,
            engagement.value_dealt.high as i64 - taken.low as i64
        );
        assert!(engagement.net.low <= engagement.net.high);
    }

    #[test]
    fn empty_ammo_falls_back_to_the_unlimited_weapon() {
        let loaded = request(UnitKind::Tank, &[UnitKind::Tank]);
        let mut empty = loaded.clone();
        empty.attacking_unit.ammo = Some(0);

        let loaded = only(&forecast(&loaded).unwrap());
        let empty = only(&forecast(&empty).unwrap());

        assert_eq!(loaded.weapon, Weapon::Ammo);
        assert_eq!(empty.weapon, Weapon::Unlimited);
        assert!(empty.damage.low < loaded.damage.low);
        assert!(empty.damage.high < loaded.damage.high);
    }

    #[test]
    fn the_counter_is_broken_out_at_each_health_the_target_may_be_left_in() {
        let report = forecast(&request(UnitKind::Infantry, &[UnitKind::Infantry])).unwrap();
        let engagement = only(&report);
        let steps = &engagement.counter_steps;

        // Every rung stands in its own bar, and they climb.
        assert!(!steps.is_empty(), "a surviving infantry answers");
        for pair in steps.windows(2) {
            assert!(
                pair[0].target_hp < pair[1].target_hp,
                "the rungs climb: {steps:?}"
            );
        }
        for step in steps {
            assert!(
                step.target_hp.is_multiple_of(10) || step.target_hp == engagement_top(&engagement),
                "a rung sits at the top of its bar: {step:?}"
            );
        }

        // The rungs say the same thing the single range does, split up: the
        // lowest rung's floor and the highest rung's ceiling are the ends of
        // the counter the forecast reports.
        let counter = engagement.counter.expect("a surviving infantry answers");
        assert_eq!(steps.first().expect("a rung").counter.low, counter.low);
        assert_eq!(steps.last().expect("a rung").counter.high, counter.high);
    }

    #[test]
    fn the_counter_falls_to_nothing_when_the_best_roll_destroys() {
        let mut request = request(UnitKind::NeoTank, &[UnitKind::MegaTank]);
        request.defending_units[0].hp = 38;
        let engagement = only(&forecast(&request).unwrap());

        assert!(engagement.may_destroy);
        let counter = engagement
            .counter
            .expect("the weakest roll leaves a survivor");
        assert_eq!(counter.low, 0, "the roll that kills is answered by nothing");
        let first = engagement.counter_steps.first().expect("a survivor rung");
        assert!(
            first.counter.low > 0,
            "the rungs speak for the healths it lives at: {first:?}"
        );
    }

    /// The most of the target that can be left standing, in points.
    fn engagement_top(engagement: &Engagement) -> u8 {
        100u8.saturating_sub(engagement.damage.low.min(255) as u8)
    }

    #[test]
    fn a_target_that_never_survives_reports_no_rungs() {
        // One bar of infantry does not survive a whole Md Tank's weakest roll.
        let mut request = request(UnitKind::MdTank, &[UnitKind::Infantry]);
        request.defending_units[0].hp = 10;
        let report = forecast(&request).unwrap();
        let engagement = only(&report);
        assert!(engagement.destroys, "a Md Tank finishes a spent infantry");
        assert!(
            engagement.counter_steps.is_empty(),
            "nothing that is gone answers"
        );
    }

    #[test]
    fn an_indirect_draws_no_rungs_because_it_draws_no_reply() {
        let report = forecast(&request(UnitKind::Artillery, &[UnitKind::Infantry])).unwrap();
        let engagement = only(&report);
        assert!(engagement.counter.is_none(), "an indirect is not answered");
        assert!(engagement.counter_steps.is_empty());
    }

    #[test]
    fn health_outside_the_scale_is_refused() {
        let mut broken = request(UnitKind::Infantry, &[UnitKind::Infantry]);
        broken.attacking_unit.hp = 0;
        assert_eq!(forecast(&broken), Err(CalculatorError::Health(0)));
    }

    #[test]
    fn towers_cannot_outnumber_the_properties_holding_them() {
        let mut broken = request(UnitKind::Infantry, &[UnitKind::Infantry]);
        broken.attacker.com_towers = 3;
        assert!(matches!(
            forecast(&broken),
            Err(CalculatorError::ComTowers { .. })
        ));
    }

    #[test]
    fn property_counts_outside_the_board_limit_are_refused() {
        let mut broken = request(UnitKind::Infantry, &[UnitKind::Infantry]);
        broken.attacker.properties = MAX_PROPERTIES + 1;
        assert_eq!(
            forecast(&broken),
            Err(CalculatorError::Properties(MAX_PROPERTIES + 1))
        );
    }

    #[test]
    fn every_pairing_in_the_ruleset_is_answered_rather_than_failing() {
        for attacker in UnitKind::ALL {
            let report = forecast(&BattleRequest {
                weather: WeatherKind::Clear,
                attacker: SideContext::default(),
                attacking_unit: fighter(attacker),
                defender: SideContext::default(),
                defending_units: UnitKind::ALL.iter().copied().map(fighter).collect(),
            })
            .unwrap_or_else(|error| panic!("{attacker} could not be scored: {error}"));
            assert_eq!(report.outcomes.len(), UnitKind::ALL.len());
        }
    }

    /// Weather is sent with every request and moves no figure in this ruleset.
    ///
    /// It is the licence a caller needs to leave weather off its controls: no
    /// commander gates a firepower or defense rule on it, so the three settings
    /// score the same engagement. The sweep is over every commander at every
    /// power level rather than over a chosen few, because the claim is about
    /// the whole commander table and a later revision is free to break it.
    #[test]
    fn weather_never_moves_a_calculator_result() {
        let powers = [None, Some(PowerLevel::Cop), Some(PowerLevel::Scop)];

        for commander in CommanderKind::ALL {
            for power in powers {
                let side = SideContext {
                    commander: Some(commander),
                    power,
                    funds: 20_000,
                    properties: 10,
                    com_towers: 2,
                };
                let mut scored = None;

                for weather in WeatherKind::ALL {
                    let report = forecast(&BattleRequest {
                        weather,
                        attacker: side,
                        attacking_unit: fighter(UnitKind::Tank),
                        defender: side,
                        defending_units: UnitKind::ALL.iter().copied().map(fighter).collect(),
                    })
                    .expect("scorable");

                    match &scored {
                        None => scored = Some(report),
                        Some(clear) => assert_eq!(
                            *clear, report,
                            "{commander:?} at {power:?} scores differently under {weather:?}"
                        ),
                    }
                }
            }
        }
    }
}
