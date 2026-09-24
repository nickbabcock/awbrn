//! Changing a map: what a brush puts down, and where symmetry repeats it.
//!
//! A map is edited as AWBW terrain, because that is what a map document holds.
//! The graphical terrain the client draws comes from [`AwbrnMap`], which reads
//! the neighbours of a tile, so this module never picks a sprite. It picks the
//! terrain, and lets the map decide how the terrain looks.
//!
//! Two rules do the work that a map maker would otherwise do by hand:
//!
//! * **Autotiling.** Roads, rivers, bridges, pipes, pipe seams and shoals come
//!   in one variant for each way they can join their neighbours. A brush names
//!   the kind, and [`MapEditor`] gives the tile and the four tiles around it the
//!   variant their neighbours ask for.
//! * **Symmetry.** A competitive map is the same board for every army on it, so
//!   an edit is repeated onto each image of the tile it landed on. The terrain
//!   is turned with the image, and a property or a unit changes to the army that
//!   image belongs to.

use crate::awbrn_map::AwbrnMap;
use crate::awbw_map::AwbwMap;
use crate::deployment::Deployment;
use crate::map_document::{AwbrnMapDocument, AwbrnMapMetadata};
use awbrn_types::{
    AwbwTerrain, BridgeType, Faction, FactionCode, GraphicalTerrain, MissileSiloStatus,
    PipeRubbleType, PipeSeamType, PipeType, PlayerFaction, Property, PropertyKind, RiverType,
    RoadType, ShoalType, Unit, VisualHp,
};
use awvm::semantic::{Dimensions, Pos};
use serde::{Deserialize, Serialize};

/// The most edits [`MapEditor`] keeps for undo.
///
/// Each step holds a whole board. A board is small — one byte for each tile
/// and a short list of units — so the depth is set by what a person can hold
/// in mind, not by what the browser can hold in memory.
pub const UNDO_DEPTH: usize = 64;

/// The four ways out of a tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    North,
    East,
    South,
    West,
}

impl Direction {
    const ALL: [Direction; 4] = [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
    ];

    /// The step this direction takes across the board.
    const fn step(self) -> (i16, i16) {
        match self {
            Direction::North => (0, -1),
            Direction::East => (1, 0),
            Direction::South => (0, 1),
            Direction::West => (-1, 0),
        }
    }

    const fn bit(self) -> u8 {
        match self {
            Direction::North => 1,
            Direction::East => 2,
            Direction::South => 4,
            Direction::West => 8,
        }
    }
}

/// Which sides of a tile a length of terrain reaches out to.
///
/// Every terrain that has variants is one of these sets, which is what lets a
/// turn or a mirror act on the set rather than on a list of variant names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Sides(u8);

impl Sides {
    const NONE: Sides = Sides(0);
    const NORTH: Sides = Sides(1);
    const EAST: Sides = Sides(2);
    const SOUTH: Sides = Sides(4);
    const WEST: Sides = Sides(8);
    const HORIZONTAL: Sides = Sides(2 | 8);
    const VERTICAL: Sides = Sides(1 | 4);

    const fn with(self, direction: Direction) -> Sides {
        Sides(self.0 | direction.bit())
    }

    const fn has(self, direction: Direction) -> bool {
        self.0 & direction.bit() != 0
    }

    /// The same set after a swap of two opposite sides.
    const fn swapped(self, first: Direction, second: Direction) -> Sides {
        let mut sides = self.0 & !(first.bit() | second.bit());
        if self.0 & first.bit() != 0 {
            sides |= second.bit();
        }
        if self.0 & second.bit() != 0 {
            sides |= first.bit();
        }
        Sides(sides)
    }
}

/// One of the eight ways a square board can be laid over itself.
///
/// A transpose reflects the board in its main diagonal, and the two flips
/// mirror it left to right and top to bottom. Every symmetry a map maker uses
/// is one of these, and the order below is the order they are applied in:
/// transpose first, then the flips.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Isometry {
    transpose: bool,
    flip_x: bool,
    flip_y: bool,
}

impl Isometry {
    /// The transform that changes nothing, which is what an original tile takes.
    pub const IDENTITY: Isometry = Isometry {
        transpose: false,
        flip_x: false,
        flip_y: false,
    };

    const fn new(transpose: bool, flip_x: bool, flip_y: bool) -> Isometry {
        Isometry {
            transpose,
            flip_x,
            flip_y,
        }
    }

    /// Whether a board of this shape stays the same shape under the transform.
    ///
    /// A transpose exchanges the two axes, so only a square board survives one.
    pub const fn fits(self, dimensions: Dimensions) -> bool {
        !self.transpose || dimensions.width() == dimensions.height()
    }

    /// Where `position` lands, or `None` when the shape refuses the transform.
    pub fn apply(self, position: Pos, dimensions: Dimensions) -> Option<Pos> {
        if !self.fits(dimensions) {
            return None;
        }

        let (mut x, mut y) = if self.transpose {
            (position.y, position.x)
        } else {
            (position.x, position.y)
        };

        if self.flip_x {
            x = dimensions.width().checked_sub(1)?.checked_sub(x)?;
        }
        if self.flip_y {
            y = dimensions.height().checked_sub(1)?.checked_sub(y)?;
        }

        let turned = Pos::new(x, y);
        dimensions.contains(turned).then_some(turned)
    }

    /// The same set of sides after the transform.
    fn turn(self, sides: Sides) -> Sides {
        let mut turned = sides;
        if self.transpose {
            // A transpose exchanges the axes, so east becomes south and north
            // becomes west.
            turned = turned.swapped(Direction::East, Direction::South);
            turned = turned.swapped(Direction::North, Direction::West);
        }
        if self.flip_x {
            turned = turned.swapped(Direction::East, Direction::West);
        }
        if self.flip_y {
            turned = turned.swapped(Direction::North, Direction::South);
        }
        turned
    }

    /// The same terrain, turned the way the tile it stands on is turned.
    ///
    /// Only the variant moves. A road stays a road and a base stays a base:
    /// which army owns it is decided by the roster, not by the transform.
    pub fn turn_terrain(self, terrain: AwbwTerrain) -> AwbwTerrain {
        match connection_of(terrain) {
            Some((kind, sides)) => kind.build(self.turn(sides)),
            None => terrain,
        }
    }
}

/// How an edit is repeated across the board.
///
/// Every mode except [`Symmetry::None`] is a group: the transforms it lists
/// close on each other, so painting an image of a tile paints the same set of
/// tiles as painting the tile itself. That is what stops a stroke near the axis
/// from drifting out of symmetry as it is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
#[serde(rename_all = "kebab-case")]
pub enum Symmetry {
    /// One edit, one tile.
    #[default]
    None,
    /// Mirrored across a vertical axis, so the left half faces the right.
    MirrorLeftRight,
    /// Mirrored across a horizontal axis, so the top half faces the bottom.
    MirrorTopBottom,
    /// Turned half a turn about the middle of the board.
    Rotate180,
    /// Turned a quarter turn about the middle. Square boards only.
    Rotate90,
    /// Mirrored in the diagonal that runs from the top left. Square boards only.
    MirrorDiagonal,
    /// Mirrored in the diagonal that runs from the top right. Square boards only.
    MirrorAntiDiagonal,
    /// Mirrored on both axes at once, which gives four quarters.
    QuadMirror,
}

impl Symmetry {
    /// Every mode, in the order the editor offers them.
    pub const ALL: [Symmetry; 8] = [
        Symmetry::None,
        Symmetry::MirrorLeftRight,
        Symmetry::MirrorTopBottom,
        Symmetry::Rotate180,
        Symmetry::Rotate90,
        Symmetry::MirrorDiagonal,
        Symmetry::MirrorAntiDiagonal,
        Symmetry::QuadMirror,
    ];

    /// The order a board is read for a fold it already holds, strongest first.
    ///
    /// A fold that shares an edit out to four armies says more than one that
    /// shares it out to two, so the two quarter folds are read first. The rest
    /// follow in the order the picker offers them, which is the order a map
    /// maker reaching for a fold thinks of them in. [`Symmetry::None`] is not
    /// listed: it is the answer when nothing else holds.
    pub const DETECTION_ORDER: [Symmetry; 7] = [
        Symmetry::Rotate90,
        Symmetry::QuadMirror,
        Symmetry::MirrorLeftRight,
        Symmetry::MirrorTopBottom,
        Symmetry::Rotate180,
        Symmetry::MirrorDiagonal,
        Symmetry::MirrorAntiDiagonal,
    ];

    /// The transforms this mode repeats an edit with, the identity first.
    pub fn isometries(self) -> &'static [Isometry] {
        const NONE: [Isometry; 1] = [Isometry::IDENTITY];
        const LEFT_RIGHT: [Isometry; 2] = [Isometry::IDENTITY, Isometry::new(false, true, false)];
        const TOP_BOTTOM: [Isometry; 2] = [Isometry::IDENTITY, Isometry::new(false, false, true)];
        const HALF_TURN: [Isometry; 2] = [Isometry::IDENTITY, Isometry::new(false, true, true)];
        const QUARTER_TURN: [Isometry; 4] = [
            Isometry::IDENTITY,
            Isometry::new(true, true, false),
            Isometry::new(false, true, true),
            Isometry::new(true, false, true),
        ];
        const DIAGONAL: [Isometry; 2] = [Isometry::IDENTITY, Isometry::new(true, false, false)];
        const ANTI_DIAGONAL: [Isometry; 2] = [Isometry::IDENTITY, Isometry::new(true, true, true)];
        const QUAD: [Isometry; 4] = [
            Isometry::IDENTITY,
            Isometry::new(false, true, false),
            Isometry::new(false, false, true),
            Isometry::new(false, true, true),
        ];

        match self {
            Symmetry::None => &NONE,
            Symmetry::MirrorLeftRight => &LEFT_RIGHT,
            Symmetry::MirrorTopBottom => &TOP_BOTTOM,
            Symmetry::Rotate180 => &HALF_TURN,
            Symmetry::Rotate90 => &QUARTER_TURN,
            Symmetry::MirrorDiagonal => &DIAGONAL,
            Symmetry::MirrorAntiDiagonal => &ANTI_DIAGONAL,
            Symmetry::QuadMirror => &QUAD,
        }
    }

    /// How many tiles one edit reaches, at most.
    pub fn order(self) -> usize {
        self.isometries().len()
    }

    /// The seat an army in `seat` lands in after `step` images of this fold.
    ///
    /// The seats have to follow the transforms: two mirrors of the same edit
    /// bring a tile back to itself, so they must bring the army back to itself
    /// as well, or a stroke drawn in one quarter of the board and the same
    /// stroke drawn in another would seat different armies.
    ///
    /// The quarter turn steps through its four seats in order, because turning
    /// twice is a half turn. The four quarters of [`Symmetry::QuadMirror`] do
    /// not: mirroring left to right twice changes nothing, so a seat there is
    /// two independent choices — which side, and which half — and a step
    /// exchanges each of them the fold names.
    fn compose(self, seat: usize, step: usize) -> usize {
        match self {
            Symmetry::QuadMirror => seat ^ step,
            _ => (seat + step) % self.order(),
        }
    }

    /// Whether a board of this shape can hold the mode.
    ///
    /// A quarter turn and both diagonals exchange the axes of the board, so
    /// they need a square one. The editor offers them anyway and says why they
    /// are out of reach, because the answer is to resize the map.
    pub fn fits(self, dimensions: Dimensions) -> bool {
        self.isometries()
            .iter()
            .all(|isometry| isometry.fits(dimensions))
    }
}

/// The armies an edit is shared out to, in seat order.
///
/// Painting an Orange Star headquarters under a half turn should put a Blue
/// Moon headquarters at the other end of the board, not a second Orange Star
/// one. The roster is what says which army each image belongs to: the army the
/// brush names takes its own seat, and each image takes the seat after it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ArmyRoster(Vec<PlayerFaction>);

impl ArmyRoster {
    /// The roster AWBW itself starts from: Orange Star, then Blue Moon, then
    /// Green Earth, then Yellow Comet.
    pub fn standard() -> ArmyRoster {
        ArmyRoster(vec![
            PlayerFaction::OrangeStar,
            PlayerFaction::BlueMoon,
            PlayerFaction::GreenEarth,
            PlayerFaction::YellowComet,
        ])
    }

    /// A roster of the named armies, with repeats removed.
    ///
    /// An empty roster is refused: every board has at least one army on it, and
    /// a roster with nothing in it would leave a painted headquarters ownerless.
    pub fn new(factions: impl IntoIterator<Item = PlayerFaction>) -> ArmyRoster {
        let mut seats: Vec<PlayerFaction> = Vec::new();
        for faction in factions {
            if !seats.contains(&faction) {
                seats.push(faction);
            }
        }
        if seats.is_empty() {
            return ArmyRoster::standard();
        }
        ArmyRoster(seats)
    }

    /// The armies a board already holds, in the order the game lists them.
    ///
    /// A map that comes out of the catalog names its own armies: the roster it
    /// is edited under is the one it was drawn with, not the one the standard
    /// four would guess. Painting as Orange Star on a board that seats Green
    /// Earth and Yellow Comet is an offer no map maker wants.
    ///
    /// A fold needs two armies to have anything to say, so a board that seats
    /// fewer takes the next armies the game lists.
    pub fn from_map(map: &AwbwMap) -> ArmyRoster {
        let mut seats: Vec<PlayerFaction> = Vec::new();
        let mut seat = |faction: Faction| {
            if let Faction::Player(player) = faction
                && !seats.contains(&player)
            {
                seats.push(player);
            }
        };

        for (_, terrain) in map.iter() {
            if let AwbwTerrain::Property(property) = terrain {
                seat(property.faction());
            }
        }
        for (_, deployment) in map.deployments().iter() {
            seat(Faction::Player(deployment.faction));
        }

        seats.sort();
        let mut roster = ArmyRoster(seats);
        roster.seat_at_least(2);
        roster
    }

    /// The armies in seat order.
    pub fn seats(&self) -> &[PlayerFaction] {
        &self.0
    }

    /// Adds armies, in the order the game lists them, until the roster holds
    /// `seats` of them.
    ///
    /// A fold of order four with two armies on the roster has nobody to give
    /// two of its four images to, and would hand them back to the army that
    /// drew the stroke. Growing the roster to the fold is what keeps a quarter
    /// turn meaning four armies.
    pub fn seat_at_least(&mut self, seats: usize) {
        for faction in (1..=u8::MAX).filter_map(PlayerFaction::from_id) {
            if self.0.len() >= seats {
                return;
            }
            if !self.0.contains(&faction) {
                self.0.push(faction);
            }
        }
    }

    /// How many armies the roster holds.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the roster holds no armies. It never does.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The army `step` seats after `faction`, inside its own group.
    ///
    /// A symmetry of order two pairs the armies two at a time, and one of order
    /// four takes them four at a time, so a roster of four armies mirrors as
    /// two pairs and turns as one set of four. That is what a map maker means
    /// by a four player mirror: two armies on each side, each facing its
    /// opposite number.
    ///
    /// An army the roster does not hold is left as it is: a map maker who
    /// paints an army that is not on the board means that army, and a mirror is
    /// not the place to correct them.
    pub fn rotate(&self, faction: PlayerFaction, symmetry: Symmetry, step: usize) -> PlayerFaction {
        let order = symmetry.order();
        let Some(seat) = self.0.iter().position(|held| *held == faction) else {
            return faction;
        };

        let group = seat - seat % order;
        let image = group + symmetry.compose(seat % order, step);
        self.0.get(image).copied().unwrap_or(faction)
    }
}

impl Default for ArmyRoster {
    fn default() -> Self {
        ArmyRoster::standard()
    }
}

/// What a stroke puts on a tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Brush {
    /// Terrain that has no variants: plain, mountain, wood, sea, reef, and the
    /// rest of the ground a map is built from. Named by its AWBW terrain id.
    Terrain {
        #[cfg_attr(feature = "typescript", tsify(type = "number"))]
        terrain: AwbwTerrain,
    },
    /// Terrain that joins its neighbours. The variant is decided by the board.
    Connecting { connection: Connection },
    /// A building, and the army that holds it. No army means neutral ground.
    Property {
        property: PropertyKind,
        #[cfg_attr(feature = "typescript", tsify(optional, type = "string"))]
        faction: Option<FactionCode>,
    },
    /// A unit the map places before the first turn.
    Unit {
        unit: Unit,
        #[cfg_attr(feature = "typescript", tsify(type = "string"))]
        faction: FactionCode,
        #[cfg_attr(feature = "typescript", tsify(type = "number"))]
        hp: VisualHp,
    },
    /// Plain ground, and no unit.
    Erase,
    /// The unit only. The ground under it is kept.
    EraseUnit,
}

/// Terrain that takes its shape from the tiles around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
#[serde(rename_all = "kebab-case")]
pub enum Connection {
    Road,
    River,
    Bridge,
    Pipe,
    PipeSeam,
    PipeRubble,
    Shoal,
}

impl Connection {
    /// The terrain of this kind that reaches out to `sides`.
    ///
    /// Roads and rivers have no variant for one side or for none, so a stub is
    /// drawn along the axis it points down, which is what AWBW itself draws.
    fn build(self, sides: Sides) -> AwbwTerrain {
        match self {
            Connection::Road => AwbwTerrain::Road(road_type(sides)),
            Connection::River => AwbwTerrain::River(river_type(sides)),
            Connection::Bridge => AwbwTerrain::Bridge(
                if sides.has(Direction::North) || sides.has(Direction::South) {
                    BridgeType::Vertical
                } else {
                    BridgeType::Horizontal
                },
            ),
            Connection::Pipe => AwbwTerrain::Pipe(pipe_type(sides)),
            Connection::PipeSeam => AwbwTerrain::PipeSeam(
                if sides.has(Direction::North) || sides.has(Direction::South) {
                    PipeSeamType::Vertical
                } else {
                    PipeSeamType::Horizontal
                },
            ),
            Connection::PipeRubble => AwbwTerrain::PipeRubble(
                if sides.has(Direction::North) || sides.has(Direction::South) {
                    PipeRubbleType::Vertical
                } else {
                    PipeRubbleType::Horizontal
                },
            ),
            Connection::Shoal => AwbwTerrain::Shoal(ShoalType::from_land(
                sides.has(Direction::North),
                sides.has(Direction::East),
                sides.has(Direction::South),
                sides.has(Direction::West),
            )),
        }
    }

    /// Whether terrain of this kind joins onto `neighbour`.
    ///
    /// The answer is the one AWBW draws: a road meets a road, a bridge, and a
    /// building, because a building has a road through it. A river meets a
    /// river and a bridge. A pipe meets a pipe, a seam, and its own rubble.
    fn joins(self, neighbour: AwbwTerrain) -> bool {
        match self {
            Connection::Road => matches!(
                neighbour,
                AwbwTerrain::Road(_) | AwbwTerrain::Bridge(_) | AwbwTerrain::Property(_)
            ),
            Connection::River => {
                matches!(neighbour, AwbwTerrain::River(_) | AwbwTerrain::Bridge(_))
            }
            // A bridge carries whatever crosses the water, so it joins the road
            // or the river at either end of it.
            Connection::Bridge => matches!(
                neighbour,
                AwbwTerrain::Bridge(_)
                    | AwbwTerrain::Road(_)
                    | AwbwTerrain::River(_)
                    | AwbwTerrain::Property(_)
            ),
            Connection::Pipe | Connection::PipeSeam | Connection::PipeRubble => matches!(
                neighbour,
                AwbwTerrain::Pipe(_)
                    | AwbwTerrain::PipeSeam(_)
                    | AwbwTerrain::PipeRubble(_)
                    | AwbwTerrain::Property(_)
            ),
            // A shoal takes its shape from the land it lies against, not from
            // the shoals beside it. The land is the land the client draws a
            // coast against, which is not the land a unit can walk on: a
            // bridge, a reef and another shoal are all water here.
            Connection::Shoal => neighbour.is_shore_land(),
        }
    }
}

fn road_type(sides: Sides) -> RoadType {
    match (
        sides.has(Direction::North),
        sides.has(Direction::East),
        sides.has(Direction::South),
        sides.has(Direction::West),
    ) {
        (true, true, true, true) => RoadType::Cross,
        (false, true, true, true) => RoadType::ESW,
        (true, false, true, true) => RoadType::SWN,
        (true, true, false, true) => RoadType::WNE,
        (true, true, true, false) => RoadType::NES,
        (false, true, true, false) => RoadType::ES,
        (false, false, true, true) => RoadType::SW,
        (true, false, false, true) => RoadType::WN,
        (true, true, false, false) => RoadType::NE,
        (true, false, true, false) => RoadType::Vertical,
        (false, true, false, true) => RoadType::Horizontal,
        (true, false, false, false) | (false, false, true, false) => RoadType::Vertical,
        _ => RoadType::Horizontal,
    }
}

fn river_type(sides: Sides) -> RiverType {
    match (
        sides.has(Direction::North),
        sides.has(Direction::East),
        sides.has(Direction::South),
        sides.has(Direction::West),
    ) {
        (true, true, true, true) => RiverType::Cross,
        (false, true, true, true) => RiverType::ESW,
        (true, false, true, true) => RiverType::SWN,
        (true, true, false, true) => RiverType::WNE,
        (true, true, true, false) => RiverType::NES,
        (false, true, true, false) => RiverType::ES,
        (false, false, true, true) => RiverType::SW,
        (true, false, false, true) => RiverType::WN,
        (true, true, false, false) => RiverType::NE,
        (true, false, true, false) => RiverType::Vertical,
        (false, true, false, true) => RiverType::Horizontal,
        (true, false, false, false) | (false, false, true, false) => RiverType::Vertical,
        _ => RiverType::Horizontal,
    }
}

/// The pipe that reaches out to `sides`.
///
/// A pipe has an end cap for one side and a corner for two, and nothing for
/// three or four: a pipe network in AWBW never branches. A tile that would
/// branch is drawn along the axis with the most of its run on it.
fn pipe_type(sides: Sides) -> PipeType {
    match (
        sides.has(Direction::North),
        sides.has(Direction::East),
        sides.has(Direction::South),
        sides.has(Direction::West),
    ) {
        (true, false, true, _) => PipeType::Vertical,
        (false, true, false, true) => PipeType::Horizontal,
        (true, true, false, false) => PipeType::NE,
        (false, true, true, false) => PipeType::ES,
        (false, false, true, true) => PipeType::SW,
        (true, false, false, true) => PipeType::WN,
        (true, false, false, false) => PipeType::NorthEnd,
        (false, true, false, false) => PipeType::EastEnd,
        (false, false, true, false) => PipeType::SouthEnd,
        (false, false, false, true) => PipeType::WestEnd,
        (true, true, true, true) | (true, true, true, false) => PipeType::Vertical,
        _ => PipeType::Horizontal,
    }
}

/// The connecting kind of a terrain, and the sides it reaches out to.
///
/// This is the inverse of [`Connection::build`], and it is what lets a mirror
/// turn a variant it was not given the kind of.
fn connection_of(terrain: AwbwTerrain) -> Option<(Connection, Sides)> {
    let sides = |north, east, south, west| {
        let mut sides = Sides::NONE;
        if north {
            sides = sides.with(Direction::North);
        }
        if east {
            sides = sides.with(Direction::East);
        }
        if south {
            sides = sides.with(Direction::South);
        }
        if west {
            sides = sides.with(Direction::West);
        }
        sides
    };

    match terrain {
        AwbwTerrain::Road(road) => Some((
            Connection::Road,
            match road {
                RoadType::Horizontal => Sides::HORIZONTAL,
                RoadType::Vertical => Sides::VERTICAL,
                RoadType::Cross => sides(true, true, true, true),
                RoadType::ES => sides(false, true, true, false),
                RoadType::SW => sides(false, false, true, true),
                RoadType::WN => sides(true, false, false, true),
                RoadType::NE => sides(true, true, false, false),
                RoadType::ESW => sides(false, true, true, true),
                RoadType::SWN => sides(true, false, true, true),
                RoadType::WNE => sides(true, true, false, true),
                RoadType::NES => sides(true, true, true, false),
            },
        )),
        AwbwTerrain::River(river) => Some((
            Connection::River,
            match river {
                RiverType::Horizontal => Sides::HORIZONTAL,
                RiverType::Vertical => Sides::VERTICAL,
                RiverType::Cross => sides(true, true, true, true),
                RiverType::ES => sides(false, true, true, false),
                RiverType::SW => sides(false, false, true, true),
                RiverType::WN => sides(true, false, false, true),
                RiverType::NE => sides(true, true, false, false),
                RiverType::ESW => sides(false, true, true, true),
                RiverType::SWN => sides(true, false, true, true),
                RiverType::WNE => sides(true, true, false, true),
                RiverType::NES => sides(true, true, true, false),
            },
        )),
        AwbwTerrain::Bridge(bridge) => Some((
            Connection::Bridge,
            match bridge {
                BridgeType::Horizontal => Sides::HORIZONTAL,
                BridgeType::Vertical => Sides::VERTICAL,
            },
        )),
        AwbwTerrain::Pipe(pipe) => Some((
            Connection::Pipe,
            match pipe {
                PipeType::Horizontal => Sides::HORIZONTAL,
                PipeType::Vertical => Sides::VERTICAL,
                PipeType::NE => sides(true, true, false, false),
                PipeType::ES => sides(false, true, true, false),
                PipeType::SW => sides(false, false, true, true),
                PipeType::WN => sides(true, false, false, true),
                PipeType::NorthEnd => Sides::NORTH,
                PipeType::EastEnd => Sides::EAST,
                PipeType::SouthEnd => Sides::SOUTH,
                PipeType::WestEnd => Sides::WEST,
            },
        )),
        AwbwTerrain::PipeSeam(seam) => Some((
            Connection::PipeSeam,
            match seam {
                PipeSeamType::Horizontal => Sides::HORIZONTAL,
                PipeSeamType::Vertical => Sides::VERTICAL,
            },
        )),
        AwbwTerrain::PipeRubble(rubble) => Some((
            Connection::PipeRubble,
            match rubble {
                PipeRubbleType::Horizontal => Sides::HORIZONTAL,
                PipeRubbleType::Vertical => Sides::VERTICAL,
            },
        )),
        AwbwTerrain::Shoal(shoal) => Some((
            Connection::Shoal,
            match shoal {
                ShoalType::Horizontal => Sides::SOUTH,
                ShoalType::HorizontalNorth => Sides::NORTH,
                ShoalType::Vertical => Sides::WEST,
                ShoalType::VerticalEast => Sides::EAST,
            },
        )),
        _ => None,
    }
}

/// The brush that draws `terrain`, which is not always the terrain itself.
///
/// A tile the board tuned to its neighbours reads back as the kind it was
/// drawn as, never as the corner piece it ended up: picking a road bend and
/// painting it somewhere else gives the road that place asks for. A building
/// reads back with the army that holds it, so picking an army's base and
/// painting it puts down that army's base and not the rail's.
fn brush_for_terrain(terrain: AwbwTerrain) -> Brush {
    if let Some((connection, _)) = connection_of(terrain) {
        return Brush::Connecting { connection };
    }

    match terrain {
        AwbwTerrain::Property(property) => Brush::Property {
            property: property.kind(),
            faction: match property.faction() {
                Faction::Player(faction) => Some(FactionCode::from(faction)),
                Faction::Neutral => None,
            },
        },
        _ => Brush::Terrain { terrain },
    }
}

/// Where a brush is filed in the palette.
///
/// The groups are the ones a map maker already works in: the ground armies
/// walk on, the water they cross, the ways they travel, the buildings they
/// take, and the units a map starts them with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
#[serde(rename_all = "kebab-case")]
pub enum PaletteGroup {
    Ground,
    Water,
    Ways,
    Property,
    Unit,
}

/// One cell of the palette: a brush, what it is called, and what it draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteEntry {
    pub brush: Brush,
    pub name: &'static str,
    pub group: PaletteGroup,
    /// The terrain the cell shows. A unit cell draws its own sprite instead.
    pub sample: Option<AwbwTerrain>,
    /// Defense stars, which is the one figure that decides between two tiles.
    pub defense: u8,
}

/// Every terrain a map can be built from, in the order the palette lists it.
///
/// The order is the order a map is drawn in: the ground first, then the water
/// it is cut by, then the ways across both, then the buildings. `owner` is the
/// army whose buildings are offered; without one the buildings are neutral.
pub fn terrain_palette(owner: Option<PlayerFaction>) -> Vec<PaletteEntry> {
    let mut palette = Vec::new();

    let mut ground = |terrain: AwbwTerrain, name: &'static str, group: PaletteGroup| {
        palette.push(PaletteEntry {
            brush: Brush::Terrain { terrain },
            name,
            group,
            sample: Some(terrain),
            defense: terrain.defense_stars(),
        });
    };

    ground(AwbwTerrain::Plain, "Plain", PaletteGroup::Ground);
    ground(AwbwTerrain::Wood, "Wood", PaletteGroup::Ground);
    ground(AwbwTerrain::Mountain, "Mountain", PaletteGroup::Ground);
    ground(
        AwbwTerrain::MissileSilo(MissileSiloStatus::Loaded),
        "Missile silo",
        PaletteGroup::Ground,
    );
    // "Teleporter" is the only name on the rail that will not set on one
    // line under a key. The short form is what the tile is called anyway.
    ground(AwbwTerrain::Teleporter, "Teleport", PaletteGroup::Ground);
    ground(AwbwTerrain::Sea, "Sea", PaletteGroup::Water);
    ground(AwbwTerrain::Reef, "Reef", PaletteGroup::Water);

    let mut connecting =
        |connection: Connection, name: &'static str, group: PaletteGroup, sample: AwbwTerrain| {
            palette.push(PaletteEntry {
                brush: Brush::Connecting { connection },
                name,
                group,
                sample: Some(sample),
                defense: sample.defense_stars(),
            });
        };

    connecting(
        Connection::Shoal,
        "Shoal",
        PaletteGroup::Water,
        AwbwTerrain::Shoal(ShoalType::Horizontal),
    );
    connecting(
        Connection::River,
        "River",
        PaletteGroup::Water,
        AwbwTerrain::River(RiverType::Horizontal),
    );
    connecting(
        Connection::Road,
        "Road",
        PaletteGroup::Ways,
        AwbwTerrain::Road(RoadType::Horizontal),
    );
    connecting(
        Connection::Bridge,
        "Bridge",
        PaletteGroup::Ways,
        AwbwTerrain::Bridge(BridgeType::Horizontal),
    );
    connecting(
        Connection::Pipe,
        "Pipe",
        PaletteGroup::Ways,
        AwbwTerrain::Pipe(PipeType::Horizontal),
    );
    connecting(
        Connection::PipeSeam,
        "Pipe seam",
        PaletteGroup::Ways,
        AwbwTerrain::PipeSeam(PipeSeamType::Horizontal),
    );
    connecting(
        Connection::PipeRubble,
        "Pipe rubble",
        PaletteGroup::Ways,
        AwbwTerrain::PipeRubble(PipeRubbleType::Horizontal),
    );

    for property in [
        PropertyKind::City,
        PropertyKind::Base,
        PropertyKind::Airport,
        PropertyKind::Port,
        PropertyKind::ComTower,
        PropertyKind::Lab,
        PropertyKind::HQ,
    ] {
        // A headquarters belongs to an army and to nobody else, so it is only
        // offered while an army is selected.
        if property == PropertyKind::HQ && owner.is_none() {
            continue;
        }
        let faction = owner.map_or(Faction::Neutral, Faction::Player);
        let sample = property_terrain(property, faction);
        palette.push(PaletteEntry {
            brush: Brush::Property {
                property,
                faction: owner.map(FactionCode::from),
            },
            name: property.name(),
            group: PaletteGroup::Property,
            sample: Some(sample),
            defense: sample.defense_stars(),
        });
    }

    palette
}

/// Every unit a map can start an army with, at full health.
pub fn unit_palette(faction: PlayerFaction) -> Vec<PaletteEntry> {
    use awbrn_types::UnitExt;

    let mut palette: Vec<PaletteEntry> = Unit::ALL
        .iter()
        .map(|unit| PaletteEntry {
            brush: Brush::Unit {
                unit: *unit,
                faction: FactionCode::from(faction),
                hp: VisualHp::new(10),
            },
            name: unit.name(),
            group: PaletteGroup::Unit,
            sample: None,
            defense: 0,
        })
        .collect();
    palette.sort_by_key(|entry| entry.name);
    palette
}

/// One tile the board draws differently after an edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TerrainChange {
    #[serde(with = "crate::xy")]
    pub position: Pos,
    pub terrain: GraphicalTerrain,
}

/// One tile whose unit arrived, left, or changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct UnitChange {
    #[serde(with = "crate::xy")]
    pub position: Pos,
    /// The unit that now stands there, or `None` when the tile is empty.
    pub deployment: Option<DeploymentView>,
}

/// A deployment as the browser reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentView {
    pub unit: Unit,
    pub faction: PlayerFaction,
    pub hp: u8,
}

impl From<Deployment> for DeploymentView {
    fn from(deployment: Deployment) -> Self {
        DeploymentView {
            unit: deployment.unit,
            faction: deployment.faction,
            hp: deployment.hp.get(),
        }
    }
}

/// What an edit changed, and nothing more.
///
/// The board redraws from this rather than from the whole map, so a stroke
/// costs the tiles it touched. An edit that changes nothing reports nothing,
/// which is what lets a stroke run over the same tile without work.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardChanges {
    pub terrain: Vec<TerrainChange>,
    pub units: Vec<UnitChange>,
    /// Set when the board changed shape, and the client must rebuild it.
    pub dimensions: Option<[u8; 2]>,
}

impl BoardChanges {
    pub fn is_empty(&self) -> bool {
        self.terrain.is_empty() && self.units.is_empty() && self.dimensions.is_none()
    }
}

/// Where the old board sits inside a new one when the map is resized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
#[serde(rename_all = "kebab-case")]
pub enum ResizeAnchor {
    #[default]
    TopLeft,
    Center,
    BottomRight,
}

/// A map that is open for editing.
///
/// The AWBW terrain is what is edited and what is saved. The graphical map
/// beside it is what the client draws, and it is rebuilt after each edit so
/// that the changes reported are the tiles that really look different, not the
/// tiles that were written to.
#[derive(Debug, Clone)]
pub struct MapEditor {
    map: AwbwMap,
    drawn: AwbrnMap,
    symmetry: Symmetry,
    roster: ArmyRoster,
    undo: Vec<AwbwMap>,
    redo: Vec<AwbwMap>,
    /// The board as the stroke in flight found it, for one undo step.
    stroke: Option<AwbwMap>,
}

impl MapEditor {
    /// Opens a map for editing, under the fold the board was drawn with.
    ///
    /// A map maker who opens a mirrored map means to keep it mirrored, and the
    /// board itself is the only record of the fold: a map document holds
    /// terrain, not the mode it was drawn under. So the mode is read back off
    /// the board rather than started at [`Symmetry::None`], which would let the
    /// first stroke break a symmetry it took a week to draw.
    pub fn open(map: AwbwMap) -> MapEditor {
        let drawn = AwbrnMap::from_map(&map);
        let roster = ArmyRoster::from_map(&map);
        let mut editor = MapEditor {
            map,
            drawn,
            symmetry: Symmetry::None,
            roster,
            undo: Vec::new(),
            redo: Vec::new(),
            stroke: None,
        };
        editor.symmetry = editor.detect_symmetry();
        editor.roster.seat_at_least(editor.symmetry.order());
        editor
    }

    /// Opens a new board of plain ground.
    pub fn blank(dimensions: Dimensions) -> MapEditor {
        MapEditor::open(AwbwMap::new(dimensions, AwbwTerrain::Plain))
    }

    /// The map as it now stands.
    pub fn map(&self) -> &AwbwMap {
        &self.map
    }

    /// The map as the client draws it.
    pub fn drawn(&self) -> &AwbrnMap {
        &self.drawn
    }

    pub fn dimensions(&self) -> Dimensions {
        self.map.dimensions()
    }

    pub fn symmetry(&self) -> Symmetry {
        self.symmetry
    }

    /// Every tile a stroke on `position` would reach, that tile first.
    ///
    /// This is the walk [`MapEditor::paint`] makes, without the paint. A
    /// cursor drawn on these tiles therefore cannot disagree with the stroke
    /// that follows it, which is the whole point of asking the editor rather
    /// than working the transforms out a second time on the screen.
    ///
    /// A tile on the fold's own axis is its own image. It is listed once,
    /// because two cursors on one tile is a brighter cursor and not a second
    /// place the stroke lands.
    pub fn images(&self, position: Pos) -> Vec<Pos> {
        let dimensions = self.dimensions();
        let mut images = Vec::with_capacity(self.symmetry.order());
        for isometry in self.symmetry.isometries() {
            let Some(target) = isometry.apply(position, dimensions) else {
                continue;
            };
            if !images.contains(&target) {
                images.push(target);
            }
        }
        images
    }

    /// Sets the mode later edits are repeated with.
    ///
    /// The board is left as it is. A mode is a rule for what happens next, and
    /// a map maker who changes it has not asked for the board to be rebuilt.
    /// A mode the shape refuses is not taken.
    pub fn set_symmetry(&mut self, symmetry: Symmetry) -> bool {
        if !symmetry.fits(self.dimensions()) {
            return false;
        }
        self.symmetry = symmetry;
        self.roster.seat_at_least(symmetry.order());
        true
    }

    /// The strongest fold the terrain already reads the same under.
    ///
    /// Several modes can hold at once, so the modes are tried in order of how
    /// much they say. A board with one terrain kind everywhere is evidence for
    /// none of them, so it is left free. Units and property owners do not set
    /// the fold.
    pub fn detect_symmetry(&self) -> Symmetry {
        if self.board_is_blank() {
            return Symmetry::None;
        }

        Symmetry::DETECTION_ORDER
            .into_iter()
            .find(|symmetry| symmetry.fits(self.dimensions()) && self.folds_exactly(*symmetry))
            .unwrap_or(Symmetry::None)
    }

    /// Whether every terrain tile is the image the fold asks for.
    fn folds_exactly(&self, symmetry: Symmetry) -> bool {
        let dimensions = self.dimensions();
        if symmetry == Symmetry::None {
            return true;
        }

        for (position, terrain) in self.map.iter() {
            for isometry in symmetry.isometries() {
                let Some(image) = isometry.apply(position, dimensions) else {
                    return false;
                };
                let Some(found) = self.map.terrain_at(image) else {
                    return false;
                };
                if !same_fold_terrain(found, isometry.turn_terrain(terrain)) {
                    return false;
                }
            }
        }
        true
    }

    /// Whether the ground reads the same under `symmetry`.
    ///
    /// A convenience over [`Self::asymmetries`] for the callers that only need
    /// the answer and not the tiles.
    pub fn is_symmetric(&self, symmetry: Symmetry) -> bool {
        self.asymmetries(symmetry).is_empty()
    }

    /// Every tile whose ground does not read the same under `symmetry`.
    ///
    /// The result is in board order. It lets the screen show the number of
    /// uneven tiles and the first one. Units and property owners are ignored.
    /// The readout also allows map makers to place buildings for a deliberate
    /// first-turn advantage.
    pub fn asymmetries(&self, symmetry: Symmetry) -> Vec<Pos> {
        let mut found = Vec::new();
        if symmetry == Symmetry::None {
            return found;
        }
        let dimensions = self.dimensions();

        for (position, terrain) in self.map.iter() {
            if !self.folds_leniently(position, terrain, symmetry, dimensions) {
                found.push(position);
            }
        }
        found
    }

    /// Whether one tile keeps the fold's promise about the ground.
    fn folds_leniently(
        &self,
        position: Pos,
        terrain: AwbwTerrain,
        symmetry: Symmetry,
        dimensions: Dimensions,
    ) -> bool {
        for isometry in symmetry.isometries() {
            let Some(image) = isometry.apply(position, dimensions) else {
                return false;
            };
            let Some(found) = self.map.terrain_at(image) else {
                return false;
            };
            if same_fold_terrain(found, isometry.turn_terrain(terrain)) {
                continue;
            }

            match (terrain, found) {
                // Buildings and their owners can differ by design.
                (AwbwTerrain::Property(_), _) | (_, AwbwTerrain::Property(_)) => continue,
                _ => return false,
            }
        }
        true
    }

    /// Whether one fold-relevant terrain kind fills the board.
    fn board_is_blank(&self) -> bool {
        let mut tiles = self.map.iter().map(|(_, terrain)| terrain);
        let Some(first) = tiles.next() else {
            return true;
        };
        tiles.all(|terrain| same_fold_terrain(terrain, first))
    }

    pub fn roster(&self) -> &ArmyRoster {
        &self.roster
    }

    pub fn set_roster(&mut self, roster: ArmyRoster) {
        self.roster = roster;
        self.roster.seat_at_least(self.symmetry.order());
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Marks the start of a stroke, so that the whole stroke is one undo step.
    ///
    /// A drag across twenty tiles is one act to the person who drew it. The
    /// board is kept as it was found here, and committed when the stroke ends.
    pub fn begin_stroke(&mut self) {
        if self.stroke.is_none() {
            self.stroke = Some(self.map.clone());
        }
    }

    /// Marks the end of a stroke, and records it for undo if it changed the map.
    pub fn end_stroke(&mut self) {
        let Some(before) = self.stroke.take() else {
            return;
        };
        if before == self.map {
            return;
        }
        self.record(before);
    }

    /// Steps one edit back, and reports what the board must redraw.
    pub fn undo(&mut self) -> BoardChanges {
        self.stroke = None;
        let Some(previous) = self.undo.pop() else {
            return BoardChanges::default();
        };
        let current = std::mem::replace(&mut self.map, previous);
        self.redo.push(current);
        self.redraw()
    }

    /// Steps one undone edit forward again.
    pub fn redo(&mut self) -> BoardChanges {
        self.stroke = None;
        let Some(next) = self.redo.pop() else {
            return BoardChanges::default();
        };
        let current = std::mem::replace(&mut self.map, next);
        self.undo.push(current);
        self.redraw()
    }

    /// The brush that would draw what is already on `position`.
    ///
    /// This is the eyedropper. A map maker who wants more of a tile they can
    /// see should not have to work out which key on the rail made it: the
    /// board holds the answer, and reading it back is cheaper than finding
    /// the tile again among fifty.
    ///
    /// What comes back is the brush, not the tile. A road picked off the board
    /// returns [`Brush::Connecting`] rather than the exact corner piece it was
    /// drawn as, so the picked road keeps joining its neighbours. Picking then
    /// painting on the same tile therefore always leaves the board as it was.
    ///
    /// A tile with a unit on it holds two answers. `ground_only` asks for the
    /// one underneath, which is the only way to reach ground a unit is
    /// standing on.
    pub fn brush_at(&self, position: Pos, ground_only: bool) -> Option<Brush> {
        if !ground_only && let Some(deployment) = self.map.deployments().get(position) {
            return Some(Brush::Unit {
                unit: deployment.unit,
                faction: FactionCode::from(deployment.faction),
                hp: deployment.hp,
            });
        }

        self.map.terrain_at(position).map(brush_for_terrain)
    }

    /// Puts `brush` on `position` and on every image symmetry gives it.
    pub fn paint(&mut self, position: Pos, brush: Brush) -> BoardChanges {
        let single = self.stroke.is_none();
        if single {
            self.begin_stroke();
        }

        for (step, isometry) in self.symmetry.isometries().iter().enumerate() {
            let Some(target) = isometry.apply(position, self.dimensions()) else {
                continue;
            };
            self.apply_brush(target, brush, *isometry, step);
        }

        // The tiles around each edit are retuned, because a road that arrives
        // beside a road changes both of them.
        self.retune_around(position);

        if single {
            self.end_stroke();
        }
        self.redraw()
    }

    /// Fills the whole board with one terrain, and clears every unit.
    pub fn fill(&mut self, terrain: AwbwTerrain) -> BoardChanges {
        let before = self.map.clone();
        let mut filled = AwbwMap::new(self.dimensions(), terrain);
        std::mem::swap(&mut self.map, &mut filled);
        self.record(before);
        self.redraw()
    }

    /// Changes the shape of the board, keeping what still fits inside it.
    ///
    /// Tiles the new board gains are plain ground, and units that fall outside
    /// it are dropped. A mode the new shape refuses is stepped down to no
    /// symmetry rather than left pointing at a board it cannot describe.
    pub fn resize(&mut self, dimensions: Dimensions, anchor: ResizeAnchor) -> BoardChanges {
        if dimensions == self.dimensions() {
            return BoardChanges::default();
        }

        let before = self.map.clone();
        let offset = resize_offset(self.dimensions(), dimensions, anchor);
        let mut resized = AwbwMap::new(dimensions, AwbwTerrain::Plain);

        for (position, terrain) in self.map.iter() {
            let Some(target) = shift(position, offset) else {
                continue;
            };
            if let Some(cell) = resized.terrain_at_mut(target) {
                *cell = terrain;
            }
        }
        for (position, deployment) in self.map.deployments().iter() {
            let Some(target) = shift(position, offset) else {
                continue;
            };
            let _ = resized.deploy(target, *deployment);
        }

        self.map = resized;
        self.record(before);
        if !self.symmetry.fits(dimensions) {
            self.symmetry = Symmetry::None;
        }

        let mut changes = self.redraw();
        changes.dimensions = Some([dimensions.width(), dimensions.height()]);
        changes
    }

    /// The map document for this board, under the metadata it is saved with.
    pub fn document(&self, metadata: AwbrnMapMetadata) -> AwbrnMapDocument {
        AwbrnMapDocument::from_awbw_map(&self.map, metadata)
    }

    /// Puts one brush on one tile, under the transform of its image.
    fn apply_brush(&mut self, position: Pos, brush: Brush, isometry: Isometry, step: usize) {
        match brush {
            Brush::Terrain { terrain } => {
                self.set_terrain(position, isometry.turn_terrain(terrain));
            }
            Brush::Connecting { connection } => {
                let sides = self.sides_at(position, connection);
                self.set_terrain(position, connection.build(sides));
            }
            Brush::Property { property, faction } => {
                // Neutral ground stays neutral however the board is folded.
                let owner = faction.map_or(Faction::Neutral, |player| {
                    Faction::Player(self.roster.rotate(player.faction(), self.symmetry, step))
                });
                self.set_terrain(position, property_terrain(property, owner));
            }
            Brush::Unit { unit, faction, hp } => {
                let owner = self.roster.rotate(faction.faction(), self.symmetry, step);
                self.map.deployments_mut().remove(position);
                let _ = self.map.deploy(
                    position,
                    Deployment {
                        unit,
                        hp,
                        faction: owner,
                    },
                );
            }
            Brush::Erase => {
                self.set_terrain(position, AwbwTerrain::Plain);
                self.map.deployments_mut().remove(position);
            }
            Brush::EraseUnit => {
                self.map.deployments_mut().remove(position);
            }
        }
    }

    fn set_terrain(&mut self, position: Pos, terrain: AwbwTerrain) {
        if let Some(cell) = self.map.terrain_at_mut(position) {
            *cell = terrain;
        }
    }

    /// Which sides a length of connecting terrain at `position` reaches out to.
    fn sides_at(&self, position: Pos, connection: Connection) -> Sides {
        let mut sides = Sides::NONE;
        for direction in Direction::ALL {
            let (dx, dy) = direction.step();
            let Some(neighbour) = position
                .offset(dx, dy)
                .and_then(|next| self.map.terrain_at(next))
            else {
                continue;
            };
            if connection.joins(neighbour) {
                sides = sides.with(direction);
            }
        }
        sides
    }

    /// Gives every image of `position`, and the tiles beside them, the variant
    /// their neighbours now ask for.
    ///
    /// Only connecting terrain moves. Everything else was decided by the brush.
    fn retune_around(&mut self, position: Pos) {
        let dimensions = self.dimensions();
        let mut retune: Vec<Pos> = Vec::new();

        for isometry in self.symmetry.isometries() {
            let Some(target) = isometry.apply(position, dimensions) else {
                continue;
            };
            for neighbour in std::iter::once(target).chain(
                Direction::ALL
                    .iter()
                    .filter_map(|direction| {
                        let (dx, dy) = direction.step();
                        target.offset(dx, dy)
                    })
                    .filter(|next| dimensions.contains(*next)),
            ) {
                if !retune.contains(&neighbour) {
                    retune.push(neighbour);
                }
            }
        }

        for tile in retune {
            let Some((connection, _)) = self.map.terrain_at(tile).and_then(connection_of) else {
                continue;
            };
            let sides = self.sides_at(tile, connection);
            self.set_terrain(tile, connection.build(sides));
        }
    }

    /// Records one step of undo, and drops the redo branch it grew from.
    fn record(&mut self, before: AwbwMap) {
        self.undo.push(before);
        if self.undo.len() > UNDO_DEPTH {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Redraws the graphical map, and reports the tiles that changed.
    fn redraw(&mut self) -> BoardChanges {
        let drawn = AwbrnMap::from_map(&self.map);
        let mut changes = BoardChanges::default();

        let same_shape = drawn.dimensions() == self.drawn.dimensions();
        for (position, terrain) in drawn.iter() {
            let before = same_shape
                .then(|| self.drawn.terrain_at(position))
                .flatten();
            if before != Some(terrain) {
                changes.terrain.push(TerrainChange { position, terrain });
            }
        }

        let previous = self.drawn.deployments();
        let current = drawn.deployments();
        for (position, deployment) in current.iter() {
            let before = same_shape.then(|| previous.get(position)).flatten();
            if before != Some(deployment) {
                changes.units.push(UnitChange {
                    position,
                    deployment: Some(DeploymentView::from(*deployment)),
                });
            }
        }
        for (position, _) in previous.iter() {
            if current.get(position).is_none() {
                changes.units.push(UnitChange {
                    position,
                    deployment: None,
                });
            }
        }

        self.drawn = drawn;
        changes
    }
}

/// Whether two terrain tiles match for a fold.
///
/// A property kind is terrain. Its owner is a map maker's choice, so the fold
/// ignores it.
fn same_fold_terrain(left: AwbwTerrain, right: AwbwTerrain) -> bool {
    match (left, right) {
        (AwbwTerrain::Property(left), AwbwTerrain::Property(right)) => left.kind() == right.kind(),
        _ => left == right,
    }
}

/// The building of this kind that `faction` holds.
///
/// A headquarters can never be neutral, so a neutral one is refused and a city
/// is put down in its place: a neutral headquarters is not a thing a map can
/// hold, and a silently wrong owner would be worse than the nearest building.
fn property_terrain(property: PropertyKind, faction: Faction) -> AwbwTerrain {
    let building = match (property, faction) {
        (PropertyKind::HQ, Faction::Player(player)) => Property::HQ(player),
        (PropertyKind::HQ, Faction::Neutral) => Property::City(Faction::Neutral),
        (PropertyKind::Airport, owner) => Property::Airport(owner),
        (PropertyKind::Base, owner) => Property::Base(owner),
        (PropertyKind::City, owner) => Property::City(owner),
        (PropertyKind::ComTower, owner) => Property::ComTower(owner),
        (PropertyKind::Lab, owner) => Property::Lab(owner),
        (PropertyKind::Port, owner) => Property::Port(owner),
    };
    AwbwTerrain::Property(building)
}

/// Where the top left of the old board sits on the new one.
fn resize_offset(from: Dimensions, to: Dimensions, anchor: ResizeAnchor) -> (i16, i16) {
    let dx = i16::from(to.width()) - i16::from(from.width());
    let dy = i16::from(to.height()) - i16::from(from.height());
    match anchor {
        ResizeAnchor::TopLeft => (0, 0),
        ResizeAnchor::Center => (dx / 2, dy / 2),
        ResizeAnchor::BottomRight => (dx, dy),
    }
}

fn shift(position: Pos, offset: (i16, i16)) -> Option<Pos> {
    position.offset(offset.0, offset.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor(width: u8, height: u8) -> MapEditor {
        MapEditor::blank(Dimensions::new(width, height))
    }

    fn terrain(editor: &MapEditor, x: u8, y: u8) -> AwbwTerrain {
        editor
            .map()
            .terrain_at(Pos::new(x, y))
            .expect("on the board")
    }

    /// A board drawn under `symmetry`, reopened the way the editor opens a map
    /// that comes out of the catalog.
    fn reopened(width: u8, height: u8, symmetry: Symmetry) -> MapEditor {
        let mut drawing = editor(width, height);
        assert!(drawing.set_symmetry(symmetry));
        drawing.paint(
            Pos::new(1, 2),
            Brush::Terrain {
                terrain: AwbwTerrain::Mountain,
            },
        );
        drawing.paint(
            Pos::new(0, 0),
            Brush::Property {
                property: PropertyKind::HQ,
                faction: Some(FactionCode::new(PlayerFaction::OrangeStar)),
            },
        );
        MapEditor::open(drawing.map().clone())
    }

    /// Every way three kinds of neighbour can stand around one tile.
    ///
    /// Land, open water and more shoal are the three answers the shore rule
    /// gives, so the four sides make eighty-one boards.
    fn shore_neighbours() -> impl Iterator<Item = [AwbwTerrain; 4]> {
        let kinds = [
            AwbwTerrain::Plain,
            AwbwTerrain::Sea,
            AwbwTerrain::Shoal(ShoalType::Horizontal),
        ];
        kinds.into_iter().flat_map(move |north| {
            kinds.into_iter().flat_map(move |east| {
                kinds.into_iter().flat_map(move |south| {
                    kinds
                        .into_iter()
                        .map(move |west| [north, east, south, west])
                })
            })
        })
    }

    #[test]
    fn a_painted_shoal_records_the_land_the_client_draws_it_against() {
        let centre = Pos::new(1, 1);
        let around = [
            Pos::new(1, 0),
            Pos::new(2, 1),
            Pos::new(1, 2),
            Pos::new(0, 1),
        ];

        for neighbours in shore_neighbours() {
            let mut drawing = editor(3, 3);
            for (position, terrain) in around.iter().zip(neighbours) {
                drawing.paint(*position, Brush::Terrain { terrain });
            }
            drawing.paint(
                centre,
                Brush::Connecting {
                    connection: Connection::Shoal,
                },
            );

            let recorded = terrain(&drawing, 1, 1);
            let drawn = AwbrnMap::shoal_direction(drawing.map(), centre);
            assert_eq!(
                recorded,
                AwbwTerrain::Shoal(ShoalType::from_direction(drawn)),
                "a shoal with {neighbours:?} around it is drawn as {drawn:?}",
            );
        }
    }

    #[test]
    fn the_eyedropper_reads_a_tile_back_as_the_brush_that_drew_it() {
        let mut drawing = editor(5, 5);
        drawing.paint(
            Pos::new(2, 2),
            Brush::Terrain {
                terrain: AwbwTerrain::Mountain,
            },
        );

        assert_eq!(
            drawing.brush_at(Pos::new(2, 2), false),
            Some(Brush::Terrain {
                terrain: AwbwTerrain::Mountain
            })
        );
    }

    #[test]
    fn a_road_is_picked_up_as_a_road_rather_than_as_the_corner_it_became() {
        let mut drawing = editor(5, 5);
        for position in [Pos::new(1, 2), Pos::new(2, 2), Pos::new(2, 3)] {
            drawing.paint(
                position,
                Brush::Connecting {
                    connection: Connection::Road,
                },
            );
        }

        // The middle tile is a bend by now, and a bend is not a brush.
        assert!(matches!(
            terrain(&drawing, 2, 2),
            AwbwTerrain::Road(RoadType::WN | RoadType::NE | RoadType::SW | RoadType::ES)
        ));
        assert_eq!(
            drawing.brush_at(Pos::new(2, 2), false),
            Some(Brush::Connecting {
                connection: Connection::Road
            })
        );
    }

    #[test]
    fn the_images_of_a_tile_are_the_tiles_a_stroke_on_it_reaches() {
        let mut drawing = editor(5, 5);
        drawing.set_symmetry(Symmetry::Rotate180);

        assert_eq!(
            drawing.images(Pos::new(0, 1)),
            vec![Pos::new(0, 1), Pos::new(4, 3)]
        );
    }

    #[test]
    fn a_free_board_gives_a_tile_no_image_but_itself() {
        let drawing = editor(5, 5);

        assert_eq!(drawing.images(Pos::new(1, 2)), vec![Pos::new(1, 2)]);
    }

    #[test]
    fn a_tile_on_the_fold_is_its_own_image_and_is_listed_once() {
        let mut drawing = editor(5, 5);
        drawing.set_symmetry(Symmetry::MirrorLeftRight);

        assert_eq!(drawing.images(Pos::new(2, 3)), vec![Pos::new(2, 3)]);
    }

    #[test]
    fn the_images_are_the_tiles_the_same_stroke_paints() {
        let mut drawing = editor(5, 5);
        drawing.set_symmetry(Symmetry::Rotate90);

        let images = drawing.images(Pos::new(0, 1));
        drawing.paint(
            Pos::new(0, 1),
            Brush::Terrain {
                terrain: AwbwTerrain::Mountain,
            },
        );

        assert_eq!(images.len(), 4);
        for image in images {
            assert_eq!(terrain(&drawing, image.x, image.y), AwbwTerrain::Mountain);
        }
    }

    #[test]
    fn a_building_is_picked_up_with_the_army_that_holds_it() {
        let mut drawing = editor(5, 5);
        drawing.paint(
            Pos::new(1, 1),
            Brush::Property {
                property: PropertyKind::Base,
                faction: Some(FactionCode::new(PlayerFaction::BlueMoon)),
            },
        );

        assert_eq!(
            drawing.brush_at(Pos::new(1, 1), false),
            Some(Brush::Property {
                property: PropertyKind::Base,
                faction: Some(FactionCode::new(PlayerFaction::BlueMoon)),
            })
        );
    }

    #[test]
    fn a_neutral_building_is_picked_up_as_nobody_s() {
        let mut drawing = editor(5, 5);
        drawing.paint(
            Pos::new(1, 1),
            Brush::Property {
                property: PropertyKind::City,
                faction: None,
            },
        );

        assert_eq!(
            drawing.brush_at(Pos::new(1, 1), false),
            Some(Brush::Property {
                property: PropertyKind::City,
                faction: None,
            })
        );
    }

    #[test]
    fn a_tile_with_a_unit_on_it_gives_the_unit_first_and_the_ground_on_request() {
        let mut drawing = editor(5, 5);
        drawing.paint(
            Pos::new(3, 1),
            Brush::Terrain {
                terrain: AwbwTerrain::Wood,
            },
        );
        drawing.paint(
            Pos::new(3, 1),
            Brush::Unit {
                unit: Unit::Infantry,
                faction: FactionCode::new(PlayerFaction::OrangeStar),
                hp: VisualHp::new(10),
            },
        );

        assert_eq!(
            drawing.brush_at(Pos::new(3, 1), false),
            Some(Brush::Unit {
                unit: Unit::Infantry,
                faction: FactionCode::new(PlayerFaction::OrangeStar),
                hp: VisualHp::new(10),
            })
        );
        assert_eq!(
            drawing.brush_at(Pos::new(3, 1), true),
            Some(Brush::Terrain {
                terrain: AwbwTerrain::Wood
            })
        );
    }

    #[test]
    fn a_pick_off_the_board_answers_nothing() {
        assert_eq!(editor(3, 3).brush_at(Pos::new(9, 9), false), None);
    }

    #[test]
    fn picking_a_tile_and_painting_it_back_leaves_the_board_alone() {
        let mut drawing = editor(6, 6);
        for position in [Pos::new(1, 1), Pos::new(2, 1), Pos::new(3, 1)] {
            drawing.paint(
                position,
                Brush::Connecting {
                    connection: Connection::River,
                },
            );
        }
        let before = drawing.map().clone();

        let picked = drawing
            .brush_at(Pos::new(2, 1), false)
            .expect("on the board");
        drawing.paint(Pos::new(2, 1), picked);

        assert_eq!(drawing.map(), &before);
    }

    #[test]
    fn a_map_opens_under_the_armies_it_already_seats() {
        let mut drawing = editor(9, 5);
        for (x, faction) in [
            (0u8, PlayerFaction::GreenEarth),
            (8, PlayerFaction::YellowComet),
        ] {
            drawing.paint(
                Pos::new(x, 0),
                Brush::Property {
                    property: PropertyKind::HQ,
                    faction: Some(FactionCode::new(faction)),
                },
            );
        }

        let reopened = MapEditor::open(drawing.map().clone());

        assert_eq!(
            reopened.roster().seats(),
            [PlayerFaction::GreenEarth, PlayerFaction::YellowComet]
        );
    }

    #[test]
    fn a_board_that_seats_nobody_opens_on_the_first_two_armies() {
        assert_eq!(
            editor(9, 5).roster().seats(),
            [PlayerFaction::OrangeStar, PlayerFaction::BlueMoon]
        );
    }

    #[test]
    fn a_quarter_turn_seats_four_armies() {
        let mut editor = editor(9, 9);
        assert_eq!(editor.roster().seats().len(), 2);

        assert!(editor.set_symmetry(Symmetry::Rotate90));

        assert_eq!(
            editor.roster().seats(),
            [
                PlayerFaction::OrangeStar,
                PlayerFaction::BlueMoon,
                PlayerFaction::GreenEarth,
                PlayerFaction::YellowComet
            ]
        );
    }

    #[test]
    fn a_mirrored_map_opens_under_its_own_fold() {
        let reopened = reopened(9, 5, Symmetry::MirrorLeftRight);

        assert_eq!(reopened.symmetry(), Symmetry::MirrorLeftRight);
        assert!(reopened.is_symmetric(Symmetry::MirrorLeftRight));
    }

    #[test]
    fn the_fold_a_map_opens_under_is_the_strongest_one_it_holds() {
        let reopened = reopened(9, 9, Symmetry::QuadMirror);

        assert_eq!(reopened.symmetry(), Symmetry::QuadMirror);
    }

    #[test]
    fn a_board_nobody_folded_opens_free() {
        let mut drawing = editor(9, 5);
        drawing.paint(
            Pos::new(1, 1),
            Brush::Terrain {
                terrain: AwbwTerrain::Mountain,
            },
        );

        let reopened = MapEditor::open(drawing.map().clone());

        assert_eq!(reopened.symmetry(), Symmetry::None);
    }

    #[test]
    fn a_blank_board_opens_free() {
        assert_eq!(editor(9, 9).symmetry(), Symmetry::None);
    }

    #[test]
    fn a_building_one_side_holds_alone_is_a_decision_and_not_a_broken_fold() {
        // A base set where the second army takes it on turn one is how a map
        // answers the first-turn advantage. The readout must leave it alone.
        let mut drawing = editor(9, 5);
        assert!(drawing.set_symmetry(Symmetry::MirrorLeftRight));
        drawing.paint(
            Pos::new(1, 2),
            Brush::Terrain {
                terrain: AwbwTerrain::Mountain,
            },
        );
        drawing.set_symmetry(Symmetry::None);
        drawing.paint(
            Pos::new(3, 2),
            Brush::Property {
                property: PropertyKind::Base,
                faction: Some(FactionCode::new(PlayerFaction::BlueMoon)),
            },
        );

        assert!(drawing.is_symmetric(Symmetry::MirrorLeftRight));
    }

    #[test]
    fn a_building_standing_on_the_fold_belongs_to_whoever_was_given_it() {
        let mut drawing = editor(9, 5);
        drawing.paint(
            Pos::new(4, 2),
            Brush::Property {
                property: PropertyKind::Airport,
                faction: Some(FactionCode::new(PlayerFaction::OrangeStar)),
            },
        );

        assert!(drawing.is_symmetric(Symmetry::MirrorLeftRight));
    }

    #[test]
    fn two_buildings_of_different_kinds_across_the_fold_are_left_alone() {
        let mut drawing = editor(9, 5);
        drawing.paint(
            Pos::new(1, 2),
            Brush::Property {
                property: PropertyKind::Base,
                faction: Some(FactionCode::new(PlayerFaction::OrangeStar)),
            },
        );
        drawing.paint(
            Pos::new(7, 2),
            Brush::Property {
                property: PropertyKind::Airport,
                faction: Some(FactionCode::new(PlayerFaction::BlueMoon)),
            },
        );

        assert!(drawing.is_symmetric(Symmetry::MirrorLeftRight));
    }

    #[test]
    fn ground_that_does_not_fold_is_still_reported() {
        // Leniency is about buildings and stops there. The shape of the land
        // is the one thing the fold does promise.
        let mut drawing = editor(9, 5);
        drawing.paint(
            Pos::new(1, 2),
            Brush::Terrain {
                terrain: AwbwTerrain::Mountain,
            },
        );

        assert!(!drawing.is_symmetric(Symmetry::MirrorLeftRight));
    }

    #[test]
    fn units_do_not_decide_which_fold_the_ground_holds() {
        let mut drawing = editor(9, 5);
        drawing.paint(
            Pos::new(2, 2),
            Brush::Unit {
                unit: Unit::Infantry,
                faction: FactionCode::new(PlayerFaction::OrangeStar),
                hp: VisualHp::new(10),
            },
        );

        assert!(drawing.is_symmetric(Symmetry::MirrorLeftRight));
    }

    #[test]
    fn a_uniform_board_with_a_unit_opens_free() {
        let mut drawing = editor(9, 9);
        drawing.paint(
            Pos::new(2, 2),
            Brush::Unit {
                unit: Unit::Infantry,
                faction: FactionCode::new(PlayerFaction::OrangeStar),
                hp: VisualHp::new(10),
            },
        );

        let reopened = MapEditor::open(drawing.map().clone());

        assert_eq!(reopened.symmetry(), Symmetry::None);
    }

    #[test]
    fn property_owners_do_not_decide_which_fold_the_ground_holds() {
        let mut drawing = editor(9, 5);
        assert!(drawing.set_symmetry(Symmetry::MirrorLeftRight));
        drawing.paint(
            Pos::new(0, 0),
            Brush::Property {
                property: PropertyKind::HQ,
                faction: Some(FactionCode::new(PlayerFaction::OrangeStar)),
            },
        );
        // A fold usually assigns another owner to the far property.
        drawing.set_symmetry(Symmetry::None);
        drawing.paint(
            Pos::new(8, 0),
            Brush::Property {
                property: PropertyKind::HQ,
                faction: Some(FactionCode::new(PlayerFaction::OrangeStar)),
            },
        );

        let reopened = MapEditor::open(drawing.map().clone());

        assert_eq!(reopened.symmetry(), Symmetry::MirrorLeftRight);
        assert!(reopened.is_symmetric(Symmetry::MirrorLeftRight));
    }

    #[test]
    fn a_mirrored_edit_lands_on_the_far_side_of_the_board() {
        let mut editor = editor(9, 5);
        assert!(editor.set_symmetry(Symmetry::MirrorLeftRight));

        editor.paint(
            Pos::new(1, 2),
            Brush::Terrain {
                terrain: AwbwTerrain::Mountain,
            },
        );

        assert_eq!(terrain(&editor, 1, 2), AwbwTerrain::Mountain);
        assert_eq!(terrain(&editor, 7, 2), AwbwTerrain::Mountain);
    }

    #[test]
    fn a_mirrored_headquarters_belongs_to_the_next_army() {
        let mut editor = editor(9, 5);
        editor.set_symmetry(Symmetry::Rotate180);

        editor.paint(
            Pos::new(1, 1),
            Brush::Property {
                property: PropertyKind::HQ,
                faction: Some(FactionCode::new(PlayerFaction::OrangeStar)),
            },
        );

        assert_eq!(
            terrain(&editor, 1, 1),
            AwbwTerrain::Property(Property::HQ(PlayerFaction::OrangeStar))
        );
        assert_eq!(
            terrain(&editor, 7, 3),
            AwbwTerrain::Property(Property::HQ(PlayerFaction::BlueMoon))
        );
    }

    #[test]
    fn neutral_ground_stays_neutral_under_symmetry() {
        let mut editor = editor(8, 4);
        editor.set_symmetry(Symmetry::MirrorLeftRight);

        editor.paint(
            Pos::new(2, 1),
            Brush::Property {
                property: PropertyKind::City,
                faction: None,
            },
        );

        assert_eq!(
            terrain(&editor, 5, 1),
            AwbwTerrain::Property(Property::City(Faction::Neutral))
        );
    }

    #[test]
    fn a_quarter_turn_needs_a_square_board() {
        let mut wide = editor(10, 6);
        assert!(!wide.set_symmetry(Symmetry::Rotate90));
        assert_eq!(wide.symmetry(), Symmetry::None);

        let mut square = editor(8, 8);
        assert!(square.set_symmetry(Symmetry::Rotate90));
    }

    #[test]
    fn a_quarter_turn_gives_four_armies_one_corner_each() {
        let mut editor = editor(8, 8);
        editor.set_symmetry(Symmetry::Rotate90);

        editor.paint(
            Pos::new(1, 0),
            Brush::Property {
                property: PropertyKind::HQ,
                faction: Some(FactionCode::new(PlayerFaction::OrangeStar)),
            },
        );

        let held: Vec<AwbwTerrain> = editor
            .map()
            .iter()
            .filter(|(_, terrain)| matches!(terrain, AwbwTerrain::Property(Property::HQ(_))))
            .map(|(_, terrain)| terrain)
            .collect();
        assert_eq!(held.len(), 4, "each corner holds one headquarters");
        for faction in [
            PlayerFaction::OrangeStar,
            PlayerFaction::BlueMoon,
            PlayerFaction::GreenEarth,
            PlayerFaction::YellowComet,
        ] {
            assert!(held.contains(&AwbwTerrain::Property(Property::HQ(faction))));
        }
    }

    #[test]
    fn a_road_joins_the_road_beside_it() {
        let mut editor = editor(6, 3);

        editor.paint(
            Pos::new(1, 1),
            Brush::Connecting {
                connection: Connection::Road,
            },
        );
        assert_eq!(
            terrain(&editor, 1, 1),
            AwbwTerrain::Road(RoadType::Horizontal)
        );

        editor.paint(
            Pos::new(2, 1),
            Brush::Connecting {
                connection: Connection::Road,
            },
        );
        assert_eq!(
            terrain(&editor, 1, 1),
            AwbwTerrain::Road(RoadType::Horizontal)
        );
        assert_eq!(
            terrain(&editor, 2, 1),
            AwbwTerrain::Road(RoadType::Horizontal)
        );

        editor.paint(
            Pos::new(2, 0),
            Brush::Connecting {
                connection: Connection::Road,
            },
        );
        assert_eq!(terrain(&editor, 2, 1), AwbwTerrain::Road(RoadType::WN));
    }

    #[test]
    fn a_mirrored_road_corner_turns_with_the_board() {
        let mut editor = editor(8, 4);
        editor.set_symmetry(Symmetry::MirrorLeftRight);

        editor.paint(
            Pos::new(1, 1),
            Brush::Connecting {
                connection: Connection::Road,
            },
        );
        editor.paint(
            Pos::new(2, 1),
            Brush::Connecting {
                connection: Connection::Road,
            },
        );
        editor.paint(
            Pos::new(2, 2),
            Brush::Connecting {
                connection: Connection::Road,
            },
        );

        // The corner turns east and south on the left, and south and west on
        // the right, which is the same corner seen in a mirror.
        assert_eq!(terrain(&editor, 2, 1), AwbwTerrain::Road(RoadType::SW));
        assert_eq!(terrain(&editor, 5, 1), AwbwTerrain::Road(RoadType::ES));
        assert_eq!(
            terrain(&editor, 2, 2),
            AwbwTerrain::Road(RoadType::Vertical)
        );
        assert_eq!(
            terrain(&editor, 5, 2),
            AwbwTerrain::Road(RoadType::Vertical)
        );
    }

    #[test]
    fn a_turned_road_corner_faces_the_way_the_board_was_turned() {
        let corner = AwbwTerrain::Road(RoadType::NE);
        let half_turn = Isometry::new(false, true, true);
        assert_eq!(
            half_turn.turn_terrain(corner),
            AwbwTerrain::Road(RoadType::SW)
        );

        let quarter_turn = Isometry::new(true, true, false);
        assert_eq!(
            quarter_turn.turn_terrain(AwbwTerrain::Road(RoadType::Horizontal)),
            AwbwTerrain::Road(RoadType::Vertical)
        );
    }

    #[test]
    fn a_unit_is_mirrored_to_the_army_of_its_side() {
        let mut editor = editor(8, 4);
        editor.set_symmetry(Symmetry::MirrorLeftRight);

        editor.paint(
            Pos::new(1, 1),
            Brush::Unit {
                unit: Unit::Infantry,
                faction: FactionCode::new(PlayerFaction::OrangeStar),
                hp: VisualHp::new(10),
            },
        );

        let mirrored = editor
            .map()
            .deployments()
            .get(Pos::new(6, 1))
            .copied()
            .expect("the mirror places a unit");
        assert_eq!(mirrored.faction, PlayerFaction::BlueMoon);
        assert_eq!(mirrored.unit, Unit::Infantry);
    }

    #[test]
    fn painting_over_a_unit_replaces_it() {
        let mut editor = editor(6, 3);
        let brush = |faction| Brush::Unit {
            unit: Unit::Infantry,
            faction: FactionCode::new(faction),
            hp: VisualHp::new(10),
        };

        editor.paint(Pos::new(2, 1), brush(PlayerFaction::OrangeStar));
        editor.paint(Pos::new(2, 1), brush(PlayerFaction::BlueMoon));

        let held = editor.map().deployments().get(Pos::new(2, 1)).copied();
        assert_eq!(held.map(|unit| unit.faction), Some(PlayerFaction::BlueMoon));
        assert_eq!(editor.map().deployments().len(), 1);
    }

    #[test]
    fn erasing_takes_the_unit_and_the_ground_with_it() {
        let mut editor = editor(6, 3);
        editor.paint(
            Pos::new(2, 1),
            Brush::Terrain {
                terrain: AwbwTerrain::Mountain,
            },
        );
        editor.paint(
            Pos::new(2, 1),
            Brush::Unit {
                unit: Unit::Mech,
                faction: FactionCode::new(PlayerFaction::OrangeStar),
                hp: VisualHp::new(10),
            },
        );

        editor.paint(Pos::new(2, 1), Brush::Erase);

        assert_eq!(terrain(&editor, 2, 1), AwbwTerrain::Plain);
        assert!(editor.map().deployments().is_empty());
    }

    #[test]
    fn a_stroke_is_one_step_of_undo() {
        let mut editor = editor(6, 3);
        let brush = Brush::Terrain {
            terrain: AwbwTerrain::Mountain,
        };

        editor.begin_stroke();
        editor.paint(Pos::new(1, 1), brush);
        editor.paint(Pos::new(2, 1), brush);
        editor.paint(Pos::new(3, 1), brush);
        editor.end_stroke();

        editor.undo();

        assert_eq!(terrain(&editor, 1, 1), AwbwTerrain::Plain);
        assert_eq!(terrain(&editor, 3, 1), AwbwTerrain::Plain);
        assert!(!editor.can_undo());
        assert!(editor.can_redo());

        editor.redo();
        assert_eq!(terrain(&editor, 2, 1), AwbwTerrain::Mountain);
    }

    #[test]
    fn an_edit_reports_only_the_tiles_that_look_different() {
        let mut editor = editor(6, 3);
        let brush = Brush::Terrain {
            terrain: AwbwTerrain::Mountain,
        };

        let first = editor.paint(Pos::new(2, 1), brush);
        assert_eq!(first.terrain.len(), 1);
        assert_eq!(first.terrain[0].position, Pos::new(2, 1));

        let second = editor.paint(Pos::new(2, 1), brush);
        assert!(
            second.is_empty(),
            "painting the same tile twice is no change"
        );
    }

    #[test]
    fn resizing_keeps_what_still_fits() {
        let mut editor = editor(6, 6);
        editor.paint(
            Pos::new(1, 1),
            Brush::Terrain {
                terrain: AwbwTerrain::Mountain,
            },
        );
        editor.paint(
            Pos::new(5, 5),
            Brush::Unit {
                unit: Unit::Infantry,
                faction: FactionCode::new(PlayerFaction::OrangeStar),
                hp: VisualHp::new(10),
            },
        );

        let changes = editor.resize(Dimensions::new(4, 4), ResizeAnchor::TopLeft);

        assert_eq!(changes.dimensions, Some([4, 4]));
        assert_eq!(terrain(&editor, 1, 1), AwbwTerrain::Mountain);
        assert!(
            editor.map().deployments().is_empty(),
            "a unit outside the new board is dropped"
        );
    }

    #[test]
    fn resizing_out_of_a_square_board_steps_symmetry_down() {
        let mut editor = editor(8, 8);
        assert!(editor.set_symmetry(Symmetry::Rotate90));

        editor.resize(Dimensions::new(10, 8), ResizeAnchor::TopLeft);

        assert_eq!(editor.symmetry(), Symmetry::None);
    }

    #[test]
    fn a_mirror_pairs_a_four_army_roster_two_at_a_time() {
        let roster = ArmyRoster::standard();

        // Under a mirror the four armies read as two facing pairs.
        assert_eq!(
            roster.rotate(PlayerFaction::OrangeStar, Symmetry::MirrorLeftRight, 1),
            PlayerFaction::BlueMoon
        );
        assert_eq!(
            roster.rotate(PlayerFaction::GreenEarth, Symmetry::MirrorLeftRight, 1),
            PlayerFaction::YellowComet
        );
        assert_eq!(
            roster.rotate(PlayerFaction::YellowComet, Symmetry::MirrorLeftRight, 1),
            PlayerFaction::GreenEarth
        );

        // Under a quarter turn they read as one set of four.
        assert_eq!(
            roster.rotate(PlayerFaction::OrangeStar, Symmetry::Rotate90, 1),
            PlayerFaction::BlueMoon
        );
        assert_eq!(
            roster.rotate(PlayerFaction::YellowComet, Symmetry::Rotate90, 1),
            PlayerFaction::OrangeStar
        );
    }

    #[test]
    fn the_quarters_of_a_quad_mirror_seat_the_same_armies_from_any_quarter() {
        let roster = ArmyRoster::standard();

        // A step is its own opposite in every quarter of the fold, so a stroke
        // drawn in one quarter seats the armies the same way as the same
        // stroke drawn in another.
        for step in 0..4 {
            for seat in 0..4 {
                let army = roster.seats()[seat];
                let image = roster.rotate(army, Symmetry::QuadMirror, step);
                assert_eq!(
                    roster.rotate(image, Symmetry::QuadMirror, step),
                    army,
                    "seat {seat} does not come back to itself after two steps of {step}"
                );
            }
        }
    }

    #[test]
    fn an_army_off_the_roster_is_left_alone() {
        let roster = ArmyRoster::standard();
        assert_eq!(
            roster.rotate(PlayerFaction::BlackHole, Symmetry::MirrorLeftRight, 1),
            PlayerFaction::BlackHole
        );
    }

    #[test]
    fn every_symmetry_closes_on_itself() {
        let dimensions = Dimensions::new(8, 8);
        for symmetry in Symmetry::ALL {
            let images: Vec<Pos> = symmetry
                .isometries()
                .iter()
                .filter_map(|isometry| isometry.apply(Pos::new(1, 2), dimensions))
                .collect();

            for image in &images {
                let from_image: Vec<Pos> = symmetry
                    .isometries()
                    .iter()
                    .filter_map(|isometry| isometry.apply(*image, dimensions))
                    .collect();
                for reached in from_image {
                    assert!(
                        images.contains(&reached),
                        "{symmetry:?} reaches {reached:?} from an image but not from the tile"
                    );
                }
            }
        }
    }

    /// The muster names the tiles rather than saying only that something is
    /// wrong, so a map maker on a 19 by 19 board is told where to look.
    #[test]
    fn an_uneven_board_names_the_tiles_that_do_not_fold() {
        let mut drawing = editor(6, 4);
        drawing.set_symmetry(Symmetry::MirrorLeftRight);
        drawing.paint(
            Pos::new(1, 1),
            Brush::Terrain {
                terrain: AwbwTerrain::Mountain,
            },
        );
        assert!(drawing.asymmetries(Symmetry::MirrorLeftRight).is_empty());

        // Reaching the far tile with the fold off leaves the board uneven at
        // exactly the pair the stroke broke.
        drawing.set_symmetry(Symmetry::None);
        drawing.paint(
            Pos::new(4, 1),
            Brush::Terrain {
                terrain: AwbwTerrain::Wood,
            },
        );

        assert_eq!(
            drawing.asymmetries(Symmetry::MirrorLeftRight),
            vec![Pos::new(1, 1), Pos::new(4, 1)]
        );
        assert!(!drawing.is_symmetric(Symmetry::MirrorLeftRight));
    }

    #[test]
    fn a_board_that_folds_names_no_tiles() {
        let drawing = editor(6, 4);
        assert!(drawing.asymmetries(Symmetry::MirrorLeftRight).is_empty());
        assert!(drawing.asymmetries(Symmetry::None).is_empty());
    }
}
