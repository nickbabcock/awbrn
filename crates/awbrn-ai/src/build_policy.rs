//! A build policy fitted to the build decisions of human players.
//!
//! One decision is one build at one production site. The choices are each
//! unit kind that the site can build and that the player can pay for, and
//! "no build". The model is a conditional logit: each unit kind has one
//! weight vector over the decision features, and all kinds share one weight
//! vector over the features of the kind itself. A softmax over the legal
//! choices gives the probabilities.
//!
//! The features use only the board that the player can see. The replay
//! extractor and the greedy policy call the same function,
//! [`BuildContext::site`], so the fit and the play read the same values.

use std::sync::LazyLock;

use awvm::commander;
use awvm::ruleset::{self, CommanderKind, TerrainTrait, UnitKind};
use awvm::semantic::{Location, PlayerIdx, Pos, State, TileOwner};

use crate::agents::strike;

/// The unit classes that the army features count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitClass {
    /// Infantry and mech.
    Foot,
    /// Recon and APC.
    Light,
    /// Tank.
    Tank,
    /// Medium tank, neotank and megatank.
    Heavy,
    /// Artillery, rockets and piperunner.
    Indirect,
    /// Anti-air and missiles.
    AntiAir,
    /// Battle and transport copters.
    Copter,
    /// Fighter, bomber, stealth and black bomb.
    Plane,
    /// All naval units.
    Naval,
}

impl UnitClass {
    pub const ALL: [Self; 9] = [
        Self::Foot,
        Self::Light,
        Self::Tank,
        Self::Heavy,
        Self::Indirect,
        Self::AntiAir,
        Self::Copter,
        Self::Plane,
        Self::Naval,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Foot => "foot",
            Self::Light => "light",
            Self::Tank => "tank",
            Self::Heavy => "heavy",
            Self::Indirect => "indirect",
            Self::AntiAir => "anti_air",
            Self::Copter => "copter",
            Self::Plane => "plane",
            Self::Naval => "naval",
        }
    }

    /// The class of a unit kind.
    pub const fn of(kind: UnitKind) -> Self {
        match kind {
            UnitKind::Infantry | UnitKind::Mech => Self::Foot,
            UnitKind::Recon | UnitKind::Apc => Self::Light,
            UnitKind::Tank => Self::Tank,
            UnitKind::MdTank | UnitKind::NeoTank | UnitKind::MegaTank => Self::Heavy,
            UnitKind::Artillery | UnitKind::Rocket | UnitKind::Piperunner => Self::Indirect,
            UnitKind::AntiAir | UnitKind::Missile => Self::AntiAir,
            UnitKind::BCopter | UnitKind::TCopter => Self::Copter,
            UnitKind::Fighter | UnitKind::Bomber | UnitKind::Stealth | UnitKind::BlackBomb => {
                Self::Plane
            }
            UnitKind::Battleship
            | UnitKind::BlackBoat
            | UnitKind::Carrier
            | UnitKind::Cruiser
            | UnitKind::Lander
            | UnitKind::Sub => Self::Naval,
        }
    }
}

/// The decision features that are not per class or per commander.
const BASE_FEATURES: [&str; 23] = [
    "day",
    "day_7_9",
    "day_10_12",
    "day_13_15",
    "day_16_20",
    "day_21_plus",
    "funds",
    "income",
    "funds_per_empty_site",
    "own_sites",
    "empty_sites",
    "own_capturers",
    "neutral_properties",
    "property_share",
    "army_share",
    "own_army",
    "enemy_army",
    "site_enemy_distance",
    "site_property_distance",
    "site_hq_distance",
    "site_threat",
    "site_own_near",
    "site_enemy_near",
];

/// The decision features that the second model adds. They come after the
/// commander features, so that the indices of the first features do not
/// change.
const EXTRA_FEATURES: [&str; 5] = [
    "built_turn",
    "built_turn_value",
    "map_area",
    "site_front_rank",
    "site_front_gap",
];

const CLASS_START: usize = BASE_FEATURES.len();
const CO_START: usize = CLASS_START + 4 * UnitClass::ALL.len();
const EXTRA_START: usize = CO_START + 2 * CommanderKind::COUNT;
/// The value share of each class in our army and in the enemy army.
const SHARE_START: usize = EXTRA_START + EXTRA_FEATURES.len();

/// The number of decision features.
pub const DECISION_FEATURES: usize = SHARE_START + 2 * UnitClass::ALL.len();

/// The features of one unit kind at one decision. All kinds share their
/// weights.
///
/// - `cost`: the cost of the kind, in thousands of funds.
/// - `overspend`: the part of the cost above the funds per empty site.
/// - `counter`: the mean exchange of one strike against the enemy army.
/// - `own_kind_count`: the number of our units of this kind.
/// - `own_class_count`, `own_class_share`: the number of our units of the
///   class of the kind, and the value share of the class in our army.
/// - `built_turn_kind`, `built_turn_class`: the number of units of this kind
///   and of its class that we built this turn.
/// - `remainder`: the funds after the build, in thousands.
/// - `fill_short`: the funds that we do not have after the build to put an
///   infantry on each other empty site, in thousands.
/// - `prey_share`, `predator_share`: the health share of the enemy army that
///   the kind hard-counters, and that hard-counters the kind.
/// - `prey_value`: the value of the enemy units that the kind hard-counters,
///   in thousands of funds.
/// - `counter_near`: as `counter`, against the enemy units near the site.
/// - `co_attack`, `co_defense`: the attack and the defense of the kind under
///   our commander, minus 100, divided by 100.
pub const KIND_FEATURES: [&str; 16] = [
    "cost",
    "overspend",
    "counter",
    "own_kind_count",
    "own_class_count",
    "own_class_share",
    "built_turn_kind",
    "built_turn_class",
    "remainder",
    "fill_short",
    "prey_share",
    "predator_share",
    "prey_value",
    "counter_near",
    "co_attack",
    "co_defense",
];

/// A strike of at least this share of the health of the target is heavy.
const HEAVY_STRIKE: f64 = 0.55;
/// A strike of at most this share of the health of the target is light.
const LIGHT_STRIKE: f64 = 0.25;

/// The names of the decision features, in vector order.
pub fn decision_feature_names() -> &'static [String] {
    static NAMES: LazyLock<Vec<String>> = LazyLock::new(|| {
        let mut names: Vec<String> = BASE_FEATURES
            .iter()
            .map(|name| (*name).to_owned())
            .collect();
        for class in UnitClass::ALL {
            names.push(format!("own_value_{}", class.name()));
            names.push(format!("own_count_{}", class.name()));
            names.push(format!("enemy_value_{}", class.name()));
            names.push(format!("enemy_count_{}", class.name()));
        }
        for side in ["own", "enemy"] {
            for co in CommanderKind::ALL {
                names.push(format!("{side}_co_{}", co.as_str()));
            }
        }
        names.extend(EXTRA_FEATURES.iter().map(|name| (*name).to_owned()));
        for side in ["own", "enemy"] {
            for class in UnitClass::ALL {
                names.push(format!("{side}_share_{}", class.name()));
            }
        }
        assert_eq!(names.len(), DECISION_FEATURES);
        names
    });
    &NAMES
}

/// The distance in tiles that a missing target gets.
const FAR: f64 = 30.0;
/// The radius in tiles of the near army features of a site.
const NEAR: u64 = 6;

/// The features of one build decision.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildFeatures {
    /// The features of the decision, in the order of
    /// [`decision_feature_names`].
    pub decision: [f64; DECISION_FEATURES],
    /// The features of each unit kind, in the order of [`KIND_FEATURES`],
    /// indexed by [`UnitKind::index`].
    pub kind: [[f64; KIND_FEATURES.len()]; UnitKind::COUNT],
}

/// The features of a position that all sites of one seat share.
#[derive(Clone, Debug)]
pub struct BuildContext<'a> {
    state: &'a State,
    seat: PlayerIdx,
    shared: [f64; DECISION_FEATURES],
    /// The cost of each kind for this seat, in funds.
    cost: [f64; UnitKind::COUNT],
    /// The expected exchange of one strike of each kind against the enemy
    /// army, in funds.
    counter: [f64; UnitKind::COUNT],
    own_kind_count: [f64; UnitKind::COUNT],
    /// The number of our units of the class of each kind, and the value
    /// share of the class in our army.
    own_class: [(f64, f64); UnitClass::ALL.len()],
    /// The units of each kind that we built this turn.
    built_kind: [f64; UnitKind::COUNT],
    built_class: [f64; UnitClass::ALL.len()],
    /// The health share and the value of the enemy units that each kind
    /// hard-counters, and the health share of the enemy units that
    /// hard-counter each kind.
    prey: [(f64, f64); UnitKind::COUNT],
    predator: [f64; UnitKind::COUNT],
    /// The attack and the defense of each kind under our commander.
    co_combat: [(f64, f64); UnitKind::COUNT],
    funds: f64,
    empty_sites: u32,
    /// Our empty production sites and the distance from each to the nearest
    /// enemy unit.
    empty_site_distance: Vec<(Pos, f64)>,
    /// The enemy units on the board by kind, with their health in tenths.
    enemy_kinds: Vec<(Pos, UnitKind, u8)>,
    funds_per_empty_site: f64,
    /// The enemy units on the board: position, value and reach. A value is
    /// in funds times health points, so that sums do not depend on the
    /// order of the units.
    enemies: Vec<(Pos, u64, u64)>,
    /// Our units on the board: position and value, as for `enemies`.
    own: Vec<(Pos, u64)>,
    /// The properties that we can still take.
    targets: Vec<Pos>,
    /// The enemy headquarters.
    headquarters: Vec<Pos>,
}

/// The value of a unit in funds times health points. An integer, so that a
/// sum is exact and does not depend on the order of the units. Divide by
/// 100 for funds.
fn value(kind: UnitKind, hp: u8) -> u64 {
    ruleset::profile(kind).cost * u64::from(hp)
}

/// Funds times health points, in thousands of funds.
fn thousands(value: u64) -> f64 {
    value as f64 / 100_000.0
}

/// Whether `terrain` produces units of some domain.
fn produces(terrain: ruleset::Terrain) -> bool {
    ruleset::terrain_has(terrain, TerrainTrait::ProducesGround)
        || ruleset::terrain_has(terrain, TerrainTrait::ProducesAir)
        || ruleset::terrain_has(terrain, TerrainTrait::ProducesSea)
}

impl<'a> BuildContext<'a> {
    /// Read the features that do not depend on the site.
    pub fn new(state: &'a State, seat: PlayerIdx) -> Self {
        let hostile = |other: PlayerIdx| crate::threat::hostile(state, seat, other);
        let mut shared = [0.0; DECISION_FEATURES];
        let day = state.turn.day as f64;
        shared[0] = day / 10.0;
        let band = match state.turn.day {
            0..=6 => None,
            7..=9 => Some(1),
            10..=12 => Some(2),
            13..=15 => Some(3),
            16..=20 => Some(4),
            _ => Some(5),
        };
        if let Some(band) = band {
            shared[band] = 1.0;
        }
        let funds = state.player(seat).funds as f64;
        shared[6] = funds / 1000.0;

        let mut income_properties = 0_u32;
        let mut own_properties = 0_u32;
        let mut enemy_properties = 0_u32;
        let mut neutral = 0_u32;
        let mut own_sites = 0_u32;
        let mut empty_sites = 0_u32;
        let mut targets = Vec::new();
        let mut headquarters = Vec::new();
        let mut empty_positions = Vec::new();
        for (position, tile) in state.board.iter() {
            let capturable = ruleset::terrain_has(tile.terrain, TerrainTrait::Capturable);
            match tile.owner {
                TileOwner::NotOwnable => {}
                TileOwner::Neutral => {
                    if capturable {
                        neutral += 1;
                        targets.push(position);
                    }
                }
                TileOwner::Owned(holder) if holder == seat => {
                    own_properties += 1;
                    if ruleset::terrain_has(tile.terrain, TerrainTrait::Income) {
                        income_properties += 1;
                    }
                    if produces(tile.terrain) && commander_site(state, seat, tile.terrain) {
                        own_sites += 1;
                        let occupied = state
                            .units
                            .iter()
                            .any(|unit| unit.location == (Location::Board { position }));
                        if !occupied {
                            empty_sites += 1;
                            empty_positions.push(position);
                        }
                    }
                }
                TileOwner::Owned(holder) => {
                    if hostile(holder) {
                        enemy_properties += 1;
                        if capturable {
                            targets.push(position);
                        }
                        if ruleset::terrain_has(tile.terrain, TerrainTrait::CaptureDefeatsOwner) {
                            headquarters.push(position);
                        }
                    }
                }
            }
        }
        let income = f64::from(income_properties)
            * commander::effective_income_per_property(state, seat) as f64;
        shared[7] = income / 1000.0;
        let funds_per_empty_site = funds / f64::from(empty_sites.max(1));
        shared[8] = funds_per_empty_site / 1000.0;
        shared[9] = f64::from(own_sites);
        shared[10] = f64::from(empty_sites);
        shared[12] = f64::from(neutral);
        let properties = own_properties + enemy_properties;
        shared[13] = if properties == 0 {
            0.5
        } else {
            f64::from(own_properties) / f64::from(properties)
        };

        let mut own_army = 0_u64;
        let mut enemy_army = 0_u64;
        let mut capturers = 0_u32;
        let mut own_kind_count = [0.0; UnitKind::COUNT];
        let mut enemy_hp = [0_u64; UnitKind::COUNT];
        let mut class_value = [0_u64; 4 * UnitClass::ALL.len()];
        let mut enemies = Vec::new();
        let mut enemy_kinds = Vec::new();
        let mut own = Vec::new();
        let mut built_kind = [0.0; UnitKind::COUNT];
        let mut built_class = [0.0; UnitClass::ALL.len()];
        let mut built_value = 0_u64;
        for unit in state.units.iter() {
            let class = UnitClass::of(unit.kind) as usize;
            let worth = value(unit.kind, unit.hp);
            let offset = if unit.owner == seat {
                own_army += worth;
                own_kind_count[unit.kind.index()] += 1.0;
                if built_this_turn(state, seat, unit) {
                    built_kind[unit.kind.index()] += 1.0;
                    built_class[class] += 1.0;
                    built_value += worth;
                }
                if ruleset::profile(unit.kind).can_capture {
                    capturers += 1;
                }
                if let Location::Board { position } = unit.location {
                    own.push((position, worth));
                }
                0
            } else if hostile(unit.owner) {
                enemy_army += worth;
                enemy_hp[unit.kind.index()] += u64::from(unit.hp);
                if let Location::Board { position } = unit.location {
                    enemies.push((position, worth, reach(unit.kind)));
                    enemy_kinds.push((position, unit.kind, unit.hp));
                }
                2
            } else {
                continue;
            };
            let index = 4 * class + offset;
            class_value[index] += worth;
            class_value[index + 1] += 1;
        }
        for (index, value) in class_value.iter().enumerate() {
            shared[CLASS_START + index] = if index % 2 == 0 {
                thousands(*value)
            } else {
                *value as f64
            };
        }
        shared[11] = f64::from(capturers);
        let army = own_army + enemy_army;
        shared[14] = if army == 0 {
            0.5
        } else {
            own_army as f64 / army as f64
        };
        shared[15] = thousands(own_army);
        shared[16] = thousands(enemy_army);
        for (index, class) in UnitClass::ALL.iter().enumerate() {
            let class = *class as usize;
            let share = |army: u64, value: u64| {
                if army == 0 {
                    0.0
                } else {
                    value as f64 / army as f64
                }
            };
            shared[SHARE_START + index] = share(own_army, class_value[4 * class]);
            shared[SHARE_START + UnitClass::ALL.len() + index] =
                share(enemy_army, class_value[4 * class + 2]);
        }
        shared[EXTRA_START] = built_class.iter().sum();
        shared[EXTRA_START + 1] = thousands(built_value);
        let dimensions = state.board.dimensions();
        shared[EXTRA_START + 2] =
            f64::from(dimensions.width()) * f64::from(dimensions.height()) / 1000.0;
        let empty_site_distance = empty_positions
            .into_iter()
            .map(|site| {
                let distance = enemies
                    .iter()
                    .map(|(position, _, _)| position.distance(site) as f64)
                    .fold(FAR, f64::min);
                (site, distance)
            })
            .collect();
        let own_class = std::array::from_fn(|class| {
            let share = if own_army == 0 {
                0.0
            } else {
                class_value[4 * class] as f64 / own_army as f64
            };
            (class_value[4 * class + 1] as f64, share)
        });

        for (side, players) in [
            (0, state.players.seats().find(|(other, _)| *other == seat)),
            (1, state.players.seats().find(|(other, _)| hostile(*other))),
        ] {
            let Some(co) = players.and_then(|(_, player)| {
                player
                    .commanders
                    .iter()
                    .find(|co| co.active)
                    .map(|co| co.id)
            }) else {
                continue;
            };
            let index = CommanderKind::ALL
                .iter()
                .position(|kind| *kind == co)
                .unwrap_or(0);
            shared[CO_START + side * CommanderKind::COUNT + index] = 1.0;
        }

        let mut cost = [0.0; UnitKind::COUNT];
        let mut counter = [0.0; UnitKind::COUNT];
        let mut prey = [(0.0, 0.0); UnitKind::COUNT];
        let mut predator = [0.0; UnitKind::COUNT];
        let enemy_health = enemy_hp.map(|hp| hp as f64 / 100.0);
        let whole = enemy_hp.iter().sum::<u64>() as f64 / 100.0;
        for kind in UnitKind::ALL {
            let base = ruleset::profile(kind).cost;
            cost[kind.index()] =
                commander::effective_build_cost(state, Some(seat), base).unwrap_or(base) as f64;
            if whole > 0.0 {
                let ours = base as f64;
                let mut total = 0.0;
                for theirs in UnitKind::ALL {
                    let health = enemy_health[theirs.index()];
                    if health == 0.0 {
                        continue;
                    }
                    let (out, back) = (strike(kind, theirs), strike(theirs, kind));
                    let dealt = out * ruleset::profile(theirs).cost as f64;
                    let taken = back * ours;
                    total += health * (dealt - taken);
                    if out >= HEAVY_STRIKE && back <= LIGHT_STRIKE {
                        prey[kind.index()].0 += health;
                        prey[kind.index()].1 += health * ruleset::profile(theirs).cost as f64;
                    }
                    if back >= HEAVY_STRIKE && out <= LIGHT_STRIKE {
                        predator[kind.index()] += health;
                    }
                }
                counter[kind.index()] = total / whole;
                prey[kind.index()].0 /= whole;
                prey[kind.index()].1 /= 1000.0;
                predator[kind.index()] /= whole;
            }
        }

        let own_funds = state.player(seat).funds;
        let co_combat =
            std::array::from_fn(|index| co_combat(state, seat, own_funds, UnitKind::ALL[index]));
        Self {
            state,
            seat,
            shared,
            cost,
            counter,
            own_kind_count,
            own_class,
            built_kind,
            built_class,
            prey,
            predator,
            co_combat,
            funds,
            empty_sites,
            empty_site_distance,
            enemy_kinds,
            funds_per_empty_site,
            enemies,
            own,
            targets,
            headquarters,
        }
    }

    /// The seat that decides.
    pub const fn seat(&self) -> PlayerIdx {
        self.seat
    }

    /// The state that the features read.
    pub const fn state(&self) -> &State {
        self.state
    }

    /// The cost of one kind for this seat, in funds.
    pub fn cost(&self, kind: UnitKind) -> f64 {
        self.cost[kind.index()]
    }

    /// The features of a build at `site`.
    pub fn site(&self, site: Pos) -> BuildFeatures {
        let mut decision = self.shared;
        let nearest = |positions: &mut dyn Iterator<Item = Pos>| {
            positions
                .map(|position| position.distance(site) as f64)
                .fold(FAR, f64::min)
        };
        decision[17] = nearest(&mut self.enemies.iter().map(|(position, _, _)| *position)) / 10.0;
        decision[18] = nearest(&mut self.targets.iter().copied()) / 10.0;
        decision[19] = nearest(&mut self.headquarters.iter().copied()) / 10.0;
        let mut threat = 0_u64;
        let mut enemy_near = 0_u64;
        for (position, worth, reach) in &self.enemies {
            let distance = position.distance(site);
            if distance <= *reach {
                threat += worth;
            }
            if distance <= NEAR {
                enemy_near += worth;
            }
        }
        let own_near: u64 = self
            .own
            .iter()
            .filter(|(position, _)| position.distance(site) <= NEAR)
            .map(|(_, worth)| worth)
            .sum();
        decision[20] = thousands(threat);
        decision[21] = thousands(own_near);
        decision[22] = thousands(enemy_near);
        // The rank of the site among our empty sites by the distance to the
        // nearest enemy unit, and the gap to the nearest of them.
        let distance = decision[17] * 10.0;
        let mut rank = 0_u32;
        let mut nearest_site = distance;
        for (other, other_distance) in &self.empty_site_distance {
            if *other != site && *other_distance < distance {
                rank += 1;
            }
            nearest_site = nearest_site.min(*other_distance);
        }
        decision[EXTRA_START + 3] = f64::from(rank);
        decision[EXTRA_START + 4] = (distance - nearest_site) / 10.0;

        // The enemy health of each kind near the site, in units of health.
        let mut near_health = [0.0; UnitKind::COUNT];
        let mut near_whole = 0.0;
        for (position, unit, hp) in &self.enemy_kinds {
            if position.distance(site) <= NEAR {
                let health = f64::from(*hp) / 100.0;
                near_health[unit.index()] += health;
                near_whole += health;
            }
        }
        // The other empty sites, after this one.
        let others = f64::from(self.empty_sites.saturating_sub(1));
        let mut kind = [[0.0; KIND_FEATURES.len()]; UnitKind::COUNT];
        for unit in UnitKind::ALL {
            let index = unit.index();
            let cost = self.cost[index];
            let class = UnitClass::of(unit) as usize;
            let counter_near = if near_whole > 0.0 {
                exchange(unit, &near_health) / near_whole
            } else {
                0.0
            };
            let remainder = self.funds - cost;
            kind[index] = [
                cost / 1000.0,
                (cost - self.funds_per_empty_site).max(0.0) / 1000.0,
                self.counter[index] / 1000.0,
                self.own_kind_count[index],
                self.own_class[class].0,
                self.own_class[class].1,
                self.built_kind[index],
                self.built_class[class],
                remainder / 1000.0,
                (1000.0 * others - remainder).max(0.0) / 1000.0,
                self.prey[index].0,
                self.predator[index],
                self.prey[index].1,
                counter_near / 1000.0,
                self.co_combat[index].0,
                self.co_combat[index].1,
            ];
        }
        BuildFeatures { decision, kind }
    }
}

/// The summed exchange of one strike of `kind` against the enemy health in
/// `health`, by kind, in funds.
fn exchange(kind: UnitKind, health: &[f64; UnitKind::COUNT]) -> f64 {
    let ours = ruleset::profile(kind).cost as f64;
    let mut total = 0.0;
    for theirs in UnitKind::ALL {
        let amount = health[theirs.index()];
        if amount == 0.0 {
            continue;
        }
        let dealt = strike(kind, theirs) * ruleset::profile(theirs).cost as f64;
        let taken = strike(theirs, kind) * ours;
        total += amount * (dealt - taken);
    }
    total
}

/// Whether `unit` of `seat` is a unit that we built this turn.
///
/// The state does not record the builds of a turn. A new unit is spent, has
/// full health, fuel and ammunition, and stands on our production site. A
/// unit that waits on our site without a move also agrees, but that is rare
/// at the time of a build. The replay extractor and the play path read the
/// same state, so they agree.
fn built_this_turn(state: &State, seat: PlayerIdx, unit: &awvm::semantic::Unit) -> bool {
    let Location::Board { position } = unit.location else {
        return false;
    };
    let profile = ruleset::profile(unit.kind);
    let tile = state.board.tile(position);
    unit.action == awvm::semantic::UnitAction::Spent
        && unit.hp == 100
        && unit.fuel == profile.max_fuel
        && unit.ammo == profile.max_ammo
        && tile.owner.is_owned_by(seat)
        && produces(tile.terrain)
}

/// The attack and the defense of `kind` under the commander of `seat`, minus
/// 100 and divided by 100. The unit stands on a plain and the strike is the
/// first strike.
fn co_combat(state: &State, seat: PlayerIdx, funds: u64, kind: UnitKind) -> (f64, f64) {
    static NO_CAPABILITIES: LazyLock<std::collections::HashSet<String>> =
        LazyLock::new(std::collections::HashSet::new);
    let unit = plain_combatant(kind, state.weather.kind, &NO_CAPABILITIES);
    let context = commander::CombatContext {
        tower_count: 0,
        funds,
        owned_properties: 0,
        base_terrain_stars: 1,
    };
    commander::effective_combat(state, Some(seat), unit, commander::Strike::Initial, context)
        .map_or((0.0, 0.0), |combat| {
            (
                (combat.attack as f64 - 100.0) / 100.0,
                (combat.defense as f64 - 100.0) / 100.0,
            )
        })
}

/// The combat side of a unit of `kind` that stands on a plain that is not a
/// property.
fn plain_combatant(
    kind: UnitKind,
    weather: awvm::ruleset::WeatherKind,
    capabilities: &std::collections::HashSet<String>,
) -> commander::Combatant<'_> {
    let profile = ruleset::profile(kind);
    let domain = match kind {
        UnitKind::Infantry | UnitKind::Mech => commander::CombatDomain::Foot,
        _ if profile.transport.is_some() => commander::CombatDomain::Transport,
        _ => match profile.domain {
            ruleset::Domain::Ground => commander::CombatDomain::GroundVehicle,
            ruleset::Domain::Air => commander::CombatDomain::Air,
            ruleset::Domain::Sea => commander::CombatDomain::Naval,
        },
    };
    commander::Combatant {
        kind,
        domain,
        fire_mode: profile.fire_mode,
        terrain: ruleset::Terrain::Plain,
        weather,
        property: false,
        capabilities,
    }
}

/// The day-to-day values of a commander for one unit kind, from the engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DayToDay {
    /// The attack, in percent. 100 is the value with no rule.
    pub attack: i64,
    /// The defense, in percent. 100 is the value with no rule.
    pub defense: i64,
    /// The range that the commander adds to the maximum attack range of an
    /// indirect unit.
    pub range_add: i64,
    /// The movement that the commander adds.
    pub move_add: i64,
}

impl DayToDay {
    /// The values with no commander rule.
    pub const NEUTRAL: Self = Self {
        attack: 100,
        defense: 100,
        range_add: 0,
        move_add: 0,
    };
}

/// The day-to-day values of `co` for a unit of `kind`.
///
/// The engine gives each value ([`commander::day_to_day_combat`],
/// [`commander::day_to_day_attack_range_add`] and
/// [`commander::day_to_day_move_add`]). The unit stands on a plain that is
/// not a property, in clear weather, with no tower and no funds. No power is
/// active. The function does not read the identity of the commander.
pub(crate) fn day_to_day(co: CommanderKind, kind: UnitKind) -> DayToDay {
    static TABLE: LazyLock<Vec<[DayToDay; UnitKind::COUNT]>> = LazyLock::new(|| {
        static NO_CAPABILITIES: LazyLock<std::collections::HashSet<String>> =
            LazyLock::new(std::collections::HashSet::new);
        CommanderKind::ALL
            .into_iter()
            .map(|co| {
                UnitKind::ALL.map(|kind| {
                    let profile = ruleset::profile(kind);
                    let unit =
                        plain_combatant(kind, awvm::ruleset::WeatherKind::Clear, &NO_CAPABILITIES);
                    let context = commander::CombatContext {
                        tower_count: 0,
                        funds: 0,
                        owned_properties: 0,
                        base_terrain_stars: 1,
                    };
                    let (attack, defense) =
                        commander::day_to_day_combat(co, unit, commander::Strike::Initial, context)
                            .map_or((100, 100), |combat| (combat.attack, combat.defense));
                    let range_add = if profile.indirect_range.is_some() {
                        commander::day_to_day_attack_range_add(
                            co,
                            kind,
                            profile.domain,
                            ruleset::FireMode::Indirect,
                        )
                    } else {
                        0
                    };
                    DayToDay {
                        attack,
                        defense,
                        range_add,
                        move_add: commander::day_to_day_move_add(co, kind, profile.domain),
                    }
                })
            })
            .collect()
    });
    TABLE[co.index()][kind.index()]
}

/// The day-to-day values of the active commander of `seat` for a unit of
/// `kind`. A seat with no active commander gets [`DayToDay::NEUTRAL`].
pub(crate) fn seat_day_to_day(state: &State, seat: PlayerIdx, kind: UnitKind) -> DayToDay {
    state
        .players
        .get(seat.get())
        .and_then(|player| player.commanders.iter().find(|co| co.active))
        .map_or(DayToDay::NEUTRAL, |co| day_to_day(co.id, kind))
}

/// The day-to-day effect of `co` on a unit of `kind`, as one number.
///
/// The value is the sum of the natural logarithms of these ratios, each
/// against a commander with no day-to-day rule ([`day_to_day`]):
///
/// - the attack, when the kind has a weapon,
/// - the defense,
/// - the maximum attack range, when the kind fires indirectly,
/// - the movement, when the kind can move.
///
/// Thus a commander with no day-to-day rule gets exactly zero for each kind.
fn kind_effect(co: CommanderKind, kind: UnitKind) -> f64 {
    let profile = ruleset::profile(kind);
    let ratio = |changed: i64, base: i64| (changed.max(1) as f64 / base as f64).ln();
    let values = day_to_day(co, kind);
    let mut effect = ratio(values.defense, 100);
    if profile.ammo_weapon.is_some() || profile.unlimited_weapon.is_some() {
        effect += ratio(values.attack, 100);
    }
    if let Some(range) = profile.indirect_range {
        let base = range.maximum as i64;
        effect += ratio(base + values.range_add, base);
    }
    if profile.movement > 0 {
        let base = profile.movement as i64;
        effect += ratio(base + values.move_add, base);
    }
    effect
}

/// The day-to-day effect of `co` on each unit kind ([`kind_effect`]),
/// indexed by [`UnitKind::index`].
pub fn commander_effects(co: CommanderKind) -> &'static [f64; UnitKind::COUNT] {
    static TABLE: LazyLock<Vec<[f64; UnitKind::COUNT]>> = LazyLock::new(|| {
        CommanderKind::ALL
            .into_iter()
            .map(|co| UnitKind::ALL.map(|kind| kind_effect(co, kind)))
            .collect()
    });
    &TABLE[co.index()]
}

/// The effect term of the build choice of `seat`: `strength` times the
/// day-to-day effect of its active commander on each kind
/// ([`commander_effects`]), indexed by [`UnitKind::index`].
///
/// The function gives `None` when the strength is zero, when the seat has
/// no active commander, or when the commander has no day-to-day effect on
/// any kind. Then the build choice does not change.
pub fn effect_bias(
    state: &State,
    seat: PlayerIdx,
    strength: f64,
) -> Option<[f64; UnitKind::COUNT]> {
    if strength == 0.0 {
        return None;
    }
    let co = state
        .player(seat)
        .commanders
        .iter()
        .find(|co| co.active)?
        .id;
    let effects = commander_effects(co);
    if effects.iter().all(|effect| *effect == 0.0) {
        return None;
    }
    Some(effects.map(|effect| strength * effect))
}

/// Whether the commander of `seat` can build at a site of this terrain.
fn commander_site(state: &State, seat: PlayerIdx, terrain: ruleset::Terrain) -> bool {
    ruleset::Domain::ALL
        .iter()
        .any(|domain| commander::production_site(state, seat, terrain, *domain))
}

/// The distance at which a unit can strike on its next turn, in tiles.
///
/// A direct unit moves and strikes an adjacent tile. An indirect unit strikes
/// from where it stands. The value does not read terrain or commander
/// effects.
fn reach(kind: UnitKind) -> u64 {
    let profile = ruleset::profile(kind);
    if profile.ammo_weapon.is_none() && profile.unlimited_weapon.is_none() {
        return 0;
    }
    match profile.indirect_range {
        Some(range) => range.maximum,
        None => profile.movement + 1,
    }
}

/// One choice of the model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildChoice {
    /// Build this kind.
    Build(UnitKind),
    /// Leave the site empty this turn.
    NoBuild,
}

/// The weights of one choice.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceWeights {
    pub intercept: f64,
    /// One weight for each name in [`BuildModelFile::decision_features`].
    pub weights: Vec<f64>,
}

/// The JSON file of a build model.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuildModelFile {
    pub schema_version: u32,
    pub name: String,
    /// The decision features that the model reads, by name.
    pub decision_features: Vec<String>,
    /// The mean and the scale of each decision feature. The model reads
    /// `(value - mean) / scale`.
    pub decision_mean: Vec<f64>,
    pub decision_scale: Vec<f64>,
    /// The kind features that the model reads, by name.
    pub kind_features: Vec<String>,
    pub kind_mean: Vec<f64>,
    pub kind_scale: Vec<f64>,
    /// The shared weight of each kind feature.
    pub kind_weights: Vec<f64>,
    /// The weights of each unit kind, by the name of the kind. A kind that
    /// is not in the map is never chosen.
    pub kinds: std::collections::BTreeMap<String, ChoiceWeights>,
    /// The weights of "no build".
    pub no_build: ChoiceWeights,
    /// Data that the fit records. The model does not read it.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub fit: serde_json::Value,
}

/// A build model, ready to evaluate.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildModel {
    /// The index of each read decision feature, its mean and its scale.
    decision: Vec<(usize, f64, f64)>,
    /// The index of each read kind feature, its mean, scale and weight.
    kind_features: Vec<(usize, f64, f64, f64)>,
    /// The weights of each unit kind, indexed by [`UnitKind::index`].
    kinds: Vec<Option<ChoiceWeights>>,
    no_build: ChoiceWeights,
}

/// An error in a build model file.
#[derive(Debug, thiserror::Error)]
pub enum BuildModelError {
    #[error("the model file is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("the model file has schema version {0}, not 1")]
    Schema(u32),
    #[error("unknown feature {0}")]
    Feature(String),
    #[error("unknown unit kind {0}")]
    Kind(String),
    #[error("the lengths of {0} do not agree")]
    Length(&'static str),
    #[error("the value {0} is not finite or the scale is not positive")]
    Value(&'static str),
}

impl BuildModel {
    /// Read a model from its JSON text.
    pub fn from_json(text: &str) -> Result<Self, BuildModelError> {
        let file: BuildModelFile = serde_json::from_str(text)?;
        Self::from_file(&file)
    }

    pub fn from_file(file: &BuildModelFile) -> Result<Self, BuildModelError> {
        if file.schema_version != 1 {
            return Err(BuildModelError::Schema(file.schema_version));
        }
        let count = file.decision_features.len();
        if file.decision_mean.len() != count || file.decision_scale.len() != count {
            return Err(BuildModelError::Length("decision features"));
        }
        let names = decision_feature_names();
        let mut decision = Vec::with_capacity(count);
        for ((name, mean), scale) in file
            .decision_features
            .iter()
            .zip(&file.decision_mean)
            .zip(&file.decision_scale)
        {
            let index = names
                .iter()
                .position(|known| known == name)
                .ok_or_else(|| BuildModelError::Feature(name.clone()))?;
            check(*mean, *scale, "decision feature")?;
            decision.push((index, *mean, *scale));
        }
        let kind_count = file.kind_features.len();
        if file.kind_mean.len() != kind_count
            || file.kind_scale.len() != kind_count
            || file.kind_weights.len() != kind_count
        {
            return Err(BuildModelError::Length("kind features"));
        }
        let mut kind_features = Vec::with_capacity(kind_count);
        for (index, name) in file.kind_features.iter().enumerate() {
            let feature = KIND_FEATURES
                .iter()
                .position(|known| known == name)
                .ok_or_else(|| BuildModelError::Feature(name.clone()))?;
            let (mean, scale, weight) = (
                file.kind_mean[index],
                file.kind_scale[index],
                file.kind_weights[index],
            );
            check(mean, scale, "kind feature")?;
            if !weight.is_finite() {
                return Err(BuildModelError::Value("kind weight"));
            }
            kind_features.push((feature, mean, scale, weight));
        }
        let weights = |choice: &ChoiceWeights| {
            if choice.weights.len() != count {
                return Err(BuildModelError::Length("choice weights"));
            }
            if !choice.intercept.is_finite() || choice.weights.iter().any(|w| !w.is_finite()) {
                return Err(BuildModelError::Value("choice weight"));
            }
            Ok(choice.clone())
        };
        let mut kinds = vec![None; UnitKind::COUNT];
        for (name, choice) in &file.kinds {
            let kind = UnitKind::ALL
                .into_iter()
                .find(|kind| kind.as_str() == name)
                .ok_or_else(|| BuildModelError::Kind(name.clone()))?;
            kinds[kind.index()] = Some(weights(choice)?);
        }
        Ok(Self {
            decision,
            kind_features,
            kinds,
            no_build: weights(&file.no_build)?,
        })
    }

    fn standardized(&self, features: &BuildFeatures) -> Vec<f64> {
        self.decision
            .iter()
            .map(|(index, mean, scale)| (features.decision[*index] - mean) / scale)
            .collect()
    }

    fn linear(choice: &ChoiceWeights, x: &[f64]) -> f64 {
        choice.intercept
            + choice
                .weights
                .iter()
                .zip(x)
                .map(|(weight, value)| weight * value)
                .sum::<f64>()
    }

    /// The utility of each choice in `legal`, in the same order. A kind that
    /// the model does not hold gets `None`.
    pub fn utilities(&self, features: &BuildFeatures, legal: &[BuildChoice]) -> Vec<Option<f64>> {
        let x = self.standardized(features);
        legal
            .iter()
            .map(|choice| match choice {
                BuildChoice::NoBuild => Some(Self::linear(&self.no_build, &x)),
                BuildChoice::Build(kind) => {
                    let weights = self.kinds[kind.index()].as_ref()?;
                    let shared: f64 = self
                        .kind_features
                        .iter()
                        .map(|(feature, mean, scale, weight)| {
                            weight * (features.kind[kind.index()][*feature] - mean) / scale
                        })
                        .sum();
                    Some(Self::linear(weights, &x) + shared)
                }
            })
            .collect()
    }

    /// The probability of each choice in `legal`. A choice that the model
    /// does not hold gets zero.
    pub fn probabilities(&self, features: &BuildFeatures, legal: &[BuildChoice]) -> Vec<f64> {
        let utilities = self.utilities(features, legal);
        let top = utilities
            .iter()
            .flatten()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        if !top.is_finite() {
            return vec![0.0; legal.len()];
        }
        let weights: Vec<f64> = utilities
            .iter()
            .map(|utility| utility.map_or(0.0, |value| (value - top).exp()))
            .collect();
        let total: f64 = weights.iter().sum();
        weights.into_iter().map(|weight| weight / total).collect()
    }

    /// The choice in `legal` with the highest utility. The first of equal
    /// choices wins. `None` when the model holds no choice in `legal`.
    pub fn choose(&self, features: &BuildFeatures, legal: &[BuildChoice]) -> Option<BuildChoice> {
        self.choose_with(features, legal, None)
    }

    /// [`BuildModel::choose`] with a term added to the utility of each kind.
    /// `bias` is indexed by [`UnitKind::index`] ([`effect_bias`]). "No build"
    /// gets no term. With `None`, the result is that of
    /// [`BuildModel::choose`].
    pub fn choose_with(
        &self,
        features: &BuildFeatures,
        legal: &[BuildChoice],
        bias: Option<&[f64; UnitKind::COUNT]>,
    ) -> Option<BuildChoice> {
        let mut best: Option<(BuildChoice, f64)> = None;
        for (choice, utility) in legal.iter().zip(self.utilities(features, legal)) {
            let Some(mut utility) = utility else {
                continue;
            };
            if let (Some(bias), BuildChoice::Build(kind)) = (bias, choice) {
                utility += bias[kind.index()];
            }
            if best.is_none_or(|(_, top)| utility > top) {
                best = Some((*choice, utility));
            }
        }
        best.map(|(choice, _)| choice)
    }
}

fn check(mean: f64, scale: f64, what: &'static str) -> Result<(), BuildModelError> {
    if mean.is_finite() && scale.is_finite() && scale > 0.0 {
        Ok(())
    } else {
        Err(BuildModelError::Value(what))
    }
}

/// The build models that a configuration can name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BuildModelId {
    /// The greedy build score chooses the kind.
    #[default]
    Off,
    /// The fit on all maps of Amarriner Cups 1 to 4, Division 1, and the
    /// Global League games. Cups 5 and 6 are the holdout. See
    /// `assets/ai-diagnostics/build-policy/README.md`.
    HumanV2a,
}

impl BuildModelId {
    pub const fn is_off(&self) -> bool {
        matches!(self, Self::Off)
    }

    /// The model of this name. `None` for [`BuildModelId::Off`].
    pub fn model(self) -> Option<&'static BuildModel> {
        static HUMAN_V2A: LazyLock<BuildModel> = LazyLock::new(|| {
            BuildModel::from_json(HUMAN_V2A_JSON).expect("the human-v2a build model is valid")
        });
        match self {
            Self::Off => None,
            Self::HumanV2a => Some(&HUMAN_V2A),
        }
    }
}

/// The text of the human-v2a model.
pub const HUMAN_V2A_JSON: &str =
    include_str!("../../../assets/ai-diagnostics/build-policy/human-v2a-model.json");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::{Limits, play_observed};
    use crate::rng::Rng;
    use awvm::semantic::{AwbwVisibility, Observation, observe};
    use awvm::session::{LegalScope, LegalVisitor, Order, OrderKind, Session};

    /// The legal production sites of the active player, with their kinds.
    fn sites(session: &Session) -> Vec<(Pos, Vec<UnitKind>)> {
        struct Collector(Vec<(Pos, Vec<UnitKind>)>, awvm::semantic::Dimensions);
        impl LegalVisitor for Collector {
            fn order(&mut self, order: Order) {
                let OrderKind::Produce(kind) = order.kind() else {
                    return;
                };
                let position = self.1.position_of(order.destination()).unwrap();
                match self.0.iter_mut().find(|(known, _)| *known == position) {
                    Some((_, kinds)) => kinds.push(kind),
                    None => self.0.push((position, vec![kind])),
                }
            }
        }
        let mut collector = Collector(Vec::new(), session.state().board.dimensions());
        session.legal().visit_scoped(
            LegalScope {
                units: &[],
                unitless: true,
            },
            &mut collector,
        );
        collector.0
    }

    /// The views of the active player during a Hard game on the arena,
    /// sampled where the player can build.
    fn build_views() -> Vec<(State, Observation)> {
        let state = crate::board::arena(false, 3);
        let mut session = Session::new(state.clone());
        let mut first = crate::profile::HARD_V2.agent(1);
        let mut second = crate::profile::HARD_V2.agent(2);
        let mut agents: [&mut dyn crate::Agent; 2] = [&mut *first, &mut *second];
        let mut entropy = Rng::from_seed(5);
        let limits = Limits {
            days: 20,
            ..Limits::DEFAULT
        };
        let mut views = Vec::new();
        let mut count = 0_usize;
        play_observed(
            state,
            &mut session,
            &mut agents,
            &mut entropy,
            limits,
            |state, _| {
                count += 1;
                if !count.is_multiple_of(3)
                    || !matches!(state.match_state, awvm::semantic::Match::Active { .. })
                {
                    return;
                }
                let view = observe(&AwbwVisibility, state, &state.turn.active_player).unwrap();
                if !sites(&Session::from_observation(&view).unwrap()).is_empty() {
                    views.push((state.clone(), view));
                }
            },
        )
        .unwrap();
        views
    }

    /// The `ai-hard-v3` greedy configuration with the build model on.
    fn model_config() -> crate::baseline::BaselineConfig {
        crate::baseline::BaselineConfig {
            weights: crate::agents::Weights {
                build_model: BuildModelId::HumanV2a,
                ..crate::profile::HARD_V3_CONFIG.weights
            },
            ..crate::profile::HARD_V3_CONFIG
        }
    }

    /// The replay extractor reads the features from a session of the
    /// observation. The greedy policy does the same in play. The features
    /// and the choices of the two paths must agree, and they must agree
    /// with the authoritative board when there is no fog.
    #[test]
    fn the_replay_path_and_the_play_path_read_the_same_features() {
        let model = BuildModelId::HumanV2a.model().unwrap();
        let views = build_views();
        assert!(views.len() > 20, "{} views", views.len());
        let mut kinds = std::collections::BTreeSet::new();
        for (state, view) in &views {
            // The replay path.
            let session = Session::from_observation(view).unwrap();
            let seat = session
                .state()
                .players
                .seat(&session.state().turn.active_player)
                .unwrap();
            let context = BuildContext::new(session.state(), seat);
            let authoritative = BuildContext::new(state, seat);
            let mut expected = Vec::new();
            for (site, legal) in sites(&session) {
                let features = context.site(site);
                assert_eq!(features, authoritative.site(site));
                let choices: Vec<_> = legal.iter().copied().map(BuildChoice::Build).collect();
                let Some(BuildChoice::Build(kind)) = model.choose(&features, &choices) else {
                    panic!("the model chose no legal kind");
                };
                assert!(legal.contains(&kind));
                assert!(context.cost(kind) <= session.state().player(seat).funds as f64);
                kinds.insert(kind);
                expected.push((site, kind));
            }
            // The play path.
            let mut greedy = model_config().build_greedy(3);
            assert_eq!(greedy.model_build_choices(view), expected);
            let mut greedy = crate::profile::V8_GREEDY_CONFIG.build_greedy(3);
            assert_eq!(greedy.model_build_choices(view), expected);
            let mut off = crate::profile::HARD_V3_CONFIG.build_greedy(3);
            assert!(off.model_build_choices(view).is_empty());
        }
        assert!(kinds.len() > 1, "{kinds:?}");
    }

    /// With the model on, the greedy policy builds only the chosen kind at a
    /// site, at the rank of the best greedy build there.
    #[test]
    fn the_greedy_policy_builds_the_kind_that_the_model_chooses() {
        for (state, view) in build_views() {
            let mut model = model_config().build_greedy(3);
            let mut stock = crate::profile::HARD_V3_CONFIG.build_greedy(3);
            let choices = model.model_build_choices(&view);
            let scored = model.scored_unitless_orders(&view).unwrap();
            let stock_scored = stock.scored_unitless_orders(&view).unwrap();
            let dimensions = state.board.dimensions();
            for (site, chosen) in choices {
                let cell = dimensions.cell_index(site).unwrap();
                let at_site = |orders: &crate::ScoredOrders| {
                    orders
                        .candidates()
                        .iter()
                        .filter(|candidate| candidate.order.destination() == cell)
                        .filter_map(|candidate| match candidate.order.kind() {
                            OrderKind::Produce(kind) => Some((kind, candidate.score)),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                };
                let best = at_site(&stock_scored)
                    .into_iter()
                    .map(|(_, score)| score)
                    .fold(f64::NEG_INFINITY, f64::max);
                for (kind, score) in at_site(&scored) {
                    if kind == chosen {
                        assert_eq!(score, best);
                    } else {
                        assert_eq!(score, 0.0);
                    }
                }
            }
        }
    }

    #[test]
    fn the_model_loads_and_evaluates_deterministically() {
        let first = BuildModel::from_json(HUMAN_V2A_JSON).unwrap();
        let second = BuildModel::from_json(HUMAN_V2A_JSON).unwrap();
        assert_eq!(first, second);
        assert_eq!(BuildModelId::HumanV2a.model(), Some(&first));
        assert_eq!(BuildModelId::Off.model(), None);
        let state = crate::board::arena(false, 3);
        let seat = state.players.seat(&state.turn.active_player).unwrap();
        let context = BuildContext::new(&state, seat);
        let (site, _) = state
            .board
            .iter()
            .find(|(_, tile)| {
                tile.owner.is_owned_by(seat)
                    && ruleset::terrain_has(tile.terrain, TerrainTrait::ProducesGround)
            })
            .unwrap();
        let features = context.site(site);
        let legal = [
            BuildChoice::Build(UnitKind::Infantry),
            BuildChoice::Build(UnitKind::Mech),
            BuildChoice::Build(UnitKind::Recon),
            BuildChoice::NoBuild,
        ];
        let probabilities = first.probabilities(&features, &legal);
        assert_eq!(probabilities, second.probabilities(&features, &legal));
        assert!((probabilities.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        let top = (0..legal.len())
            .max_by(|a, b| probabilities[*a].total_cmp(&probabilities[*b]))
            .unwrap();
        assert_eq!(first.choose(&features, &legal), Some(legal[top]));
    }

    /// The model holds the kinds that humans build, and never prefers "no
    /// build".
    #[test]
    fn the_model_never_prefers_no_build() {
        let model = BuildModelId::HumanV2a.model().unwrap();
        let (state, seat, _, site) = arena_site();
        let features = BuildContext::new(&state, seat).site(site);
        let legal = [
            BuildChoice::Build(UnitKind::Infantry),
            BuildChoice::Build(UnitKind::Tank),
            BuildChoice::NoBuild,
        ];
        let probabilities = model.probabilities(&features, &legal);
        assert!(probabilities[2] < 1e-100, "{probabilities:?}");
        assert!(matches!(
            model.choose(&features, &legal),
            Some(BuildChoice::Build(_))
        ));
    }

    /// The arena, the active seat, the other seat, and an empty ground site
    /// of the active seat.
    fn arena_site() -> (State, PlayerIdx, PlayerIdx, Pos) {
        let state = crate::board::arena(false, 3);
        let seat = state.players.seat(&state.turn.active_player).unwrap();
        let other = state
            .players
            .seats()
            .map(|(other, _)| other)
            .find(|other| *other != seat)
            .unwrap();
        let (site, _) = state
            .board
            .iter()
            .find(|(position, tile)| {
                tile.owner.is_owned_by(seat)
                    && ruleset::terrain_has(tile.terrain, TerrainTrait::ProducesGround)
                    && !state.units.iter().any(|unit| {
                        unit.location
                            == (Location::Board {
                                position: *position,
                            })
                    })
            })
            .unwrap();
        (state, seat, other, site)
    }

    fn new_unit(id: u32, kind: UnitKind, owner: PlayerIdx, position: Pos) -> awvm::semantic::Unit {
        let profile = ruleset::profile(kind);
        awvm::semantic::Unit {
            id: awvm::semantic::UnitId::new(id),
            kind,
            owner,
            hp: 100,
            fuel: profile.max_fuel,
            ammo: profile.max_ammo,
            action: awvm::semantic::UnitAction::Ready,
            concealment: awvm::semantic::Concealment::Exposed,
            location: Location::Board { position },
        }
    }

    fn decision(features: &BuildFeatures, name: &str) -> f64 {
        let index = decision_feature_names()
            .iter()
            .position(|known| known == name)
            .unwrap();
        features.decision[index]
    }

    fn kind(features: &BuildFeatures, unit: UnitKind, name: &str) -> f64 {
        let index = KIND_FEATURES
            .iter()
            .position(|known| *known == name)
            .unwrap();
        features.kind[unit.index()][index]
    }

    /// A tile that holds no unit, at `distance` from `from` or nearer, and
    /// that is not `from`.
    fn free_tile(state: &State, from: Pos, distance: u64) -> Pos {
        state
            .board
            .iter()
            .map(|(position, _)| position)
            .find(|position| {
                *position != from
                    && position.distance(from) <= distance
                    && !state.units.iter().any(|unit| {
                        unit.location
                            == (Location::Board {
                                position: *position,
                            })
                    })
            })
            .unwrap()
    }

    /// A unit that we built this turn counts in the turn features. A unit
    /// that is ready, or that has lost health, does not.
    #[test]
    fn a_unit_built_this_turn_counts_in_the_turn_features() {
        let (mut state, seat, _, site) = arena_site();
        let before = BuildContext::new(&state, seat);
        let other_site = state
            .board
            .iter()
            .map(|(position, _)| position)
            .find(|position| {
                *position != site
                    && before
                        .empty_site_distance
                        .iter()
                        .any(|(p, _)| p == position)
            })
            .expect("the arena has two empty ground sites");
        let features = before.site(other_site);
        assert_eq!(decision(&features, "built_turn"), 0.0);
        assert_eq!(kind(&features, UnitKind::Tank, "built_turn_kind"), 0.0);

        let mut built = new_unit(9001, UnitKind::Tank, seat, site);
        built.action = awvm::semantic::UnitAction::Spent;
        state.units.push(built);
        let features = BuildContext::new(&state, seat).site(other_site);
        assert_eq!(decision(&features, "built_turn"), 1.0);
        assert_eq!(decision(&features, "built_turn_value"), 7.0);
        assert_eq!(kind(&features, UnitKind::Tank, "built_turn_kind"), 1.0);
        assert_eq!(kind(&features, UnitKind::Tank, "built_turn_class"), 1.0);
        assert_eq!(kind(&features, UnitKind::MdTank, "built_turn_class"), 0.0);
        assert_eq!(kind(&features, UnitKind::Infantry, "built_turn_kind"), 0.0);

        for change in [
            |unit: &mut awvm::semantic::Unit| unit.action = awvm::semantic::UnitAction::Ready,
            |unit: &mut awvm::semantic::Unit| unit.hp = 90,
        ] {
            let mut changed = state.clone();
            change(
                changed
                    .units
                    .get_mut(awvm::semantic::UnitId::new(9001))
                    .unwrap(),
            );
            let features = BuildContext::new(&changed, seat).site(other_site);
            assert_eq!(decision(&features, "built_turn"), 0.0);
        }
    }

    /// The funds after a build, and the funds that are missing to put an
    /// infantry on each other empty site.
    #[test]
    fn the_funds_after_the_build_features() {
        let (mut state, seat, _, site) = arena_site();
        state.player_mut(seat).funds = 9000;
        let context = BuildContext::new(&state, seat);
        let others = f64::from(context.empty_sites - 1);
        let features = context.site(site);
        assert_eq!(kind(&features, UnitKind::Tank, "remainder"), 2.0);
        assert_eq!(
            kind(&features, UnitKind::Tank, "fill_short"),
            (others - 2.0).max(0.0)
        );
        assert_eq!(kind(&features, UnitKind::Infantry, "remainder"), 8.0);
        assert_eq!(
            kind(&features, UnitKind::Infantry, "fill_short"),
            (others - 8.0).max(0.0)
        );
        state.player_mut(seat).funds = 7000;
        let context = BuildContext::new(&state, seat);
        let features = context.site(site);
        assert_eq!(kind(&features, UnitKind::Tank, "fill_short"), others);
    }

    /// An enemy battle copter is prey for an anti-air, and an enemy anti-air
    /// is a predator for a battle copter. The near exchange reads only the
    /// enemy units near the site.
    #[test]
    fn the_hard_counter_features_read_the_enemy_army() {
        let (mut state, seat, other, site) = arena_site();
        state.units.retain(|unit| unit.owner != other);
        let features = BuildContext::new(&state, seat).site(site);
        assert_eq!(kind(&features, UnitKind::AntiAir, "prey_share"), 0.0);
        assert_eq!(kind(&features, UnitKind::AntiAir, "counter_near"), 0.0);

        let near = free_tile(&state, site, 3);
        state
            .units
            .push(new_unit(9001, UnitKind::BCopter, other, near));
        let features = BuildContext::new(&state, seat).site(site);
        assert_eq!(kind(&features, UnitKind::AntiAir, "prey_share"), 1.0);
        assert_eq!(kind(&features, UnitKind::AntiAir, "prey_value"), 9.0);
        assert_eq!(kind(&features, UnitKind::Infantry, "prey_share"), 0.0);
        assert!(kind(&features, UnitKind::AntiAir, "counter_near") > 0.0);
        assert!(kind(&features, UnitKind::Infantry, "counter_near") < 0.0);
        assert_eq!(kind(&features, UnitKind::BCopter, "predator_share"), 0.0);

        let far = state
            .board
            .iter()
            .map(|(position, _)| position)
            .find(|position| {
                position.distance(site) > NEAR
                    && !state.units.iter().any(|unit| {
                        unit.location
                            == (Location::Board {
                                position: *position,
                            })
                    })
            })
            .unwrap();
        state
            .units
            .push(new_unit(9002, UnitKind::AntiAir, other, far));
        let features = BuildContext::new(&state, seat).site(site);
        assert_eq!(kind(&features, UnitKind::BCopter, "predator_share"), 0.5);
        assert_eq!(kind(&features, UnitKind::AntiAir, "prey_share"), 0.5);
        // The far anti-air does not change the near exchange.
        let mut only_near = state.clone();
        only_near
            .units
            .retain(|unit| unit.id != awvm::semantic::UnitId::new(9002));
        assert_eq!(
            kind(&features, UnitKind::BCopter, "counter_near"),
            kind(
                &BuildContext::new(&only_near, seat).site(site),
                UnitKind::BCopter,
                "counter_near"
            )
        );
    }

    /// The commander features read the day-to-day effects of our commander.
    #[test]
    fn the_commander_features_read_the_day_to_day_effects() {
        let (mut state, seat, _, site) = arena_site();
        let features = BuildContext::new(&state, seat).site(site);
        for unit in UnitKind::ALL {
            assert_eq!(kind(&features, unit, "co_attack"), 0.0, "{unit:?}");
            assert_eq!(kind(&features, unit, "co_defense"), 0.0, "{unit:?}");
        }
        for co in state.player_mut(seat).commanders.iter_mut() {
            co.id = CommanderKind::Max;
        }
        let features = BuildContext::new(&state, seat).site(site);
        assert!(kind(&features, UnitKind::Tank, "co_attack") > 0.0);
        assert!(kind(&features, UnitKind::Artillery, "co_attack") < 0.0);
        assert_eq!(kind(&features, UnitKind::Infantry, "co_attack"), 0.0);
    }

    /// Set the commander of `seat` to `co`.
    fn set_commander(state: &mut State, seat: PlayerIdx, co: CommanderKind) {
        for commander in state.player_mut(seat).commanders.iter_mut() {
            commander.id = co;
        }
    }

    /// The probability of an indirect kind at `site` under the human-v2a
    /// model, with the effect term of `strength`. All ground kinds that the
    /// model holds are legal.
    fn indirect_share(state: &State, seat: PlayerIdx, site: Pos, strength: f64) -> f64 {
        let model = BuildModelId::HumanV2a.model().unwrap();
        let legal: Vec<BuildChoice> = UnitKind::ALL
            .into_iter()
            .filter(|kind| ruleset::profile(*kind).domain == ruleset::Domain::Ground)
            .map(BuildChoice::Build)
            .collect();
        let features = BuildContext::new(state, seat).site(site);
        let bias = effect_bias(state, seat, strength).unwrap_or([0.0; UnitKind::COUNT]);
        // The kinds that the model holds, with their utilities and the term.
        let utilities: Vec<(UnitKind, f64)> = legal
            .iter()
            .zip(model.utilities(&features, &legal))
            .filter_map(|(choice, utility)| match choice {
                BuildChoice::Build(kind) => Some((*kind, utility? + bias[kind.index()])),
                BuildChoice::NoBuild => None,
            })
            .collect();
        let top = utilities
            .iter()
            .map(|(_, utility)| *utility)
            .fold(f64::NEG_INFINITY, f64::max);
        let weights: Vec<(UnitKind, f64)> = utilities
            .iter()
            .map(|(kind, utility)| (*kind, (utility - top).exp()))
            .collect();
        let total: f64 = weights.iter().map(|(_, weight)| weight).sum();
        weights
            .iter()
            .filter(|(kind, _)| UnitClass::of(*kind) == UnitClass::Indirect)
            .map(|(_, weight)| weight / total)
            .sum()
    }

    /// A commander with no day-to-day effect gets no effect term, so the
    /// build choice does not change.
    #[test]
    fn a_commander_with_no_day_to_day_effect_gets_no_effect_term() {
        assert!(
            commander_effects(CommanderKind::Andy)
                .iter()
                .all(|effect| *effect == 0.0)
        );
        let (mut state, seat, _, site) = arena_site();
        state.player_mut(seat).funds = 30_000;
        set_commander(&mut state, seat, CommanderKind::Andy);
        assert!(effect_bias(&state, seat, 5.0).is_none());
        let model = BuildModelId::HumanV2a.model().unwrap();
        let legal: Vec<BuildChoice> = UnitKind::ALL.map(BuildChoice::Build).to_vec();
        let features = BuildContext::new(&state, seat).site(site);
        assert_eq!(
            model.choose_with(&features, &legal, None),
            model.choose(&features, &legal)
        );
        assert_eq!(
            indirect_share(&state, seat, site, 5.0),
            indirect_share(&state, seat, site, 0.0)
        );
        // Zero strength gives no term for each commander.
        set_commander(&mut state, seat, CommanderKind::Grit);
        assert!(effect_bias(&state, seat, 0.0).is_none());
    }

    /// The effect of Grit, read from the engine, is positive on the indirect
    /// kinds and negative on the direct vehicles. The effect of Max is the
    /// opposite. Infantry is not changed by either.
    #[test]
    fn the_commander_effects_read_the_engine() {
        let grit = commander_effects(CommanderKind::Grit);
        let max = commander_effects(CommanderKind::Max);
        let at = |table: &[f64; UnitKind::COUNT], kind: UnitKind| table[kind.index()];
        for kind in [UnitKind::Artillery, UnitKind::Rocket, UnitKind::Missile] {
            assert!(at(grit, kind) > 0.0, "{kind:?}");
            assert!(at(max, kind) < 0.0, "{kind:?}");
        }
        for kind in [UnitKind::Tank, UnitKind::MdTank, UnitKind::BCopter] {
            assert!(at(grit, kind) < 0.0, "{kind:?}");
            assert!(at(max, kind) > 0.0, "{kind:?}");
        }
        assert_eq!(at(grit, UnitKind::Infantry), 0.0);
        assert_eq!(at(max, UnitKind::Infantry), 0.0);
        // Attack 120 and range 3 to 4: ln 1.2 + ln (4 / 3).
        let expected = 1.2_f64.ln() + (4.0_f64 / 3.0).ln();
        assert!((at(grit, UnitKind::Artillery) - expected).abs() < 1e-12);
    }

    /// With the effect term, Grit prefers the indirect kinds more than
    /// without it, and Max prefers them less. A stronger term moves the
    /// choice more.
    #[test]
    fn the_effect_term_moves_the_indirect_share() {
        let (mut state, seat, _, site) = arena_site();
        state.player_mut(seat).funds = 30_000;
        set_commander(&mut state, seat, CommanderKind::Grit);
        let base = indirect_share(&state, seat, site, 0.0);
        let weak = indirect_share(&state, seat, site, 1.5);
        let strong = indirect_share(&state, seat, site, 5.0);
        assert!(base < weak && weak < strong, "{base} {weak} {strong}");
        set_commander(&mut state, seat, CommanderKind::Max);
        let base = indirect_share(&state, seat, site, 0.0);
        let strong = indirect_share(&state, seat, site, 5.0);
        assert!(strong < base, "{base} {strong}");
    }

    /// The nearest empty site to the enemy has rank zero and gap zero. A
    /// farther site has a positive rank and gap.
    #[test]
    fn the_front_features_rank_the_empty_sites() {
        let (state, seat, _, _) = arena_site();
        let context = BuildContext::new(&state, seat);
        assert!(context.empty_site_distance.len() >= 2);
        let nearest = context
            .empty_site_distance
            .iter()
            .map(|(_, distance)| *distance)
            .fold(f64::INFINITY, f64::min);
        for (site, distance) in &context.empty_site_distance {
            let features = context.site(*site);
            let rank = decision(&features, "site_front_rank");
            let gap = decision(&features, "site_front_gap");
            assert!((gap - (distance - nearest) / 10.0).abs() < 1e-12);
            assert_eq!(rank == 0.0, *distance == nearest, "{site:?}");
        }
    }

    #[test]
    fn a_model_file_with_an_unknown_feature_is_refused() {
        let mut file: BuildModelFile = serde_json::from_str(HUMAN_V2A_JSON).unwrap();
        file.decision_features[0] = "no-such-feature".into();
        assert!(matches!(
            BuildModel::from_file(&file),
            Err(BuildModelError::Feature(_))
        ));
    }

    #[test]
    fn the_feature_names_are_unique() {
        let names = decision_feature_names();
        let unique: std::collections::BTreeSet<_> = names.iter().collect();
        assert_eq!(unique.len(), names.len());
        assert_eq!(names.len(), DECISION_FEATURES);
    }

    /// The greedy weights serialize the build model fields only when they
    /// are on, so the earlier fingerprints do not change.
    #[test]
    fn the_build_model_fields_are_left_out_when_off() {
        let off = serde_json::to_string(&crate::profile::HARD_V3_CONFIG.weights).unwrap();
        assert!(!off.contains("build_model"));
        let on = serde_json::to_string(&model_config().weights).unwrap();
        assert!(on.contains("\"build_model\":\"human-v2a\""));
    }
}
