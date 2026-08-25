//! What the board itself says, before anybody plays on it.
//!
//! Every other map in this crate is a map of the position: what the enemy can
//! take off a tile, what a play would light, where our capturers are. This one
//! is a map of the *geography*, and it answers the question the opening is
//! about — which of the thirty-odd properties on this board are mine to take,
//! and which of them are already the other side's.
//!
//! [`ContestMap`] is the first of them. It runs one multi-source arrival for
//! each side, seeded at the production the side holds, and labels each tile by
//! how many turns each side is from it. A property the enemy stands three
//! turns nearer to is one our infantry walks four turns to and then loses; a
//! property we reach first is one worth walking at. The capture field cannot
//! state that, because it measures the distance from us and nothing else, and
//! a decay tuned to say "not that far away" says the same thing about the
//! property behind the enemy headquarters as about the one behind ours.
//!
//! **The seeds are production, not units.** Each side is seeded at the
//! properties it holds that build ground units, and at its headquarters. A
//! capturer walks, so a map seeded at capturers moves with every play and is a
//! field rather than an analysis; production stands still, and a board's
//! contest is a fact about where the two sides build. It is also the reading
//! fog cannot take away: terrain and ownership are visible under fog, and the
//! units are not.
//!
//! The map is rebuilt only when what it reads moves, which is when a property
//! changes hands or the weather reprices the ground. Everything the search
//! reads is held beside the answer and compared, which is the same contract
//! [`crate::agents::Weights`]'s capture field keeps.

use awvm::commander;
use awvm::query::{self, Travel};
use awvm::ruleset::{self, MovementClass, TerrainTrait, UnitKind};
use awvm::semantic::{
    Concealment, Location, PlayerIdx, Pos, State, TileOwner, Unit, UnitAction, UnitId,
};

use crate::threat;

/// The most turns of deficit the map reports.
///
/// A property the enemy reaches this many turns before us is already theirs
/// for every purpose an evaluation has, and reading further only splits the
/// pull field into more searches than the answer is worth.
pub const MAX_DEFICIT: u16 = 3;

/// How many turns each side is from each tile, and what that makes of the
/// properties on it.
#[derive(Debug, Default)]
pub struct ContestMap {
    /// Turns for our side's production to reach each tile, `None` where no
    /// route exists at all.
    ours: Vec<Option<u16>>,
    /// The same for every side at war with us, taking the nearest of them.
    theirs: Vec<Option<u16>>,
    /// Everything the two searches were run from, so that a play which
    /// changed none of it reads the answer instead of searching again.
    built: Option<Built>,
    /// The searches' output, kept so that a rebuild does not allocate.
    points: Vec<Option<u16>>,
    seeds: Vec<Pos>,
}

/// One side's whole input to the search.
///
/// The entry costs are the derived table [`Travel::costs`] reads, so terrain,
/// weather, the seat's commander and its power are folded in already: a rule
/// added to any of them moves this table and throws the answer away, without
/// this file knowing the rule exists.
#[derive(Clone, Debug, PartialEq)]
struct Search {
    seat: PlayerIdx,
    allowance: u16,
    costs: Vec<Option<u16>>,
    seeds: Vec<Pos>,
}

#[derive(Clone, Debug, PartialEq)]
struct Built {
    ours: Search,
    theirs: Vec<Search>,
}

impl ContestMap {
    pub const fn new() -> Self {
        Self {
            ours: Vec::new(),
            theirs: Vec::new(),
            built: None,
            points: Vec::new(),
            seeds: Vec::new(),
        }
    }

    /// Whether a map has been built to read.
    pub fn is_built(&self) -> bool {
        !self.ours.is_empty()
    }

    /// Forget the map, so that a later reader knows there is none.
    pub fn forget(&mut self) {
        self.ours.clear();
        self.theirs.clear();
        self.built = None;
    }

    /// How many turns the enemy is ahead of us at this tile.
    ///
    /// Zero where we are level or ahead, and where there is no map to read.
    /// [`MAX_DEFICIT`] where the enemy can reach the tile and we cannot, which
    /// is the strongest statement this map makes.
    pub fn deficit(&self, cell: usize) -> u16 {
        let (Some(ours), Some(theirs)) = (
            self.ours.get(cell).copied().unwrap_or(None),
            self.theirs.get(cell).copied().unwrap_or(None),
        ) else {
            return match self.theirs.get(cell).copied().unwrap_or(None) {
                // Ground only they can stand on, which is as contested as
                // ground gets.
                Some(_) if self.is_built() => MAX_DEFICIT,
                _ => 0,
            };
        };
        ours.saturating_sub(theirs).min(MAX_DEFICIT)
    }

    /// Rebuild the map for the position in front of the agent.
    ///
    /// One search for each side, and neither of them is run while the answer
    /// in hand was taken from the same tables and the same seeds.
    pub fn build(&mut self, state: &State, seat: PlayerIdx) {
        let ours = match self.search_for(state, seat) {
            Some(search) => search,
            None => {
                self.forget();
                return;
            }
        };
        let theirs: Vec<Search> = state
            .players
            .seats()
            .map(|(other, _)| other)
            .filter(|other| threat::hostile(state, seat, *other))
            .filter_map(|other| self.search_for(state, other))
            .collect();
        if theirs.is_empty() {
            self.forget();
            return;
        }

        let built = Built {
            ours: ours.clone(),
            theirs: theirs.clone(),
        };
        if self.built.as_ref() == Some(&built) && self.is_built() {
            return;
        }

        let cells = state.board.dimensions().len();
        self.turns_to(state, &ours, cells);
        std::mem::swap(&mut self.ours, &mut self.points);

        self.theirs.clear();
        self.theirs.resize(cells, None);
        for search in &theirs {
            self.turns_to(state, search, cells);
            for (best, turns) in self.theirs.iter_mut().zip(self.points.iter().copied()) {
                *best = match (*best, turns) {
                    (Some(held), Some(found)) => Some(held.min(found)),
                    (held, found) => held.or(found),
                };
            }
        }
        self.built = Some(built);
    }

    /// Run one side's search into [`ContestMap::points`], in turns.
    fn turns_to(&mut self, state: &State, search: &Search, cells: usize) {
        let Some(mut travel) = Travel::open(state, search.seat) else {
            self.points.clear();
            self.points.resize(cells, None);
            return;
        };
        travel.points_to(
            MovementClass::Foot,
            search.allowance,
            search.seeds.iter().copied(),
            &mut self.points,
        );
        for entry in self.points.iter_mut() {
            *entry = entry.map(|points| query::Travel::turns(points, search.allowance));
        }
    }

    /// What one side's search reads: its entry costs, its allowance and the
    /// production it starts from.
    fn search_for(&mut self, state: &State, seat: PlayerIdx) -> Option<Search> {
        self.seeds.clear();
        for (position, tile) in state.board.iter() {
            if tile.owner != TileOwner::Owned(seat) {
                continue;
            }
            let produces = ruleset::terrain_has(tile.terrain, TerrainTrait::ProducesGround);
            let home = ruleset::terrain_has(tile.terrain, TerrainTrait::CaptureDefeatsOwner);
            if produces || home {
                self.seeds.push(position);
            }
        }
        if self.seeds.is_empty() {
            return None;
        }
        let mut travel = Travel::open(state, seat)?;
        Some(Search {
            seat,
            allowance: foot_allowance(state, seat),
            costs: travel.costs(MovementClass::Foot).to_vec(),
            seeds: std::mem::take(&mut self.seeds),
        })
    }
}

/// What a soldier of this seat spends in a turn.
///
/// A synthetic soldier rather than one off the board: the map is about the
/// board's geography and is read before either side has built anything, and a
/// side with no soldier on it walks at the same rate as one with a dozen. The
/// commander's own movement rule reaches it, because the operator does.
fn foot_allowance(state: &State, seat: PlayerIdx) -> u16 {
    let profile = ruleset::profile(UnitKind::Infantry);
    let unit = Unit {
        id: UnitId::new(1),
        kind: UnitKind::Infantry,
        owner: seat,
        hp: 100,
        fuel: profile.max_fuel,
        ammo: profile.max_ammo,
        action: UnitAction::Ready,
        concealment: Concealment::Exposed,
        location: Location::Board {
            position: Pos::new(0, 0),
        },
    };
    commander::effective_move(state, &unit, profile.movement, profile.domain)
        .min(u64::from(u16::MAX)) as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::amber_valley;
    use awvm::ruleset::{Terrain, WeatherKind};

    fn seats(state: &State) -> (PlayerIdx, PlayerIdx) {
        let mut seats = state.players.seats().map(|(seat, _)| seat);
        (
            seats.next().expect("a first seat"),
            seats.next().expect("a second seat"),
        )
    }

    /// The board's own geography, read off the opening position.
    #[test]
    fn each_side_is_nearer_the_property_beside_its_own_production() {
        let state = amber_valley(false, 1);
        let (first, second) = seats(&state);
        let dimensions = state.board.dimensions();

        let mut mine = ContestMap::new();
        mine.build(&state, first);
        let mut theirs = ContestMap::new();
        theirs.build(&state, second);
        assert!(mine.is_built() && theirs.is_built());

        // The map read from either seat is the same board, so a tile one side
        // is ahead at is a tile the other side is behind at.
        let mut ahead = 0;
        let mut behind = 0;
        for (position, tile) in state.board.iter() {
            if !ruleset::terrain_has(tile.terrain, TerrainTrait::Capturable) {
                continue;
            }
            let cell = dimensions.index(position).expect("a tile of this board");
            if mine.deficit(cell) == 0 && theirs.deficit(cell) > 0 {
                ahead += 1;
            }
            if mine.deficit(cell) > 0 && theirs.deficit(cell) == 0 {
                behind += 1;
            }
        }
        assert!(
            ahead > 0 && behind > 0,
            "a mirrored board holds properties for both sides: {ahead} ours, {behind} theirs"
        );
        assert_eq!(
            ahead, behind,
            "the board is a mirror, so the two halves must match"
        );
    }

    /// The properties beside our own headquarters are ours to take.
    #[test]
    fn the_ground_around_our_own_production_carries_no_deficit() {
        let state = amber_valley(false, 1);
        let (seat, _) = seats(&state);
        let dimensions = state.board.dimensions();
        let mut map = ContestMap::new();
        map.build(&state, seat);

        for (position, tile) in state.board.iter() {
            if tile.owner != TileOwner::Owned(seat) {
                continue;
            }
            if !ruleset::terrain_has(tile.terrain, TerrainTrait::CaptureDefeatsOwner) {
                continue;
            }
            let cell = dimensions.index(position).expect("a tile of this board");
            assert_eq!(
                map.deficit(cell),
                0,
                "our own headquarters at {position:?} is not contested"
            );
        }
    }

    /// The answer in hand is kept while nothing it reads has moved, and
    /// thrown away when something has.
    #[test]
    fn a_kept_map_is_thrown_away_when_a_property_changes_hands() {
        let state = amber_valley(false, 1);
        let (seat, other) = seats(&state);
        let mut map = ContestMap::new();
        map.build(&state, seat);
        let first = map.ours.clone();

        // The same position again changes nothing, and must answer the same.
        map.build(&state, seat);
        assert_eq!(
            map.ours, first,
            "a position that did not move moved the map"
        );

        // A base of theirs taken is production that now seeds our search.
        let mut taken = state.clone();
        let base = taken
            .board
            .iter()
            .find(|(_, tile)| {
                tile.owner == TileOwner::Owned(other)
                    && ruleset::terrain_has(tile.terrain, TerrainTrait::ProducesGround)
            })
            .map(|(position, _)| position)
            .expect("the board holds a base of theirs");
        taken
            .board
            .get_mut(base)
            .expect("the tile is on the board")
            .owner = TileOwner::Owned(seat);
        map.build(&taken, seat);
        assert_ne!(
            map.ours, first,
            "a base that changed hands did not move the map"
        );
    }

    /// A rule outside this crate reaches the key, because the key holds the
    /// table the rule is folded into.
    #[test]
    fn snow_throws_away_a_map_built_under_clear_skies() {
        let clear = amber_valley(false, 1);
        let (seat, _) = seats(&clear);
        let mut snowy = clear.clone();
        snowy.weather.kind = WeatherKind::Snow;

        let mut kept = ContestMap::new();
        kept.build(&clear, seat);
        let under_clear = kept.ours.clone();
        kept.build(&snowy, seat);

        let mut fresh = ContestMap::new();
        fresh.build(&snowy, seat);
        assert_eq!(kept.ours, fresh.ours, "a kept map survived the thaw");
        assert_ne!(
            under_clear, kept.ours,
            "snow reprices the board, so the map must move"
        );
    }

    /// Ground neither side can walk to belongs to neither of them.
    #[test]
    fn a_property_no_soldier_can_reach_is_nobody_s() {
        let mut state = amber_valley(false, 1);
        let (other, seat) = seats(&state);
        let dimensions = state.board.dimensions();
        // A city in the sea, beside nothing: no soldier of ours can walk to
        // it, and none of theirs can either, so it is not the enemy's ground
        // and carries no deficit.
        let island = state
            .board
            .iter()
            .find(|(_, tile)| tile.terrain == Terrain::Sea)
            .map(|(position, _)| position)
            .expect("the board holds sea");
        let tile = state
            .board
            .get_mut(island)
            .expect("the tile is on the board");
        tile.terrain = Terrain::City;
        tile.owner = TileOwner::Owned(other);

        let mut map = ContestMap::new();
        map.build(&state, seat);
        let cell = dimensions.index(island).expect("a tile of this board");
        assert_eq!(
            map.deficit(cell),
            0,
            "a tile neither side can reach is nobody's"
        );
    }
}
