//! What this player remembers of the enemy in fog, and what it infers about
//! the enemy units it has not seen.
//!
//! An observation shows the enemy units that this player can see now. An
//! agent that reads only the observation treats each fogged tile as empty: a
//! tank that went into the fog one turn ago does not exist for it. This
//! module keeps three records across the views of one match.
//!
//! - **Tracks.** An enemy unit that goes out of sight becomes a track: its
//!   kind, its health, and the tiles where it can be now. Each enemy turn
//!   adds the tiles that the unit can move to. A tile that this player can
//!   see and that is empty is removed. A track ends when a visible unit of
//!   the same kind is in its area, when its area is empty, or after
//!   [`MAX_TRACK_TURNS`] enemy turns.
//! - **Properties.** Fog hides the owner of a property. The memory keeps the
//!   last owner seen on each property. Early in a match, few enemy
//!   properties are visible, so the estimate assumes that the enemy captures
//!   at the same rate as this player. The properties seen as enemy-owned are
//!   the minimum. The properties that are not seen as ours or neutral now are
//!   the maximum.
//! - **Value ledger.** Fog hides the enemy funds. The ledger assumes that the
//!   enemy spends the same share of its income as this player does, and
//!   subtracts the damage that this player is seen to do. In a two-player
//!   match, every fight includes a unit of this player, so this player sees
//!   all of that damage.
//!
//! The difference between the ledger value and the value of the enemy units
//! that are visible or tracked is the *residual*: value that the enemy has
//! and that this player has not seen.
//!
//! The memory reads only the observations of its own player. A test or a
//! diagnostic can compare it with the true state through
//! [`FogMemory::accuracy`]. An agent must never do this.

use std::sync::Arc;

use awvm::query::{self, Travel};
use awvm::ruleset::{self, TerrainTrait, UnitKind};
use awvm::semantic::{
    CAPTURE_REQUIRED_POINTS, CellIdx, Dimensions, Location, Observation, ObservedPlayer,
    ObservedTileOwner, ObservedUnitHp, PlayerId, PlayerIdx, Pos, State, TileOwner, TileVisibility,
};

use crate::threat::{ThreatOverlay, hostile};

/// The largest number of enemy turns that a track is kept for.
///
/// After three turns the area of most units covers a large part of the board,
/// and the threat of the track is spread too thin to change a decision.
pub const MAX_TRACK_TURNS: u32 = 3;

/// The weight of a track's threat for each enemy turn since it was seen.
///
/// A unit seen this turn is where it was seen, and its threat counts in full.
/// A unit seen one turn ago can be at any tile of its area, so its threat
/// covers more tiles at a lower weight.
const TRACK_DECAY: f32 = 0.5;

/// A unit at whole health.
const WHOLE: u8 = 100;

/// One enemy unit in one view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Sighting {
    kind: UnitKind,
    owner: PlayerIdx,
    hp: u8,
    /// False when the enemy commander hides health. `hp` is then whole.
    hp_known: bool,
    cell: CellIdx,
}

/// An enemy unit that this player saw and cannot see now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Track {
    pub kind: UnitKind,
    pub owner: PlayerIdx,
    /// The health when last seen, in hundredths of a unit.
    pub hp: u8,
    /// Where the unit was last seen.
    pub last_seen: Pos,
    /// Enemy turns since the unit was last seen.
    pub turns: u32,
    /// The tiles where the unit can be now, in cell order.
    area: Vec<CellIdx>,
}

impl Track {
    /// The tiles where the unit can be now, in cell order.
    pub fn area(&self) -> &[CellIdx] {
        &self.area
    }

    /// The funds the unit is worth at its last seen health.
    pub fn value(&self) -> f64 {
        value(self.kind, self.hp)
    }

    fn weight(&self) -> f32 {
        TRACK_DECAY.powi(i32::try_from(self.turns).unwrap_or(i32::MAX))
    }
}

/// The last owner seen on one tile.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Seen {
    /// The tile is not an income property, or it was never visible.
    #[default]
    Unknown,
    Neutral,
    Held(PlayerIdx),
}

/// What this player remembers of one income property.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Property {
    income: bool,
    owner: Seen,
    /// This player held the property in its first view.
    ours_at_start: bool,
    /// This player held the property at some time.
    ever_ours: bool,
    /// The last owner seen before this player took the property.
    origin: Seen,
    /// This player took the property when it was neutral or unseen.
    captured_neutral: bool,
}

/// The income and losses of one enemy.
#[derive(Clone, Debug, PartialEq)]
struct EnemyLedger {
    seat: PlayerIdx,
    /// Turns of income counted.
    turns: u64,
    /// Starting funds and all estimated income.
    income: f64,
    /// The value that this player is seen to remove from the enemy army.
    losses: f64,
    /// The estimated properties in the last view. The income of an enemy
    /// turn that ends before the next own turn is counted with this value,
    /// because a capture of this player's turn pays this player first.
    properties: f64,
}

/// What this player remembers of the enemy, across the views of one match.
#[derive(Clone, Debug, Default)]
pub struct FogMemory {
    recipient: Option<PlayerId>,
    dimensions: Option<Dimensions>,
    /// The day of the own turn that the last view is in.
    day: Option<u64>,
    /// The enemy units in the last view.
    visible: Vec<Sighting>,
    tracks: Vec<Track>,
    properties: Vec<Property>,
    /// Income properties that are visible and neutral in the last view.
    visible_neutral: u32,
    our_start_properties: u32,
    /// The value of this player's units in its first view.
    our_start_value: f64,
    /// Starting funds and all income of this player.
    our_income: f64,
    our_turns: u64,
    our_funds: f64,
    /// The share of its income that this player spent, at the end of its
    /// last complete turn.
    spend_ratio: f64,
    enemies: Vec<EnemyLedger>,
    /// Increases after each view that changes the memory.
    version: u64,
}

/// The estimate of one enemy for one position.
#[derive(Clone, Debug, PartialEq)]
pub struct EnemyEstimate {
    pub seat: PlayerIdx,
    /// Estimated income properties.
    pub properties: f64,
    /// Income properties seen as held by the enemy.
    pub known_properties: u32,
    /// Starting funds and all estimated income.
    pub income: f64,
    /// Estimated value of the enemy army, in funds.
    pub value: f64,
    /// Value of the visible enemy units.
    pub visible: f64,
    /// Value of the tracked enemy units.
    pub tracked: f64,
    /// Value that is neither visible nor tracked.
    pub residual: f64,
}

/// What the agent reads from the fog memory for one position.
#[derive(Debug)]
pub struct FogEstimate {
    /// The seat that the estimate is for.
    pub seat: PlayerIdx,
    /// The threat of the tracked units.
    pub threat: Arc<ThreatOverlay>,
    /// The tracked units, as kind and health share of a whole unit.
    pub tracks: Vec<(UnitKind, f64)>,
    /// The enemy value that is neither visible nor tracked, in funds.
    pub residual: f64,
    pub enemies: Vec<EnemyEstimate>,
}

/// The memory compared with the true state, for one enemy.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FogAccuracy {
    pub day: u64,
    /// The true value of the enemy army.
    pub true_value: f64,
    /// The ledger estimate of the enemy army.
    pub estimated_value: f64,
    /// The estimate of the old rule: the enemy army equals ours now.
    pub mirror_value: f64,
    pub true_properties: u32,
    pub estimated_properties: f64,
    /// Income properties seen as held by the enemy.
    pub known_properties: u32,
    /// The true value of the enemy units that this player cannot see.
    pub hidden_value: f64,
    pub tracks: u32,
    /// Tracks with a hidden enemy unit of the same kind in their area.
    pub tracks_found: u32,
    /// Value of tracked units.
    pub tracked_value: f64,
    /// Starting funds and the estimated enemy income.
    pub estimated_income: f64,
    /// The estimated enemy spending.
    pub estimated_spent: f64,
    /// The value that this player is seen to remove from the enemy.
    pub estimated_losses: f64,
}

impl FogMemory {
    /// An empty memory.
    pub fn new() -> Self {
        Self::default()
    }

    /// Increases after each view that changes the memory.
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// The tracked enemy units.
    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    /// Read one view of the recipient.
    ///
    /// Give the memory each view of its own player in order: the view at the
    /// start of each own turn and the view before each own command. Views of
    /// other players and views without fog are ignored.
    pub fn update(&mut self, view: &Observation) {
        if !view.settings.fog || view.turn.active_player != view.recipient {
            return;
        }
        if self
            .recipient
            .as_ref()
            .is_some_and(|recipient| *recipient != view.recipient)
        {
            *self = Self::new();
        }
        let Ok(state) = query::reify(view) else {
            return;
        };
        let Some(us) = state.players.seat(&view.recipient) else {
            return;
        };
        let dimensions = state.board.dimensions();
        if self.dimensions != Some(dimensions) {
            *self = Self::new();
        }
        let first = self.recipient.is_none();
        self.recipient = Some(view.recipient.clone());
        self.dimensions = Some(dimensions);

        let visible = sightings(view, &state);
        let new_turn = self.day != Some(view.turn.day);
        if first {
            self.start(view, &state, us);
        } else if new_turn {
            self.next_turn(&state);
        } else {
            self.compare(view, &visible);
        }
        self.day = Some(view.turn.day);
        self.match_tracks(view, &visible);
        self.remember_properties(view, &state, us);
        if new_turn {
            self.count_income(view, &state, us);
        }
        for index in 0..self.enemies.len() {
            let seat = self.enemies[index].seat;
            self.enemies[index].properties = self.properties_of(&state, us, seat);
        }
        self.our_funds = own_funds(view);
        self.visible = visible;
        self.version += 1;
    }

    fn start(&mut self, view: &Observation, state: &State, us: PlayerIdx) {
        let dimensions = state.board.dimensions();
        self.properties = vec![Property::default(); dimensions.len()];
        for (position, tile) in state.board.iter() {
            let Some(cell) = dimensions.cell_index(position) else {
                continue;
            };
            self.properties[usize::from(cell.get())].income =
                ruleset::terrain_has(tile.terrain, TerrainTrait::Income);
        }
        self.our_start_value = state
            .units
            .iter()
            .filter(|unit| unit.owner == us)
            .map(|unit| value(unit.kind, unit.hp))
            .sum();
        // The memory starts with no income counted, and the first own turn
        // counts it.
        let starting = view.settings.starting_funds as f64;
        self.our_income = starting;
        self.spend_ratio = 1.0;
        self.enemies = state
            .players
            .seats()
            .map(|(seat, _)| seat)
            .filter(|seat| hostile(state, us, *seat))
            .map(|seat| EnemyLedger {
                seat,
                turns: 0,
                income: starting,
                losses: 0.0,
                properties: 0.0,
            })
            .collect();
    }

    /// The enemy moved since the last view. Every unit in the last view
    /// becomes a track, and every track grows by one turn of movement.
    fn next_turn(&mut self, state: &State) {
        if self.our_income > 0.0 {
            let spent = (self.our_income - self.our_funds).max(0.0);
            self.spend_ratio = (spent / self.our_income).clamp(0.0, 1.0);
        }
        let dimensions = state.board.dimensions();
        for sighting in std::mem::take(&mut self.visible) {
            let Some(position) = dimensions.position_of(sighting.cell) else {
                continue;
            };
            self.tracks.push(Track {
                kind: sighting.kind,
                owner: sighting.owner,
                hp: sighting.hp,
                last_seen: position,
                turns: 0,
                area: vec![sighting.cell],
            });
        }
        let mut search = Reach::default();
        for track in &mut self.tracks {
            track.turns += 1;
            let Some(mut travel) = Travel::open(state, track.owner) else {
                continue;
            };
            let profile = ruleset::profile(track.kind);
            let costs = travel.costs(profile.movement_class);
            let allowance = u16::try_from(profile.movement).unwrap_or(u16::MAX);
            track.area = search.run(state.board.dimensions(), costs, &track.area, allowance);
        }
        self.tracks.retain(|track| track.turns <= MAX_TRACK_TURNS);
    }

    /// Compare two views of one own turn. The enemy does not move during
    /// this turn, so a change to an enemy unit is either damage done by this
    /// player, or a change of what this player can see.
    fn compare(&mut self, view: &Observation, visible: &[Sighting]) {
        let dimensions = match self.dimensions {
            Some(dimensions) => dimensions,
            None => return,
        };
        for before in std::mem::take(&mut self.visible) {
            let after = visible.iter().find(|after| {
                after.cell == before.cell
                    && after.kind == before.kind
                    && after.owner == before.owner
            });
            match after {
                Some(after) => {
                    if before.hp_known && after.hp_known && after.hp < before.hp {
                        let lost = value(before.kind, before.hp - after.hp);
                        self.add_loss(before.owner, lost);
                    }
                }
                None => {
                    let Some(position) = dimensions.position_of(before.cell) else {
                        continue;
                    };
                    let seen = view
                        .board
                        .get(position)
                        .is_some_and(|tile| tile.visibility == TileVisibility::Visible);
                    if seen && !conceals(before.kind) {
                        // The tile is visible and the unit is not on it. The
                        // enemy cannot move in this turn, so this player
                        // destroyed the unit.
                        self.add_loss(before.owner, value(before.kind, before.hp));
                    } else {
                        self.tracks.push(Track {
                            kind: before.kind,
                            owner: before.owner,
                            hp: before.hp,
                            last_seen: position,
                            turns: 0,
                            area: vec![before.cell],
                        });
                    }
                }
            }
        }
    }

    fn add_loss(&mut self, seat: PlayerIdx, lost: f64) {
        if let Some(enemy) = self.enemies.iter_mut().find(|enemy| enemy.seat == seat) {
            enemy.losses += lost;
        }
    }

    /// End each track that a visible unit explains, and take the visible
    /// tiles out of the area of the others.
    fn match_tracks(&mut self, view: &Observation, visible: &[Sighting]) {
        let Some(dimensions) = self.dimensions else {
            return;
        };
        for sighting in visible {
            // The unit is the track that fits it best: the fewest turns,
            // then the nearest health, then the nearest last position. Two
            // units of one kind are common, and a match with the wrong one
            // would count its health difference as damage.
            let position = dimensions.position_of(sighting.cell);
            let found = self
                .tracks
                .iter()
                .enumerate()
                .filter(|(_, track)| {
                    track.kind == sighting.kind
                        && track.owner == sighting.owner
                        && track.area.binary_search(&sighting.cell).is_ok()
                })
                .min_by_key(|(_, track)| {
                    (
                        track.turns,
                        track.hp.abs_diff(sighting.hp),
                        position.map_or(0, |position| position.distance(track.last_seen)),
                    )
                })
                .map(|(index, _)| index);
            if let Some(index) = found {
                let track = self.tracks.remove(index);
                // A unit that has less health than when it was last seen took
                // damage from this player: a counterattack in the enemy
                // turn, or an attack that was not seen. A repair adds health
                // and is part of the enemy spending, so it is not counted.
                if sighting.hp_known && sighting.hp < track.hp {
                    self.add_loss(track.owner, value(track.kind, track.hp - sighting.hp));
                }
            }
        }
        for track in &mut self.tracks {
            if conceals(track.kind) {
                continue;
            }
            track.area.retain(|cell| {
                dimensions.position_of(*cell).is_none_or(|position| {
                    view.board
                        .get(position)
                        .is_none_or(|tile| tile.visibility != TileVisibility::Visible)
                })
            });
        }
        self.tracks.retain(|track| !track.area.is_empty());
    }

    fn remember_properties(&mut self, view: &Observation, state: &State, us: PlayerIdx) {
        let dimensions = state.board.dimensions();
        // This player always sees its own properties. A property of ours
        // that is dark now was captured. With one enemy, that enemy holds it.
        let captor = match self.enemies.as_slice() {
            [enemy] => Seen::Held(enemy.seat),
            _ => Seen::Unknown,
        };
        self.visible_neutral = 0;
        for (position, tile) in view.board.iter() {
            if tile.visibility != TileVisibility::Visible {
                if let Some(cell) = dimensions.cell_index(position) {
                    let property = &mut self.properties[usize::from(cell.get())];
                    if property.income && property.owner == Seen::Held(us) {
                        property.owner = captor;
                    }
                }
                continue;
            }
            let Some(cell) = dimensions.cell_index(position) else {
                continue;
            };
            let property = &mut self.properties[usize::from(cell.get())];
            if !property.income {
                continue;
            }
            let owner = match &tile.owner {
                ObservedTileOwner::Owned(player) => match state.players.seat(player) {
                    Some(seat) => Seen::Held(seat),
                    None => continue,
                },
                ObservedTileOwner::Neutral => Seen::Neutral,
                ObservedTileOwner::NotOwnable => continue,
            };
            if owner == Seen::Neutral {
                self.visible_neutral += 1;
            }
            let ours = owner == Seen::Held(us);
            if self.our_turns == 0 && ours {
                property.ours_at_start = true;
            } else if ours && property.owner != Seen::Held(us) {
                property.origin = property.owner;
                property.captured_neutral |=
                    matches!(property.owner, Seen::Neutral | Seen::Unknown);
            }
            property.ever_ours |= ours;
            property.owner = owner;
        }
        if self.our_turns == 0 {
            self.our_start_properties = self.count_held(us);
        }
    }

    fn count_held(&self, seat: PlayerIdx) -> u32 {
        let held = self
            .properties
            .iter()
            .filter(|property| property.owner == Seen::Held(seat))
            .count();
        u32::try_from(held).unwrap_or(u32::MAX)
    }

    /// Count the income of each turn that started since the last own turn.
    fn count_income(&mut self, view: &Observation, state: &State, us: PlayerIdx) {
        let rate = view.settings.income_per_property as f64;
        let first = self.our_turns == 0;
        self.our_turns += 1;
        if first && view.turn.day > 1 {
            // The memory starts in the middle of a match, for example in an
            // agent that a host makes again for each turn. The income before
            // this view is not known, so the ledger starts here: the enemy
            // army is our army now, and income counts from this turn.
            self.our_income = own_funds(view);
            for enemy in &mut self.enemies {
                enemy.turns = completed_turns(view, state, enemy.seat);
                enemy.income = 0.0;
            }
            return;
        }
        self.our_income += f64::from(self.count_held(us)) * rate;
        for index in 0..self.enemies.len() {
            let seat = self.enemies[index].seat;
            let turns = completed_turns(view, state, seat);
            let properties = if first {
                self.properties_of(state, us, seat)
            } else {
                self.enemies[index].properties
            };
            let enemy = &mut self.enemies[index];
            while enemy.turns < turns {
                enemy.turns += 1;
                enemy.income += properties * rate;
            }
        }
    }

    /// The estimated income properties of the enemy at `seat`.
    ///
    /// The estimate assumes that the enemy captured as many neutral
    /// properties as this player did. It is never fewer than the properties
    /// seen as held by the enemy. It is never more than the properties that
    /// are not ours, not neutral in this view, and not seen as held by
    /// another enemy.
    fn properties_of(&self, state: &State, us: PlayerIdx, seat: PlayerIdx) -> f64 {
        let mut ours = 0_u32;
        let mut captured = 0_u32;
        let mut taken_from_them = 0_u32;
        let mut lost_to_them = 0_u32;
        let mut theirs = 0_u32;
        let mut others = 0_u32;
        let mut total = 0_u32;
        for property in self.properties.iter().filter(|property| property.income) {
            total += 1;
            // Each neutral capture of ours is a neutral capture of theirs,
            // also when the property changed owner after that.
            if property.captured_neutral {
                captured += 1;
            }
            match property.owner {
                Seen::Held(owner) if owner == us => {
                    ours += 1;
                    if !property.ours_at_start && property.origin == Seen::Held(seat) {
                        taken_from_them += 1;
                    }
                }
                Seen::Held(owner) if owner == seat => {
                    theirs += 1;
                    if property.ever_ours {
                        lost_to_them += 1;
                    }
                }
                Seen::Held(owner) if hostile(state, us, owner) => others += 1,
                _ => {}
            }
        }
        let mirror = f64::from(self.our_start_properties + captured + lost_to_them)
            - f64::from(taken_from_them);
        let upper = f64::from(total)
            - f64::from(ours)
            - f64::from(self.visible_neutral)
            - f64::from(others);
        let lower = f64::from(theirs);
        mirror.min(upper.max(lower)).max(lower)
    }

    /// Build a position model from the current observation and this memory.
    ///
    /// A hidden property has no capture points in an observation. Simulation
    /// needs a value to complete a capture. Use full capture points and the
    /// last owner seen. An owner that was never seen is modeled as neutral.
    pub fn planning_state(&self, observed: &State) -> State {
        let mut state = observed.clone();
        if !state.settings.fog {
            return state;
        }
        let remembered = self.dimensions == Some(state.board.dimensions());
        for position in state.board.dimensions().positions() {
            let cell = state
                .board
                .dimensions()
                .cell_index(position)
                .expect("a board cell");
            let tile = state.board.tile_mut(position);
            if tile.owner != TileOwner::NotOwnable
                || !ruleset::terrain_has(tile.terrain, TerrainTrait::Capturable)
            {
                continue;
            }
            let owner = remembered
                .then(|| self.properties.get(usize::from(cell.get())))
                .flatten()
                .map_or(Seen::Unknown, |property| property.owner);
            tile.owner = match owner {
                Seen::Held(seat) => TileOwner::Owned(seat),
                Seen::Unknown | Seen::Neutral => TileOwner::Neutral,
            };
            tile.capture_points = Some(CAPTURE_REQUIRED_POINTS);
        }
        state
    }

    /// The full estimate of visible and tracked enemy units for `state`.
    ///
    /// `state` is the position that this player can see. Its terrain and
    /// commanders decide how far each tracked unit can move. Returns `None`
    /// before the first view.
    pub fn estimate(&self, state: &State) -> Option<FogEstimate> {
        self.estimate_with_tracks(state, false)
    }

    /// Estimate threat only from enemy positions known in this own turn.
    ///
    /// The enemy cannot move during this turn. After an enemy turn, its
    /// possible positions remain available through [`Self::estimate`] for
    /// diagnostics, but do not change tactical decisions.
    pub fn estimate_for_policy(&self, state: &State) -> Option<FogEstimate> {
        self.estimate_with_tracks(state, true)
    }

    fn estimate_with_tracks(&self, state: &State, known_only: bool) -> Option<FogEstimate> {
        let recipient = self.recipient.as_ref()?;
        let us = state.players.seat(recipient)?;
        let dimensions = state.board.dimensions();
        if self.dimensions != Some(dimensions) {
            return None;
        }
        let mut overlay = ThreatOverlay::new(dimensions);
        let mut search = Reach::default();
        let mut immediate = Vec::new();
        let mut deferred = Vec::new();
        for track in &self.tracks {
            if known_only && track.turns > 0 {
                continue;
            }
            let Some(mut travel) = Travel::open(state, track.owner) else {
                continue;
            };
            let profile = ruleset::profile(track.kind);
            let costs = travel.costs(profile.movement_class);
            let allowance = u16::try_from(profile.movement).unwrap_or(u16::MAX);
            let mut immediate_counts = vec![0_u32; dimensions.len()];
            let mut deferred_counts = vec![0_u32; dimensions.len()];
            // Each possible position has an equal share of the track. Count
            // a target once per position, even if several moves can reach it.
            for origin in &track.area {
                let stops = search.run(dimensions, costs, &[*origin], allowance);
                immediate.clear();
                deferred.clear();
                match profile.indirect_range {
                    Some(range) => {
                        ring(dimensions, &[*origin], range, &mut immediate);
                        let moved: Vec<CellIdx> = stops
                            .iter()
                            .copied()
                            .filter(|cell| cell != origin)
                            .collect();
                        ring(dimensions, &moved, range, &mut deferred);
                    }
                    None => {
                        let adjacent = ruleset::AttackRange {
                            minimum: 1,
                            maximum: 1,
                        };
                        ring(dimensions, &stops, adjacent, &mut immediate);
                    }
                }
                for cell in &immediate {
                    immediate_counts[usize::from(cell.get())] += 1;
                }
                for cell in &deferred {
                    deferred_counts[usize::from(cell.get())] += 1;
                }
            }
            let probabilities = |counts: &[u32]| -> Vec<(CellIdx, f32)> {
                counts
                    .iter()
                    .enumerate()
                    .filter(|(_, count)| **count > 0)
                    .filter_map(|(index, count)| {
                        Some((
                            CellIdx::from_raw(u16::try_from(index).ok()?),
                            *count as f32 / track.area.len() as f32,
                        ))
                    })
                    .collect()
            };
            overlay.add(
                state,
                track.kind,
                track.hp,
                track.weight(),
                &probabilities(&immediate_counts),
                &probabilities(&deferred_counts),
            );
        }

        let visible_units: Vec<Sighting> = state
            .units
            .iter()
            .filter(|unit| hostile(state, us, unit.owner))
            .filter_map(|unit| {
                let Location::Board { position } = unit.location else {
                    return None;
                };
                Some(Sighting {
                    kind: unit.kind,
                    owner: unit.owner,
                    hp: unit.hp,
                    hp_known: true,
                    cell: dimensions.cell_index(position)?,
                })
            })
            .collect();
        let enemies: Vec<EnemyEstimate> = self
            .enemies
            .iter()
            .map(|enemy| {
                let visible: f64 = visible_units
                    .iter()
                    .filter(|unit| unit.owner == enemy.seat)
                    .map(|unit| value(unit.kind, unit.hp))
                    .sum();
                let tracked: f64 = self
                    .tracks
                    .iter()
                    .filter(|track| track.owner == enemy.seat)
                    .map(Track::value)
                    .sum();
                let estimated = self.value_of(enemy);
                EnemyEstimate {
                    seat: enemy.seat,
                    properties: self.properties_of(state, us, enemy.seat),
                    known_properties: self.count_held(enemy.seat),
                    income: enemy.income,
                    value: estimated,
                    visible,
                    tracked,
                    residual: (estimated - visible - tracked).max(0.0),
                }
            })
            .collect();
        Some(FogEstimate {
            seat: us,
            threat: Arc::new(overlay),
            tracks: self
                .tracks
                .iter()
                .map(|track| (track.kind, f64::from(track.hp) / f64::from(WHOLE)))
                .collect(),
            residual: enemies.iter().map(|enemy| enemy.residual).sum(),
            enemies,
        })
    }

    /// The ledger value of one enemy army.
    fn value_of(&self, enemy: &EnemyLedger) -> f64 {
        (self.our_start_value + self.spend_ratio * enemy.income - enemy.losses).max(0.0)
    }

    /// Compare the memory with the true state.
    ///
    /// Use this only to score the memory. The true state must never reach
    /// the agent.
    pub fn accuracy(&self, truth: &State) -> Vec<FogAccuracy> {
        let Some(recipient) = self.recipient.as_ref() else {
            return Vec::new();
        };
        let Some(us) = truth.players.seat(recipient) else {
            return Vec::new();
        };
        let Ok(view) = awvm::semantic::observe(&awvm::semantic::AwbwVisibility, truth, recipient)
        else {
            return Vec::new();
        };
        let Ok(seen) = query::reify(&view) else {
            return Vec::new();
        };
        let Some(estimate) = self.estimate(&seen) else {
            return Vec::new();
        };
        let dimensions = truth.board.dimensions();
        let visible_cells: Vec<CellIdx> = sightings(&view, &seen)
            .into_iter()
            .map(|sighting| sighting.cell)
            .collect();
        let mirror: f64 = truth
            .units
            .iter()
            .filter(|unit| unit.owner == us)
            .map(|unit| value(unit.kind, unit.hp))
            .sum();
        estimate
            .enemies
            .iter()
            .map(|enemy| {
                let units = || truth.units.iter().filter(|unit| unit.owner == enemy.seat);
                let hidden = || {
                    units().filter(|unit| match unit.location {
                        Location::Board { position } => dimensions
                            .cell_index(position)
                            .is_some_and(|cell| !visible_cells.contains(&cell)),
                        Location::Cargo { .. } => true,
                    })
                };
                let tracks: Vec<&Track> = self
                    .tracks
                    .iter()
                    .filter(|track| track.owner == enemy.seat)
                    .collect();
                let found = tracks
                    .iter()
                    .filter(|track| {
                        hidden().any(|unit| {
                            unit.kind == track.kind
                                && matches!(unit.location, Location::Board { position }
                                    if dimensions.cell_index(position)
                                        .is_some_and(|cell| track.area.binary_search(&cell).is_ok()))
                        })
                    })
                    .count();
                let true_properties = truth
                    .board
                    .iter()
                    .filter(|(_, tile)| {
                        tile.owner.holder() == Some(&enemy.seat)
                            && ruleset::terrain_has(tile.terrain, TerrainTrait::Income)
                    })
                    .count();
                FogAccuracy {
                    day: truth.turn.day,
                    true_value: units().map(|unit| value(unit.kind, unit.hp)).sum(),
                    estimated_value: enemy.value,
                    mirror_value: mirror,
                    true_properties: u32::try_from(true_properties).unwrap_or(u32::MAX),
                    estimated_properties: enemy.properties,
                    known_properties: enemy.known_properties,
                    hidden_value: hidden().map(|unit| value(unit.kind, unit.hp)).sum(),
                    tracks: u32::try_from(tracks.len()).unwrap_or(u32::MAX),
                    tracks_found: u32::try_from(found).unwrap_or(u32::MAX),
                    tracked_value: enemy.tracked,
                    estimated_income: enemy.income,
                    estimated_spent: self.spend_ratio * enemy.income,
                    estimated_losses: self
                        .enemies
                        .iter()
                        .find(|ledger| ledger.seat == enemy.seat)
                        .map_or(0.0, |ledger| ledger.losses),
                }
            })
            .collect()
    }
}

/// The enemy units in one view, on the board.
fn sightings(view: &Observation, state: &State) -> Vec<Sighting> {
    let dimensions = state.board.dimensions();
    let Some(us) = state.players.seat(&view.recipient) else {
        return Vec::new();
    };
    view.units
        .iter()
        .filter_map(|unit| {
            let owner = state.players.seat(&unit.owner)?;
            if !hostile(state, us, owner) {
                return None;
            }
            let Location::Board { position } = unit.location else {
                return None;
            };
            let (hp, hp_known) = match unit.hp {
                ObservedUnitHp::Exact(hp) => (hp, true),
                ObservedUnitHp::Hidden(_) => (WHOLE, false),
            };
            Some(Sighting {
                kind: unit.kind,
                owner,
                hp,
                hp_known,
                cell: dimensions.cell_index(position)?,
            })
        })
        .collect()
}

/// The funds of the recipient in `view`.
fn own_funds(view: &Observation) -> f64 {
    view.players
        .iter()
        .find_map(|player| match player {
            ObservedPlayer::Private { id, funds, .. } if *id == view.recipient => {
                Some(*funds as f64)
            }
            _ => None,
        })
        .unwrap_or(0.0)
}

/// The turns that `seat` completed before this own turn started.
fn completed_turns(view: &Observation, state: &State, seat: PlayerIdx) -> u64 {
    let id = state.player_id(seat);
    let before_us = view
        .turn
        .order
        .iter()
        .position(|player| player == id)
        .is_some_and(|position| position < view.turn.position);
    if before_us {
        view.turn.day
    } else {
        view.turn.day.saturating_sub(1)
    }
}

/// Whether a unit of `kind` can hide on a visible tile.
fn conceals(kind: UnitKind) -> bool {
    ruleset::profile(kind).concealment.is_some()
}

/// The funds a unit of `kind` at `hp` is worth.
fn value(kind: UnitKind, hp: u8) -> f64 {
    ruleset::profile(kind).cost as f64 * f64::from(hp) / f64::from(WHOLE)
}

/// Add the tiles in `range` of each of `centers` to `out`, once each.
fn ring(
    dimensions: Dimensions,
    centers: &[CellIdx],
    range: ruleset::AttackRange,
    out: &mut Vec<CellIdx>,
) {
    let mut marked = vec![false; dimensions.len()];
    let reach = i16::try_from(range.maximum).unwrap_or(i16::MAX);
    for center in centers {
        let Some(center) = dimensions.position_of(*center) else {
            continue;
        };
        for dy in -reach..=reach {
            let span = reach - dy.abs();
            for dx in -span..=span {
                let Some(target) = center.offset(dx, dy) else {
                    continue;
                };
                if center.distance(target) < range.minimum {
                    continue;
                }
                let Some(cell) = dimensions.cell_index(target) else {
                    continue;
                };
                let index = usize::from(cell.get());
                if !marked[index] {
                    marked[index] = true;
                    out.push(cell);
                }
            }
        }
    }
    out.sort_unstable();
}

/// One turn of movement from a set of tiles, with the units on the board
/// ignored.
///
/// A unit of the enemy can pass its own units, and the units of this player
/// that block it are a small part of the board. A search that ignores them
/// overestimates the area a little and is the same for every position of
/// this player's units.
#[derive(Debug, Default)]
struct Reach {
    spent: Vec<u16>,
    queue: std::collections::BinaryHeap<std::cmp::Reverse<(u16, u16)>>,
}

impl Reach {
    /// The tiles that can be reached from `sources` with `allowance` movement
    /// points, the sources included, in cell order.
    fn run(
        &mut self,
        dimensions: Dimensions,
        costs: &[Option<u16>],
        sources: &[CellIdx],
        allowance: u16,
    ) -> Vec<CellIdx> {
        let cells = costs.len().min(dimensions.len());
        self.spent.clear();
        self.spent.resize(cells, u16::MAX);
        self.queue.clear();
        for source in sources {
            let index = usize::from(source.get());
            if index < cells {
                self.spent[index] = 0;
                self.queue.push(std::cmp::Reverse((0, source.get())));
            }
        }
        while let Some(std::cmp::Reverse((spent, cell))) = self.queue.pop() {
            let index = usize::from(cell);
            if spent > self.spent[index] {
                continue;
            }
            let Some(position) = dimensions.position_of(CellIdx::from_raw(cell)) else {
                continue;
            };
            for next in position.orthogonal() {
                let Some(next) = dimensions.cell_index(next) else {
                    continue;
                };
                let next = usize::from(next.get());
                let Some(cost) = costs.get(next).copied().flatten() else {
                    continue;
                };
                let total = spent.saturating_add(cost);
                if total > allowance || total >= self.spent[next] {
                    continue;
                }
                self.spent[next] = total;
                let next = u16::try_from(next).unwrap_or(u16::MAX);
                self.queue.push(std::cmp::Reverse((total, next)));
            }
        }
        self.spent
            .iter()
            .enumerate()
            .filter(|(_, spent)| **spent != u16::MAX)
            .filter_map(|(index, _)| u16::try_from(index).ok().map(CellIdx::from_raw))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::arena;
    use awvm::ruleset::Terrain;
    use awvm::semantic::{AwbwVisibility, Concealment, Unit, UnitAction, UnitId, observe};

    /// A fog arena with plain terrain, no properties and no units. The first
    /// seat is to move.
    fn open_field() -> (State, PlayerIdx, PlayerIdx) {
        let mut state = arena(true, 1);
        state.units.retain(|_| false);
        for position in state.board.dimensions().positions() {
            if let Some(tile) = state.board.get_mut(position) {
                tile.terrain = Terrain::Plain;
                tile.owner = TileOwner::NotOwnable;
                tile.capture_points = None;
            }
        }
        let us = state
            .players
            .seat(&state.turn.active_player)
            .expect("the active player holds a seat");
        let them = state
            .players
            .seats()
            .map(|(seat, _)| seat)
            .find(|seat| *seat != us)
            .expect("the arena seats two players");
        (state, us, them)
    }

    fn unit(id: u32, kind: UnitKind, owner: PlayerIdx, position: Pos) -> Unit {
        let profile = ruleset::profile(kind);
        Unit {
            id: UnitId::new(id),
            kind,
            owner,
            hp: WHOLE,
            fuel: profile.max_fuel,
            ammo: profile.max_ammo,
            action: UnitAction::Ready,
            concealment: Concealment::Exposed,
            location: Location::Board { position },
        }
    }

    fn view(state: &State) -> Observation {
        observe(&AwbwVisibility, state, &state.turn.active_player).expect("the state observes")
    }

    fn move_unit(state: &mut State, id: u32, position: Pos) {
        state
            .units
            .get_mut(UnitId::new(id))
            .expect("the unit is in play")
            .location = Location::Board { position };
    }

    fn cell(state: &State, position: Pos) -> CellIdx {
        state
            .board
            .dimensions()
            .cell_index(position)
            .expect("the position is on the board")
    }

    #[test]
    fn a_unit_that_walks_into_the_fog_is_tracked_where_it_can_be() {
        let (mut state, us, them) = open_field();
        state
            .units
            .push(unit(1, UnitKind::Infantry, us, Pos::new(2, 2)));
        state
            .units
            .push(unit(2, UnitKind::Tank, them, Pos::new(4, 2)));
        let mut memory = FogMemory::new();
        memory.update(&view(&state));
        assert!(memory.tracks().is_empty(), "a visible unit is not a track");

        state.turn.day = 2;
        move_unit(&mut state, 2, Pos::new(9, 2));
        memory.update(&view(&state));
        let [track] = memory.tracks() else {
            panic!("one track, found {:?}", memory.tracks());
        };
        assert_eq!(track.kind, UnitKind::Tank);
        assert_eq!(track.turns, 1);
        assert_eq!(track.last_seen, Pos::new(4, 2));
        assert!(
            track
                .area()
                .binary_search(&cell(&state, Pos::new(9, 2)))
                .is_ok(),
            "the tank can be where it went"
        );
        assert!(
            track
                .area()
                .binary_search(&cell(&state, Pos::new(3, 2)))
                .is_err(),
            "a visible empty tile is not in the area"
        );

        let seen = query::reify(&view(&state)).expect("the view reifies");
        let estimate = memory.estimate(&seen).expect("the memory has a view");
        assert!(
            estimate
                .threat
                .immediate(cell(&state, Pos::new(10, 2)), UnitKind::Infantry)
                > 0.0,
            "the tracked tank threatens the tiles it can reach"
        );
        assert_eq!(estimate.tracks, vec![(UnitKind::Tank, 1.0)]);

        state.turn.day = 3;
        move_unit(&mut state, 2, Pos::new(3, 2));
        memory.update(&view(&state));
        assert!(memory.tracks().is_empty(), "the tank is visible again");
    }

    #[test]
    fn a_hidden_property_can_be_captured_in_the_position_model() {
        let (mut state, us, _) = open_field();
        let target = Pos::new(5, 2);
        let tile = state.board.tile_mut(target);
        tile.terrain = Terrain::City;
        tile.owner = TileOwner::Neutral;
        tile.capture_points = Some(CAPTURE_REQUIRED_POINTS);
        state
            .units
            .push(unit(1, UnitKind::Infantry, us, Pos::new(2, 2)));
        let observation = view(&state);
        assert_eq!(
            observation.board.tile(target).visibility,
            TileVisibility::Fogged
        );
        let mut memory = FogMemory::new();
        memory.update(&observation);
        let observed = query::reify(&observation).expect("the view reifies");
        let play = crate::Play::new(
            UnitId::new(1),
            cell(&state, target),
            awvm::session::OrderKind::Capture,
        );
        let mut original = awvm::session::Session::new(observed.clone());
        let command = play.command(&original).expect("the capture has a path");
        let mut entropy = crate::Rng::from_seed(3);
        assert!(
            matches!(
                original.apply_command::<()>(command.clone(), &mut entropy, &mut ()),
                Err(awvm::session::Error::Failed(
                    awvm::transition::ExecuteError::UnsupportedRuleset
                ))
            ),
            "the observation has no capture points for the simulation"
        );
        let modeled = memory.planning_state(&observed);
        let mut session = awvm::session::Session::new(modeled);
        session
            .apply_command::<()>(command, &mut entropy, &mut ())
            .expect("the modeled capture plays");
        assert_eq!(session.state().board.tile(target).capture_points, Some(10));
        assert_eq!(
            observed.board.tile(target).capture_points,
            None,
            "the observation is not changed"
        );
    }

    #[test]
    fn the_position_model_uses_remembered_owners_and_accepts_new_observations() {
        let (mut state, us, them) = open_field();
        let known = Pos::new(6, 2);
        let unknown = Pos::new(15, 15);
        for position in [known, unknown] {
            let tile = state.board.tile_mut(position);
            tile.terrain = Terrain::Base;
            tile.owner = TileOwner::Owned(them);
            tile.capture_points = Some(CAPTURE_REQUIRED_POINTS);
        }
        state
            .units
            .push(unit(1, UnitKind::Recon, us, Pos::new(2, 2)));
        let mut memory = FogMemory::new();
        memory.update(&view(&state));
        move_unit(&mut state, 1, Pos::new(2, 12));
        memory.update(&view(&state));
        let observed = query::reify(&view(&state)).expect("the view reifies");
        let modeled = memory.planning_state(&observed);
        assert_eq!(observed.board.tile(known).owner, TileOwner::NotOwnable);
        assert_eq!(modeled.board.tile(known).owner, TileOwner::Owned(them));
        assert_eq!(
            modeled.board.tile(unknown).owner,
            TileOwner::Neutral,
            "unseen ownership does not enter the model"
        );

        state.board.tile_mut(known).owner = TileOwner::Neutral;
        move_unit(&mut state, 1, Pos::new(2, 2));
        memory.update(&view(&state));
        let observed = query::reify(&view(&state)).expect("the view reifies");
        assert_eq!(
            memory.planning_state(&observed).board.tile(known).owner,
            TileOwner::Neutral
        );
    }

    #[test]
    fn policy_threat_uses_only_positions_known_in_this_turn() {
        let (mut state, us, them) = open_field();
        state
            .units
            .push(unit(1, UnitKind::Recon, us, Pos::new(2, 2)));
        state
            .units
            .push(unit(2, UnitKind::Artillery, them, Pos::new(6, 2)));
        let mut memory = FogMemory::new();
        memory.update(&view(&state));
        move_unit(&mut state, 1, Pos::new(2, 12));
        memory.update(&view(&state));
        let seen = query::reify(&view(&state)).expect("the view reifies");
        let known = memory
            .estimate_for_policy(&seen)
            .expect("the memory has a view");
        assert!(
            !known.threat.is_empty(),
            "the artillery cannot move in our turn"
        );

        state.turn.day += 1;
        memory.update(&view(&state));
        let seen = query::reify(&view(&state)).expect("the view reifies");
        let uncertain = memory.estimate(&seen).expect("the memory has a view");
        assert!(
            !uncertain.threat.is_empty(),
            "diagnostics retain possible positions"
        );
        let policy = memory
            .estimate_for_policy(&seen)
            .expect("the memory has a view");
        assert!(
            policy.threat.is_empty(),
            "an inferred position does not change tactics"
        );
    }

    #[test]
    fn uncertain_threat_is_the_mean_of_possible_positions() {
        let (state, _, them) = open_field();
        let mut memory = FogMemory::new();
        memory.update(&view(&state));
        let origins = [Pos::new(6, 2), Pos::new(15, 15)];
        for kind in [UnitKind::Tank, UnitKind::Artillery] {
            let track = Track {
                kind,
                owner: them,
                hp: WHOLE,
                last_seen: origins[0],
                turns: 1,
                area: origins
                    .iter()
                    .map(|position| cell(&state, *position))
                    .collect(),
            };
            memory.tracks = vec![track.clone()];
            let uncertain = memory.estimate(&state).expect("the memory has a view");
            let fixed: Vec<_> = track
                .area
                .iter()
                .map(|origin| {
                    memory.tracks[0].area = vec![*origin];
                    memory.estimate(&state).expect("the memory has a view")
                })
                .collect();
            let mut partial = false;
            for position in state.board.dimensions().positions() {
                let target = cell(&state, position);
                for deferred in [false, true] {
                    let read = |estimate: &FogEstimate| {
                        if deferred {
                            estimate.threat.deferred(target, UnitKind::Infantry)
                        } else {
                            estimate.threat.immediate(target, UnitKind::Infantry)
                        }
                    };
                    let expected = (read(&fixed[0]) + read(&fixed[1])) / 2.0;
                    assert!((read(&uncertain) - expected).abs() < 0.01);
                    partial |= read(&fixed[0]) > 0.0 && read(&fixed[1]) == 0.0;
                }
            }
            assert!(partial, "some targets are in range of only one position");
        }
    }

    #[test]
    fn a_unit_that_leaves_sight_in_our_turn_is_where_it_was() {
        let (mut state, us, them) = open_field();
        state
            .units
            .push(unit(1, UnitKind::Recon, us, Pos::new(2, 2)));
        state
            .units
            .push(unit(2, UnitKind::Artillery, them, Pos::new(6, 2)));
        let mut memory = FogMemory::new();
        memory.update(&view(&state));
        assert_eq!(memory.visible.len(), 1, "the recon sees the artillery");

        // The recon drives away in this turn, and the artillery goes dark.
        move_unit(&mut state, 1, Pos::new(2, 12));
        memory.update(&view(&state));
        let [track] = memory.tracks() else {
            panic!("one track, found {:?}", memory.tracks());
        };
        assert_eq!(track.turns, 0);
        assert_eq!(track.area(), &[cell(&state, Pos::new(6, 2))]);
        assert_eq!(
            memory.enemies[0].losses, 0.0,
            "a unit out of sight is not a loss"
        );
    }

    #[test]
    fn a_property_of_ours_that_goes_dark_was_captured() {
        let mut state = arena(true, 1);
        state.units.retain(|_| false);
        let us = state
            .players
            .seat(&state.turn.active_player)
            .expect("the active player holds a seat");
        let them = state
            .players
            .seats()
            .map(|(seat, _)| seat)
            .find(|seat| *seat != us)
            .expect("the arena seats two players");
        let mut memory = FogMemory::new();
        memory.update(&view(&state));
        let start = memory.our_start_properties;

        let lost = state
            .board
            .iter()
            .find(|(_, tile)| tile.terrain == Terrain::Base && tile.owner == TileOwner::Owned(us))
            .map(|(position, _)| position)
            .expect("we hold a base");
        state
            .board
            .get_mut(lost)
            .expect("the base is on the board")
            .owner = TileOwner::Owned(them);
        state.turn.day = 2;
        let after = view(&state);
        assert_eq!(
            after.board.get(lost).map(|tile| tile.visibility),
            Some(TileVisibility::Fogged),
            "a captured base is dark to its old owner"
        );
        memory.update(&after);
        assert_eq!(memory.count_held(us), start - 1);
        assert_eq!(memory.count_held(them), 1, "the captor holds the base");
        let seen = query::reify(&after).expect("the view reifies");
        let estimate = memory.estimate(&seen).expect("the memory has a view");
        assert_eq!(
            estimate.enemies[0].properties,
            f64::from(start + 1),
            "the enemy holds its own properties and our base"
        );
    }

    #[test]
    fn a_unit_seen_again_with_less_health_counts_the_damage() {
        let (mut state, us, them) = open_field();
        state
            .units
            .push(unit(1, UnitKind::Tank, us, Pos::new(2, 2)));
        state
            .units
            .push(unit(2, UnitKind::Tank, them, Pos::new(3, 2)));
        let mut memory = FogMemory::new();
        memory.update(&view(&state));

        // The enemy attacked in its turn and took a counterattack.
        state.turn.day = 2;
        state
            .units
            .get_mut(UnitId::new(2))
            .expect("the tank is in play")
            .hp = 70;
        memory.update(&view(&state));
        let tank = ruleset::profile(UnitKind::Tank).cost as f64;
        assert!((memory.enemies[0].losses - 0.3 * tank).abs() < 1e-9);
        assert!(memory.tracks().is_empty());
    }

    #[test]
    fn a_memory_that_starts_mid_match_mirrors_our_army() {
        let (mut state, us, them) = open_field();
        state.turn.day = 9;
        state
            .units
            .push(unit(1, UnitKind::Tank, us, Pos::new(2, 2)));
        state
            .units
            .push(unit(2, UnitKind::Infantry, us, Pos::new(2, 3)));
        state
            .units
            .push(unit(3, UnitKind::Infantry, them, Pos::new(15, 15)));
        let mut memory = FogMemory::new();
        memory.update(&view(&state));
        let seen = query::reify(&view(&state)).expect("the view reifies");
        let estimate = memory.estimate(&seen).expect("the memory has a view");
        let ours = value(UnitKind::Tank, WHOLE) + value(UnitKind::Infantry, WHOLE);
        assert_eq!(
            estimate.enemies[0].income, 0.0,
            "no income before the first view"
        );
        assert!(
            (estimate.enemies[0].value - ours).abs() < 1e-9,
            "the enemy army is our army now"
        );
    }

    #[test]
    fn damage_and_kills_in_our_turn_are_enemy_losses() {
        let (mut state, us, them) = open_field();
        state
            .units
            .push(unit(1, UnitKind::Tank, us, Pos::new(2, 2)));
        state
            .units
            .push(unit(2, UnitKind::Tank, them, Pos::new(3, 2)));
        let mut memory = FogMemory::new();
        memory.update(&view(&state));

        state
            .units
            .get_mut(UnitId::new(2))
            .expect("the tank is in play")
            .hp = 40;
        memory.update(&view(&state));
        let tank = ruleset::profile(UnitKind::Tank).cost as f64;
        assert!((memory.enemies[0].losses - 0.6 * tank).abs() < 1e-9);

        state.units.retain(|unit| unit.id != UnitId::new(2));
        memory.update(&view(&state));
        assert!((memory.enemies[0].losses - tank).abs() < 1e-9);
        assert!(
            memory.tracks().is_empty(),
            "a destroyed unit is not a track"
        );
    }

    #[test]
    fn the_enemy_is_assumed_to_capture_as_we_do() {
        let mut state = arena(true, 1);
        state.units.retain(|_| false);
        let us = state
            .players
            .seat(&state.turn.active_player)
            .expect("the active player holds a seat");
        let them = state
            .players
            .seats()
            .map(|(seat, _)| seat)
            .find(|seat| *seat != us)
            .expect("the arena seats two players");
        let mut memory = FogMemory::new();
        memory.update(&view(&state));
        let start = memory.our_start_properties;
        assert!(start > 0, "the arena gives each seat properties");
        let seen = query::reify(&view(&state)).expect("the view reifies");
        let estimate = memory.estimate(&seen).expect("the memory has a view");
        assert_eq!(
            estimate.enemies[0].properties,
            f64::from(start),
            "on day 1 the enemy holds what we hold"
        );

        // We take two neutral cities. The enemy is assumed to do the same in
        // the fog.
        let cities: Vec<Pos> = state
            .board
            .iter()
            .filter(|(_, tile)| tile.terrain == Terrain::City && tile.owner == TileOwner::Neutral)
            .map(|(position, _)| position)
            .take(2)
            .collect();
        for position in &cities {
            state
                .board
                .get_mut(*position)
                .expect("the city is on the board")
                .owner = TileOwner::Owned(us);
        }
        state.turn.day = 2;
        memory.update(&view(&state));
        let seen = query::reify(&view(&state)).expect("the view reifies");
        let estimate = memory.estimate(&seen).expect("the memory has a view");
        assert_eq!(estimate.enemies[0].seat, them);
        assert_eq!(estimate.enemies[0].properties, f64::from(start + 2));
        let rate = state.settings.income_per_property as f64;
        assert_eq!(
            estimate.enemies[0].income,
            f64::from(start) * rate,
            "the enemy moves second, so one enemy turn is complete on day 2"
        );
    }
}
