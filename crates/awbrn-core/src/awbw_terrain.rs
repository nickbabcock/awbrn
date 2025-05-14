use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    BridgeType, Faction, GameplayTerrain, MissileSiloStatus, PipeRubbleType, PipeSeamType,
    PipeType, PlayerFaction, Property, RiverType, RoadType, ShoalType,
};

/// Main terrain type enum that categorizes terrain by its primary function
///
/// Ref: <https://awbw.amarriner.com/terrain_map.php>
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AwbwTerrain {
    Plain,
    Mountain,
    Wood,
    River(RiverType),
    Road(RoadType),
    Bridge(BridgeType),
    Sea,
    Shoal(ShoalType),
    Reef,
    Property(Property),
    Pipe(PipeType),
    MissileSilo(MissileSiloStatus),
    PipeSeam(PipeSeamType),
    PipeRubble(PipeRubbleType),
    Teleporter,
}

impl AwbwTerrain {
    /// Get the symbol character for this terrain
    pub fn symbol(&self) -> Option<char> {
        match self {
            AwbwTerrain::Plain => Some('.'),
            AwbwTerrain::Mountain => Some('^'),
            AwbwTerrain::Wood => Some('@'),
            AwbwTerrain::River(RiverType::Horizontal) => Some('{'),
            AwbwTerrain::River(RiverType::Vertical) => Some('}'),
            AwbwTerrain::River(RiverType::Cross) => Some('~'),
            AwbwTerrain::River(RiverType::ES) => Some('I'),
            AwbwTerrain::River(RiverType::SW) => Some('J'),
            AwbwTerrain::River(RiverType::WN) => Some('K'),
            AwbwTerrain::River(RiverType::NE) => Some('L'),
            AwbwTerrain::River(RiverType::ESW) => Some('M'),
            AwbwTerrain::River(RiverType::SWN) => Some('N'),
            AwbwTerrain::River(RiverType::WNE) => Some('O'),
            AwbwTerrain::River(RiverType::NES) => Some('P'),
            AwbwTerrain::Road(RoadType::Horizontal) => Some('-'),
            AwbwTerrain::Road(RoadType::Vertical) => Some('='),
            AwbwTerrain::Road(RoadType::Cross) => Some('+'),
            AwbwTerrain::Road(RoadType::ES) => Some('A'),
            AwbwTerrain::Road(RoadType::SW) => Some('B'),
            AwbwTerrain::Road(RoadType::WN) => Some('C'),
            AwbwTerrain::Road(RoadType::NE) => Some('D'),
            AwbwTerrain::Road(RoadType::ESW) => Some('E'),
            AwbwTerrain::Road(RoadType::SWN) => Some('F'),
            AwbwTerrain::Road(RoadType::WNE) => Some('G'),
            AwbwTerrain::Road(RoadType::NES) => Some('H'),
            AwbwTerrain::Bridge(BridgeType::Horizontal) => Some('['),
            AwbwTerrain::Bridge(BridgeType::Vertical) => Some(']'),
            AwbwTerrain::Sea => Some(','),
            AwbwTerrain::Shoal(ShoalType::Horizontal) => Some('<'),
            AwbwTerrain::Shoal(ShoalType::HorizontalNorth) => Some('('),
            AwbwTerrain::Shoal(ShoalType::Vertical) => Some('>'),
            AwbwTerrain::Shoal(ShoalType::VerticalEast) => Some(')'),
            AwbwTerrain::Reef => Some('%'),
            AwbwTerrain::Property(Property::City(Faction::Neutral)) => Some('a'),
            AwbwTerrain::Property(Property::Base(Faction::Neutral)) => Some('b'),
            AwbwTerrain::Property(Property::Airport(Faction::Neutral)) => Some('c'),
            AwbwTerrain::Property(Property::Port(Faction::Neutral)) => Some('d'),
            AwbwTerrain::Property(Property::City(Faction::Player(PlayerFaction::OrangeStar))) => {
                Some('e')
            }
            AwbwTerrain::Property(Property::Base(Faction::Player(PlayerFaction::OrangeStar))) => {
                Some('f')
            }
            AwbwTerrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::OrangeStar,
            ))) => Some('g'),
            AwbwTerrain::Property(Property::Port(Faction::Player(PlayerFaction::OrangeStar))) => {
                Some('h')
            }
            AwbwTerrain::Property(Property::HQ(PlayerFaction::OrangeStar)) => Some('i'),
            AwbwTerrain::Property(Property::City(Faction::Player(PlayerFaction::BlueMoon))) => {
                Some('j')
            }
            AwbwTerrain::Property(Property::Base(Faction::Player(PlayerFaction::BlueMoon))) => {
                Some('l')
            }
            AwbwTerrain::Property(Property::Airport(Faction::Player(PlayerFaction::BlueMoon))) => {
                Some('m')
            }
            AwbwTerrain::Property(Property::Port(Faction::Player(PlayerFaction::BlueMoon))) => {
                Some('n')
            }
            AwbwTerrain::Property(Property::HQ(PlayerFaction::BlueMoon)) => Some('o'),
            AwbwTerrain::Property(Property::City(Faction::Player(PlayerFaction::GreenEarth))) => {
                Some('p')
            }
            AwbwTerrain::Property(Property::Base(Faction::Player(PlayerFaction::GreenEarth))) => {
                Some('q')
            }
            AwbwTerrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::GreenEarth,
            ))) => Some('r'),
            AwbwTerrain::Property(Property::Port(Faction::Player(PlayerFaction::GreenEarth))) => {
                Some('s')
            }
            AwbwTerrain::Property(Property::HQ(PlayerFaction::GreenEarth)) => Some('t'),
            AwbwTerrain::Property(Property::City(Faction::Player(PlayerFaction::YellowComet))) => {
                Some('u')
            }
            AwbwTerrain::Property(Property::Base(Faction::Player(PlayerFaction::YellowComet))) => {
                Some('v')
            }
            AwbwTerrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::YellowComet,
            ))) => Some('w'),
            AwbwTerrain::Property(Property::Port(Faction::Player(PlayerFaction::YellowComet))) => {
                Some('x')
            }
            AwbwTerrain::Property(Property::HQ(PlayerFaction::YellowComet)) => Some('y'),
            AwbwTerrain::Property(Property::City(Faction::Player(PlayerFaction::RedFire))) => {
                Some('U')
            }
            AwbwTerrain::Property(Property::Base(Faction::Player(PlayerFaction::RedFire))) => {
                Some('T')
            }
            AwbwTerrain::Property(Property::Airport(Faction::Player(PlayerFaction::RedFire))) => {
                Some('S')
            }
            AwbwTerrain::Property(Property::Port(Faction::Player(PlayerFaction::RedFire))) => {
                Some('R')
            }
            AwbwTerrain::Property(Property::HQ(PlayerFaction::RedFire)) => Some('Q'),
            AwbwTerrain::Property(Property::City(Faction::Player(PlayerFaction::GreySky))) => {
                Some('Z')
            }
            AwbwTerrain::Property(Property::Base(Faction::Player(PlayerFaction::GreySky))) => {
                Some('Y')
            }
            AwbwTerrain::Property(Property::Airport(Faction::Player(PlayerFaction::GreySky))) => {
                Some('X')
            }
            AwbwTerrain::Property(Property::Port(Faction::Player(PlayerFaction::GreySky))) => {
                Some('W')
            }
            AwbwTerrain::Property(Property::HQ(PlayerFaction::GreySky)) => Some('V'),
            AwbwTerrain::Property(Property::City(Faction::Player(PlayerFaction::BlackHole))) => {
                Some('1')
            }
            AwbwTerrain::Property(Property::Base(Faction::Player(PlayerFaction::BlackHole))) => {
                Some('2')
            }
            AwbwTerrain::Property(Property::Airport(Faction::Player(PlayerFaction::BlackHole))) => {
                Some('3')
            }
            AwbwTerrain::Property(Property::Port(Faction::Player(PlayerFaction::BlackHole))) => {
                Some('4')
            }
            AwbwTerrain::Property(Property::HQ(PlayerFaction::BlackHole)) => Some('5'),
            AwbwTerrain::Pipe(PipeType::Vertical) => Some('k'),
            AwbwTerrain::Pipe(PipeType::Horizontal) => Some('z'),
            AwbwTerrain::Pipe(PipeType::NE) => Some('!'),
            AwbwTerrain::Pipe(PipeType::ES) => Some('#'),
            AwbwTerrain::Pipe(PipeType::SW) => Some('$'),
            AwbwTerrain::Pipe(PipeType::WN) => Some('&'),
            AwbwTerrain::Pipe(PipeType::NorthEnd) => Some('*'),
            AwbwTerrain::Pipe(PipeType::EastEnd) => Some('|'),
            AwbwTerrain::Pipe(PipeType::SouthEnd) => Some('`'),
            AwbwTerrain::Pipe(PipeType::WestEnd) => Some('\''),
            AwbwTerrain::MissileSilo(MissileSiloStatus::Loaded) => Some('"'),
            AwbwTerrain::MissileSilo(MissileSiloStatus::Unloaded) => Some(';'),
            AwbwTerrain::PipeSeam(PipeSeamType::Horizontal) => Some(':'),
            AwbwTerrain::PipeSeam(PipeSeamType::Vertical) => Some('?'),
            AwbwTerrain::PipeRubble(PipeRubbleType::Horizontal) => Some('/'),
            AwbwTerrain::PipeRubble(PipeRubbleType::Vertical) => Some('0'),
            AwbwTerrain::Property(Property::ComTower(Faction::Neutral)) => Some('_'),
            AwbwTerrain::Property(Property::Lab(Faction::Neutral)) => Some('6'),
            _ => None,
        }
    }

    /// Get the name of this terrain
    pub const fn name(&self) -> &'static str {
        match self {
            // Basic terrains
            AwbwTerrain::Plain => "Plain",
            AwbwTerrain::Mountain => "Mountain",
            AwbwTerrain::Wood => "Wood",

            // Rivers
            AwbwTerrain::River(RiverType::Horizontal) => "HRiver",
            AwbwTerrain::River(RiverType::Vertical) => "VRiver",
            AwbwTerrain::River(RiverType::Cross) => "CRiver",
            AwbwTerrain::River(RiverType::ES) => "ESRiver",
            AwbwTerrain::River(RiverType::SW) => "SWRiver",
            AwbwTerrain::River(RiverType::WN) => "WNRiver",
            AwbwTerrain::River(RiverType::NE) => "NERiver",
            AwbwTerrain::River(RiverType::ESW) => "ESWRiver",
            AwbwTerrain::River(RiverType::SWN) => "SWNRiver",
            AwbwTerrain::River(RiverType::WNE) => "WNERiver",
            AwbwTerrain::River(RiverType::NES) => "NESRiver",

            // Roads
            AwbwTerrain::Road(RoadType::Horizontal) => "HRoad",
            AwbwTerrain::Road(RoadType::Vertical) => "VRoad",
            AwbwTerrain::Road(RoadType::Cross) => "CRoad",
            AwbwTerrain::Road(RoadType::ES) => "ESRoad",
            AwbwTerrain::Road(RoadType::SW) => "SWRoad",
            AwbwTerrain::Road(RoadType::WN) => "WNRoad",
            AwbwTerrain::Road(RoadType::NE) => "NERoad",
            AwbwTerrain::Road(RoadType::ESW) => "ESWRoad",
            AwbwTerrain::Road(RoadType::SWN) => "SWNRoad",
            AwbwTerrain::Road(RoadType::WNE) => "WNERoad",
            AwbwTerrain::Road(RoadType::NES) => "NESRoad",

            // Bridges
            AwbwTerrain::Bridge(BridgeType::Horizontal) => "HBridge",
            AwbwTerrain::Bridge(BridgeType::Vertical) => "VBridge",

            // Sea and coastal
            AwbwTerrain::Sea => "Sea",
            AwbwTerrain::Shoal(ShoalType::Horizontal) => "HShoal",
            AwbwTerrain::Shoal(ShoalType::HorizontalNorth) => "HShoalN",
            AwbwTerrain::Shoal(ShoalType::Vertical) => "VShoal",
            AwbwTerrain::Shoal(ShoalType::VerticalEast) => "VShoalE",
            AwbwTerrain::Reef => "Reef",

            AwbwTerrain::Property(x) => x.name(),

            // Pipes
            AwbwTerrain::Pipe(PipeType::Vertical) => "VPipe",
            AwbwTerrain::Pipe(PipeType::Horizontal) => "HPipe",
            AwbwTerrain::Pipe(PipeType::NE) => "NEPipe",
            AwbwTerrain::Pipe(PipeType::ES) => "ESPipe",
            AwbwTerrain::Pipe(PipeType::SW) => "SWPipe",
            AwbwTerrain::Pipe(PipeType::WN) => "WNPipe",
            AwbwTerrain::Pipe(PipeType::NorthEnd) => "NPipe End",
            AwbwTerrain::Pipe(PipeType::EastEnd) => "EPipe End",
            AwbwTerrain::Pipe(PipeType::SouthEnd) => "SPipe End",
            AwbwTerrain::Pipe(PipeType::WestEnd) => "WPipe End",

            // Missile silos
            AwbwTerrain::MissileSilo(MissileSiloStatus::Loaded) => "Missile Silo",
            AwbwTerrain::MissileSilo(MissileSiloStatus::Unloaded) => "Missile Silo Empty",

            // Pipe seams and rubble
            AwbwTerrain::PipeSeam(PipeSeamType::Horizontal) => "HPipe Seam",
            AwbwTerrain::PipeSeam(PipeSeamType::Vertical) => "VPipe Seam",
            AwbwTerrain::PipeRubble(PipeRubbleType::Horizontal) => "HPipe Rubble",
            AwbwTerrain::PipeRubble(PipeRubbleType::Vertical) => "VPipe Rubble",

            // Teleporter
            AwbwTerrain::Teleporter => "Teleporter",
        }
    }

    /// Get the ID of terrain
    pub fn id(&self) -> AwbwTerrainId {
        AwbwTerrainId::from(*self)
    }

    /// Get the faction that owns this property (if applicable)
    pub fn owner(&self) -> Option<Faction> {
        match self {
            AwbwTerrain::Property(property_type) => Some(match property_type {
                Property::City(faction) => *faction,
                Property::Base(faction) => *faction,
                Property::Airport(faction) => *faction,
                Property::Port(faction) => *faction,
                Property::HQ(faction) => Faction::Player(*faction),
                Property::ComTower(faction) => *faction,
                Property::Lab(faction) => *faction,
            }),
            _ => None,
        }
    }

    /// Check if a property is an HQ
    pub fn is_hq(&self) -> bool {
        matches!(self, AwbwTerrain::Property(Property::HQ(_)))
    }

    /// Get defense stars for the terrain (0-4 stars)
    pub fn defense_stars(&self) -> u8 {
        match self.gameplay_type() {
            GameplayTerrain::Plain => 1,
            GameplayTerrain::Mountain => 4,
            GameplayTerrain::Wood => 2,
            GameplayTerrain::River => 0,
            GameplayTerrain::Road => 0,
            GameplayTerrain::Bridge => 0,
            GameplayTerrain::Sea => 0,
            GameplayTerrain::Shoal => 0,
            GameplayTerrain::Reef => 1,
            GameplayTerrain::Property(property_category) => match property_category {
                Property::HQ(_) => 4,
                _ => 3,
            },
            GameplayTerrain::Pipe => 1,
            GameplayTerrain::PipeSeam => 1,
            GameplayTerrain::PipeRubble => 1,
            GameplayTerrain::MissileSilo(_) => 3,
            GameplayTerrain::Teleporter => 0,
        }
    }

    /// Check if terrain can be occupied by ground units
    pub fn is_land(&self) -> bool {
        !matches!(
            self.gameplay_type(),
            GameplayTerrain::River | GameplayTerrain::Sea
        )
    }

    /// Check if terrain can be occupied by naval units
    pub fn is_sea(&self) -> bool {
        matches!(
            self.gameplay_type(),
            GameplayTerrain::Sea | GameplayTerrain::Property(Property::Port(_))
        )
    }

    /// Check if terrain can be captured by infantry
    pub fn is_capturable(&self) -> bool {
        matches!(self, AwbwTerrain::Property(_))
    }

    /// Get the gameplay-relevant terrain type
    pub fn gameplay_type(&self) -> GameplayTerrain {
        match self {
            AwbwTerrain::Plain => GameplayTerrain::Plain,
            AwbwTerrain::Mountain => GameplayTerrain::Mountain,
            AwbwTerrain::Wood => GameplayTerrain::Wood,
            AwbwTerrain::River(_) => GameplayTerrain::River,
            AwbwTerrain::Road(_) => GameplayTerrain::Road,
            AwbwTerrain::Bridge(_) => GameplayTerrain::Bridge,
            AwbwTerrain::Sea => GameplayTerrain::Sea,
            AwbwTerrain::Shoal(_) => GameplayTerrain::Shoal,
            AwbwTerrain::Reef => GameplayTerrain::Reef,
            AwbwTerrain::Property(property) => GameplayTerrain::Property(match property {
                Property::City(faction) => Property::City(*faction),
                Property::Base(faction) => Property::Base(*faction),
                Property::Airport(faction) => Property::Airport(*faction),
                Property::Port(faction) => Property::Port(*faction),
                Property::HQ(faction) => Property::HQ(*faction),
                Property::ComTower(faction) => Property::ComTower(*faction),
                Property::Lab(faction) => Property::Lab(*faction),
            }),
            AwbwTerrain::Pipe(_) => GameplayTerrain::Pipe,
            AwbwTerrain::PipeSeam(_) => GameplayTerrain::PipeSeam,
            AwbwTerrain::PipeRubble(_) => GameplayTerrain::PipeRubble,
            AwbwTerrain::MissileSilo(status) => GameplayTerrain::MissileSilo(*status),
            AwbwTerrain::Teleporter => GameplayTerrain::Teleporter,
        }
    }
}

/// Custom deserializer implementation to handle deserializing terrain from numeric IDs
impl<'de> Deserialize<'de> for AwbwTerrain {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // First try to deserialize as a u8
        let value = u8::deserialize(deserializer)?;

        // Then use our TryFrom<u8> implementation
        AwbwTerrain::try_from(value).map_err(serde::de::Error::custom)
    }
}

impl Serialize for AwbwTerrain {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u8(self.id().0)
    }
}

/// Newtype wrapper around u8 for terrain ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AwbwTerrainId(u8);

impl AwbwTerrainId {
    /// Create a new Terrain with the given ID
    pub const fn new(id: u8) -> Self {
        Self(id)
    }
}

impl From<AwbwTerrain> for AwbwTerrainId {
    fn from(terrain_type: AwbwTerrain) -> Self {
        match terrain_type {
            AwbwTerrain::Plain => AwbwTerrainId(1),
            AwbwTerrain::Mountain => AwbwTerrainId(2),
            AwbwTerrain::Wood => AwbwTerrainId(3),

            // Rivers
            AwbwTerrain::River(river_type) => match river_type {
                RiverType::Horizontal => AwbwTerrainId(4),
                RiverType::Vertical => AwbwTerrainId(5),
                RiverType::Cross => AwbwTerrainId(6),
                RiverType::ES => AwbwTerrainId(7),
                RiverType::SW => AwbwTerrainId(8),
                RiverType::WN => AwbwTerrainId(9),
                RiverType::NE => AwbwTerrainId(10),
                RiverType::ESW => AwbwTerrainId(11),
                RiverType::SWN => AwbwTerrainId(12),
                RiverType::WNE => AwbwTerrainId(13),
                RiverType::NES => AwbwTerrainId(14),
            },

            // Roads
            AwbwTerrain::Road(road_type) => match road_type {
                RoadType::Horizontal => AwbwTerrainId(15),
                RoadType::Vertical => AwbwTerrainId(16),
                RoadType::Cross => AwbwTerrainId(17),
                RoadType::ES => AwbwTerrainId(18),
                RoadType::SW => AwbwTerrainId(19),
                RoadType::WN => AwbwTerrainId(20),
                RoadType::NE => AwbwTerrainId(21),
                RoadType::ESW => AwbwTerrainId(22),
                RoadType::SWN => AwbwTerrainId(23),
                RoadType::WNE => AwbwTerrainId(24),
                RoadType::NES => AwbwTerrainId(25),
            },

            // Bridges
            AwbwTerrain::Bridge(bridge_type) => match bridge_type {
                BridgeType::Horizontal => AwbwTerrainId(26),
                BridgeType::Vertical => AwbwTerrainId(27),
            },

            AwbwTerrain::Sea => AwbwTerrainId(28),

            // Shoals
            AwbwTerrain::Shoal(shoal_type) => match shoal_type {
                ShoalType::Horizontal => AwbwTerrainId(29),
                ShoalType::HorizontalNorth => AwbwTerrainId(30),
                ShoalType::Vertical => AwbwTerrainId(31),
                ShoalType::VerticalEast => AwbwTerrainId(32),
            },

            AwbwTerrain::Reef => AwbwTerrainId(33),

            // Properties
            AwbwTerrain::Property(property_type) => match property_type {
                // Neutral properties
                Property::City(Faction::Neutral) => AwbwTerrainId(34),
                Property::Base(Faction::Neutral) => AwbwTerrainId(35),
                Property::Airport(Faction::Neutral) => AwbwTerrainId(36),
                Property::Port(Faction::Neutral) => AwbwTerrainId(37),

                // Orange Star properties
                Property::City(Faction::Player(PlayerFaction::OrangeStar)) => AwbwTerrainId(38),
                Property::Base(Faction::Player(PlayerFaction::OrangeStar)) => AwbwTerrainId(39),
                Property::Airport(Faction::Player(PlayerFaction::OrangeStar)) => AwbwTerrainId(40),
                Property::Port(Faction::Player(PlayerFaction::OrangeStar)) => AwbwTerrainId(41),
                Property::HQ(PlayerFaction::OrangeStar) => AwbwTerrainId(42),

                // Blue Moon properties
                Property::City(Faction::Player(PlayerFaction::BlueMoon)) => AwbwTerrainId(43),
                Property::Base(Faction::Player(PlayerFaction::BlueMoon)) => AwbwTerrainId(44),
                Property::Airport(Faction::Player(PlayerFaction::BlueMoon)) => AwbwTerrainId(45),
                Property::Port(Faction::Player(PlayerFaction::BlueMoon)) => AwbwTerrainId(46),
                Property::HQ(PlayerFaction::BlueMoon) => AwbwTerrainId(47),

                // Green Earth properties
                Property::City(Faction::Player(PlayerFaction::GreenEarth)) => AwbwTerrainId(48),
                Property::Base(Faction::Player(PlayerFaction::GreenEarth)) => AwbwTerrainId(49),
                Property::Airport(Faction::Player(PlayerFaction::GreenEarth)) => AwbwTerrainId(50),
                Property::Port(Faction::Player(PlayerFaction::GreenEarth)) => AwbwTerrainId(51),
                Property::HQ(PlayerFaction::GreenEarth) => AwbwTerrainId(52),

                // Yellow Comet properties
                Property::City(Faction::Player(PlayerFaction::YellowComet)) => AwbwTerrainId(53),
                Property::Base(Faction::Player(PlayerFaction::YellowComet)) => AwbwTerrainId(54),
                Property::Airport(Faction::Player(PlayerFaction::YellowComet)) => AwbwTerrainId(55),
                Property::Port(Faction::Player(PlayerFaction::YellowComet)) => AwbwTerrainId(56),
                Property::HQ(PlayerFaction::YellowComet) => AwbwTerrainId(57),

                // Red Fire properties
                Property::City(Faction::Player(PlayerFaction::RedFire)) => AwbwTerrainId(81),
                Property::Base(Faction::Player(PlayerFaction::RedFire)) => AwbwTerrainId(82),
                Property::Airport(Faction::Player(PlayerFaction::RedFire)) => AwbwTerrainId(83),
                Property::Port(Faction::Player(PlayerFaction::RedFire)) => AwbwTerrainId(84),
                Property::HQ(PlayerFaction::RedFire) => AwbwTerrainId(85),

                // Grey Sky properties
                Property::City(Faction::Player(PlayerFaction::GreySky)) => AwbwTerrainId(86),
                Property::Base(Faction::Player(PlayerFaction::GreySky)) => AwbwTerrainId(87),
                Property::Airport(Faction::Player(PlayerFaction::GreySky)) => AwbwTerrainId(88),
                Property::Port(Faction::Player(PlayerFaction::GreySky)) => AwbwTerrainId(89),
                Property::HQ(PlayerFaction::GreySky) => AwbwTerrainId(90),

                // Black Hole properties
                Property::City(Faction::Player(PlayerFaction::BlackHole)) => AwbwTerrainId(91),
                Property::Base(Faction::Player(PlayerFaction::BlackHole)) => AwbwTerrainId(92),
                Property::Airport(Faction::Player(PlayerFaction::BlackHole)) => AwbwTerrainId(93),
                Property::Port(Faction::Player(PlayerFaction::BlackHole)) => AwbwTerrainId(94),
                Property::HQ(PlayerFaction::BlackHole) => AwbwTerrainId(95),

                // Brown Desert properties
                Property::City(Faction::Player(PlayerFaction::BrownDesert)) => AwbwTerrainId(96),
                Property::Base(Faction::Player(PlayerFaction::BrownDesert)) => AwbwTerrainId(97),
                Property::Airport(Faction::Player(PlayerFaction::BrownDesert)) => AwbwTerrainId(98),
                Property::Port(Faction::Player(PlayerFaction::BrownDesert)) => AwbwTerrainId(99),
                Property::HQ(PlayerFaction::BrownDesert) => AwbwTerrainId(100),

                // Amber Blaze properties
                Property::Airport(Faction::Player(PlayerFaction::AmberBlaze)) => AwbwTerrainId(117),
                Property::Base(Faction::Player(PlayerFaction::AmberBlaze)) => AwbwTerrainId(118),
                Property::City(Faction::Player(PlayerFaction::AmberBlaze)) => AwbwTerrainId(119),
                Property::HQ(PlayerFaction::AmberBlaze) => AwbwTerrainId(120),
                Property::Port(Faction::Player(PlayerFaction::AmberBlaze)) => AwbwTerrainId(121),

                // Jade Sun properties
                Property::Airport(Faction::Player(PlayerFaction::JadeSun)) => AwbwTerrainId(122),
                Property::Base(Faction::Player(PlayerFaction::JadeSun)) => AwbwTerrainId(123),
                Property::City(Faction::Player(PlayerFaction::JadeSun)) => AwbwTerrainId(124),
                Property::HQ(PlayerFaction::JadeSun) => AwbwTerrainId(125),
                Property::Port(Faction::Player(PlayerFaction::JadeSun)) => AwbwTerrainId(126),

                // Com Towers
                Property::ComTower(Faction::Player(PlayerFaction::AmberBlaze)) => {
                    AwbwTerrainId(127)
                }
                Property::ComTower(Faction::Player(PlayerFaction::BlackHole)) => AwbwTerrainId(128),
                Property::ComTower(Faction::Player(PlayerFaction::BlueMoon)) => AwbwTerrainId(129),
                Property::ComTower(Faction::Player(PlayerFaction::BrownDesert)) => {
                    AwbwTerrainId(130)
                }
                Property::ComTower(Faction::Player(PlayerFaction::GreenEarth)) => {
                    AwbwTerrainId(131)
                }
                Property::ComTower(Faction::Player(PlayerFaction::JadeSun)) => AwbwTerrainId(132),
                Property::ComTower(Faction::Neutral) => AwbwTerrainId(133),
                Property::ComTower(Faction::Player(PlayerFaction::OrangeStar)) => {
                    AwbwTerrainId(134)
                }
                Property::ComTower(Faction::Player(PlayerFaction::RedFire)) => AwbwTerrainId(135),
                Property::ComTower(Faction::Player(PlayerFaction::YellowComet)) => {
                    AwbwTerrainId(136)
                }
                Property::ComTower(Faction::Player(PlayerFaction::GreySky)) => AwbwTerrainId(137),

                // Labs
                Property::Lab(Faction::Player(PlayerFaction::AmberBlaze)) => AwbwTerrainId(138),
                Property::Lab(Faction::Player(PlayerFaction::BlackHole)) => AwbwTerrainId(139),
                Property::Lab(Faction::Player(PlayerFaction::BlueMoon)) => AwbwTerrainId(140),
                Property::Lab(Faction::Player(PlayerFaction::BrownDesert)) => AwbwTerrainId(141),
                Property::Lab(Faction::Player(PlayerFaction::GreenEarth)) => AwbwTerrainId(142),
                Property::Lab(Faction::Player(PlayerFaction::GreySky)) => AwbwTerrainId(143),
                Property::Lab(Faction::Player(PlayerFaction::JadeSun)) => AwbwTerrainId(144),
                Property::Lab(Faction::Neutral) => AwbwTerrainId(145),
                Property::Lab(Faction::Player(PlayerFaction::OrangeStar)) => AwbwTerrainId(146),
                Property::Lab(Faction::Player(PlayerFaction::RedFire)) => AwbwTerrainId(147),
                Property::Lab(Faction::Player(PlayerFaction::YellowComet)) => AwbwTerrainId(148),

                // Cobalt Ice properties
                Property::Airport(Faction::Player(PlayerFaction::CobaltIce)) => AwbwTerrainId(149),
                Property::Base(Faction::Player(PlayerFaction::CobaltIce)) => AwbwTerrainId(150),
                Property::City(Faction::Player(PlayerFaction::CobaltIce)) => AwbwTerrainId(151),
                Property::ComTower(Faction::Player(PlayerFaction::CobaltIce)) => AwbwTerrainId(152),
                Property::HQ(PlayerFaction::CobaltIce) => AwbwTerrainId(153),
                Property::Lab(Faction::Player(PlayerFaction::CobaltIce)) => AwbwTerrainId(154),
                Property::Port(Faction::Player(PlayerFaction::CobaltIce)) => AwbwTerrainId(155),

                // Pink Cosmos properties
                Property::Airport(Faction::Player(PlayerFaction::PinkCosmos)) => AwbwTerrainId(156),
                Property::Base(Faction::Player(PlayerFaction::PinkCosmos)) => AwbwTerrainId(157),
                Property::City(Faction::Player(PlayerFaction::PinkCosmos)) => AwbwTerrainId(158),
                Property::ComTower(Faction::Player(PlayerFaction::PinkCosmos)) => {
                    AwbwTerrainId(159)
                }
                Property::HQ(PlayerFaction::PinkCosmos) => AwbwTerrainId(160),
                Property::Lab(Faction::Player(PlayerFaction::PinkCosmos)) => AwbwTerrainId(161),
                Property::Port(Faction::Player(PlayerFaction::PinkCosmos)) => AwbwTerrainId(162),

                // Teal Galaxy properties
                Property::Airport(Faction::Player(PlayerFaction::TealGalaxy)) => AwbwTerrainId(163),
                Property::Base(Faction::Player(PlayerFaction::TealGalaxy)) => AwbwTerrainId(164),
                Property::City(Faction::Player(PlayerFaction::TealGalaxy)) => AwbwTerrainId(165),
                Property::ComTower(Faction::Player(PlayerFaction::TealGalaxy)) => {
                    AwbwTerrainId(166)
                }
                Property::HQ(PlayerFaction::TealGalaxy) => AwbwTerrainId(167),
                Property::Lab(Faction::Player(PlayerFaction::TealGalaxy)) => AwbwTerrainId(168),
                Property::Port(Faction::Player(PlayerFaction::TealGalaxy)) => AwbwTerrainId(169),

                // Purple Lightning properties
                Property::Airport(Faction::Player(PlayerFaction::PurpleLightning)) => {
                    AwbwTerrainId(170)
                }
                Property::Base(Faction::Player(PlayerFaction::PurpleLightning)) => {
                    AwbwTerrainId(171)
                }
                Property::City(Faction::Player(PlayerFaction::PurpleLightning)) => {
                    AwbwTerrainId(172)
                }
                Property::ComTower(Faction::Player(PlayerFaction::PurpleLightning)) => {
                    AwbwTerrainId(173)
                }
                Property::HQ(PlayerFaction::PurpleLightning) => AwbwTerrainId(174),
                Property::Lab(Faction::Player(PlayerFaction::PurpleLightning)) => {
                    AwbwTerrainId(175)
                }
                Property::Port(Faction::Player(PlayerFaction::PurpleLightning)) => {
                    AwbwTerrainId(176)
                }

                // Acid Rain properties
                Property::Airport(Faction::Player(PlayerFaction::AcidRain)) => AwbwTerrainId(181),
                Property::Base(Faction::Player(PlayerFaction::AcidRain)) => AwbwTerrainId(182),
                Property::City(Faction::Player(PlayerFaction::AcidRain)) => AwbwTerrainId(183),
                Property::ComTower(Faction::Player(PlayerFaction::AcidRain)) => AwbwTerrainId(184),
                Property::HQ(PlayerFaction::AcidRain) => AwbwTerrainId(185),
                Property::Lab(Faction::Player(PlayerFaction::AcidRain)) => AwbwTerrainId(186),
                Property::Port(Faction::Player(PlayerFaction::AcidRain)) => AwbwTerrainId(187),

                // White Nova properties
                Property::Airport(Faction::Player(PlayerFaction::WhiteNova)) => AwbwTerrainId(188),
                Property::Base(Faction::Player(PlayerFaction::WhiteNova)) => AwbwTerrainId(189),
                Property::City(Faction::Player(PlayerFaction::WhiteNova)) => AwbwTerrainId(190),
                Property::ComTower(Faction::Player(PlayerFaction::WhiteNova)) => AwbwTerrainId(191),
                Property::HQ(PlayerFaction::WhiteNova) => AwbwTerrainId(192),
                Property::Lab(Faction::Player(PlayerFaction::WhiteNova)) => AwbwTerrainId(193),
                Property::Port(Faction::Player(PlayerFaction::WhiteNova)) => AwbwTerrainId(194),

                // Azure Asteroid properties
                Property::Airport(Faction::Player(PlayerFaction::AzureAsteroid)) => {
                    AwbwTerrainId(196)
                }
                Property::Base(Faction::Player(PlayerFaction::AzureAsteroid)) => AwbwTerrainId(197),
                Property::City(Faction::Player(PlayerFaction::AzureAsteroid)) => AwbwTerrainId(198),
                Property::ComTower(Faction::Player(PlayerFaction::AzureAsteroid)) => {
                    AwbwTerrainId(199)
                }
                Property::HQ(PlayerFaction::AzureAsteroid) => AwbwTerrainId(200),
                Property::Lab(Faction::Player(PlayerFaction::AzureAsteroid)) => AwbwTerrainId(201),
                Property::Port(Faction::Player(PlayerFaction::AzureAsteroid)) => AwbwTerrainId(202),

                // Noir Eclipse properties
                Property::Airport(Faction::Player(PlayerFaction::NoirEclipse)) => {
                    AwbwTerrainId(203)
                }
                Property::Base(Faction::Player(PlayerFaction::NoirEclipse)) => AwbwTerrainId(204),
                Property::City(Faction::Player(PlayerFaction::NoirEclipse)) => AwbwTerrainId(205),
                Property::ComTower(Faction::Player(PlayerFaction::NoirEclipse)) => {
                    AwbwTerrainId(206)
                }
                Property::HQ(PlayerFaction::NoirEclipse) => AwbwTerrainId(207),
                Property::Lab(Faction::Player(PlayerFaction::NoirEclipse)) => AwbwTerrainId(208),
                Property::Port(Faction::Player(PlayerFaction::NoirEclipse)) => AwbwTerrainId(209),

                // Silver Claw properties
                Property::Airport(Faction::Player(PlayerFaction::SilverClaw)) => AwbwTerrainId(210),
                Property::Base(Faction::Player(PlayerFaction::SilverClaw)) => AwbwTerrainId(211),
                Property::City(Faction::Player(PlayerFaction::SilverClaw)) => AwbwTerrainId(212),
                Property::ComTower(Faction::Player(PlayerFaction::SilverClaw)) => {
                    AwbwTerrainId(213)
                }
                Property::HQ(PlayerFaction::SilverClaw) => AwbwTerrainId(214),
                Property::Lab(Faction::Player(PlayerFaction::SilverClaw)) => AwbwTerrainId(215),
                Property::Port(Faction::Player(PlayerFaction::SilverClaw)) => AwbwTerrainId(216),
            },

            // Pipes
            AwbwTerrain::Pipe(pipe_type) => match pipe_type {
                PipeType::Vertical => AwbwTerrainId(101),
                PipeType::Horizontal => AwbwTerrainId(102),
                PipeType::NE => AwbwTerrainId(103),
                PipeType::ES => AwbwTerrainId(104),
                PipeType::SW => AwbwTerrainId(105),
                PipeType::WN => AwbwTerrainId(106),
                PipeType::NorthEnd => AwbwTerrainId(107),
                PipeType::EastEnd => AwbwTerrainId(108),
                PipeType::SouthEnd => AwbwTerrainId(109),
                PipeType::WestEnd => AwbwTerrainId(110),
            },

            // Missile Silos
            AwbwTerrain::MissileSilo(status) => match status {
                MissileSiloStatus::Loaded => AwbwTerrainId(111),
                MissileSiloStatus::Unloaded => AwbwTerrainId(112),
            },

            // Pipe Seams
            AwbwTerrain::PipeSeam(pipe_seam_type) => match pipe_seam_type {
                PipeSeamType::Horizontal => AwbwTerrainId(113),
                PipeSeamType::Vertical => AwbwTerrainId(114),
            },

            // Pipe Rubble
            AwbwTerrain::PipeRubble(pipe_rubble_type) => match pipe_rubble_type {
                PipeRubbleType::Horizontal => AwbwTerrainId(115),
                PipeRubbleType::Vertical => AwbwTerrainId(116),
            },

            // Teleporter
            AwbwTerrain::Teleporter => AwbwTerrainId(195),
        }
    }
}

/// Error type for converting from u8 to terrain types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TryFromTerrainError {
    InvalidId(u8),
}

impl fmt::Display for TryFromTerrainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TryFromTerrainError::InvalidId(id) => write!(f, "Invalid terrain ID: {}", id),
        }
    }
}

impl std::error::Error for TryFromTerrainError {}

impl TryFromTerrainError {
    pub fn invalid_id(&self) -> u8 {
        match self {
            TryFromTerrainError::InvalidId(id) => *id,
        }
    }
}

impl TryFrom<u8> for AwbwTerrain {
    type Error = TryFromTerrainError;

    fn try_from(id: u8) -> Result<Self, Self::Error> {
        match id {
            // Basic terrains
            1 => Ok(AwbwTerrain::Plain),
            2 => Ok(AwbwTerrain::Mountain),
            3 => Ok(AwbwTerrain::Wood),

            // Rivers
            4 => Ok(AwbwTerrain::River(RiverType::Horizontal)),
            5 => Ok(AwbwTerrain::River(RiverType::Vertical)),
            6 => Ok(AwbwTerrain::River(RiverType::Cross)),
            7 => Ok(AwbwTerrain::River(RiverType::ES)),
            8 => Ok(AwbwTerrain::River(RiverType::SW)),
            9 => Ok(AwbwTerrain::River(RiverType::WN)),
            10 => Ok(AwbwTerrain::River(RiverType::NE)),
            11 => Ok(AwbwTerrain::River(RiverType::ESW)),
            12 => Ok(AwbwTerrain::River(RiverType::SWN)),
            13 => Ok(AwbwTerrain::River(RiverType::WNE)),
            14 => Ok(AwbwTerrain::River(RiverType::NES)),

            // Roads
            15 => Ok(AwbwTerrain::Road(RoadType::Horizontal)),
            16 => Ok(AwbwTerrain::Road(RoadType::Vertical)),
            17 => Ok(AwbwTerrain::Road(RoadType::Cross)),
            18 => Ok(AwbwTerrain::Road(RoadType::ES)),
            19 => Ok(AwbwTerrain::Road(RoadType::SW)),
            20 => Ok(AwbwTerrain::Road(RoadType::WN)),
            21 => Ok(AwbwTerrain::Road(RoadType::NE)),
            22 => Ok(AwbwTerrain::Road(RoadType::ESW)),
            23 => Ok(AwbwTerrain::Road(RoadType::SWN)),
            24 => Ok(AwbwTerrain::Road(RoadType::WNE)),
            25 => Ok(AwbwTerrain::Road(RoadType::NES)),

            // Bridges
            26 => Ok(AwbwTerrain::Bridge(BridgeType::Horizontal)),
            27 => Ok(AwbwTerrain::Bridge(BridgeType::Vertical)),

            // Sea
            28 => Ok(AwbwTerrain::Sea),

            // Shoals
            29 => Ok(AwbwTerrain::Shoal(ShoalType::Horizontal)),
            30 => Ok(AwbwTerrain::Shoal(ShoalType::HorizontalNorth)),
            31 => Ok(AwbwTerrain::Shoal(ShoalType::Vertical)),
            32 => Ok(AwbwTerrain::Shoal(ShoalType::VerticalEast)),
            33 => Ok(AwbwTerrain::Reef),

            // Properties
            34 => Ok(AwbwTerrain::Property(Property::City(Faction::Neutral))),
            35 => Ok(AwbwTerrain::Property(Property::Base(Faction::Neutral))),
            36 => Ok(AwbwTerrain::Property(Property::Airport(Faction::Neutral))),
            37 => Ok(AwbwTerrain::Property(Property::Port(Faction::Neutral))),

            // Orange Star properties
            38 => Ok(AwbwTerrain::Property(Property::City(Faction::Player(
                PlayerFaction::OrangeStar,
            )))),
            39 => Ok(AwbwTerrain::Property(Property::Base(Faction::Player(
                PlayerFaction::OrangeStar,
            )))),
            40 => Ok(AwbwTerrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::OrangeStar,
            )))),
            41 => Ok(AwbwTerrain::Property(Property::Port(Faction::Player(
                PlayerFaction::OrangeStar,
            )))),
            42 => Ok(AwbwTerrain::Property(Property::HQ(
                PlayerFaction::OrangeStar,
            ))),

            // Blue Moon properties
            43 => Ok(AwbwTerrain::Property(Property::City(Faction::Player(
                PlayerFaction::BlueMoon,
            )))),
            44 => Ok(AwbwTerrain::Property(Property::Base(Faction::Player(
                PlayerFaction::BlueMoon,
            )))),
            45 => Ok(AwbwTerrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::BlueMoon,
            )))),
            46 => Ok(AwbwTerrain::Property(Property::Port(Faction::Player(
                PlayerFaction::BlueMoon,
            )))),
            47 => Ok(AwbwTerrain::Property(Property::HQ(PlayerFaction::BlueMoon))),

            // Green Earth properties
            48 => Ok(AwbwTerrain::Property(Property::City(Faction::Player(
                PlayerFaction::GreenEarth,
            )))),
            49 => Ok(AwbwTerrain::Property(Property::Base(Faction::Player(
                PlayerFaction::GreenEarth,
            )))),
            50 => Ok(AwbwTerrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::GreenEarth,
            )))),
            51 => Ok(AwbwTerrain::Property(Property::Port(Faction::Player(
                PlayerFaction::GreenEarth,
            )))),
            52 => Ok(AwbwTerrain::Property(Property::HQ(
                PlayerFaction::GreenEarth,
            ))),

            // Yellow Comet properties
            53 => Ok(AwbwTerrain::Property(Property::City(Faction::Player(
                PlayerFaction::YellowComet,
            )))),
            54 => Ok(AwbwTerrain::Property(Property::Base(Faction::Player(
                PlayerFaction::YellowComet,
            )))),
            55 => Ok(AwbwTerrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::YellowComet,
            )))),
            56 => Ok(AwbwTerrain::Property(Property::Port(Faction::Player(
                PlayerFaction::YellowComet,
            )))),
            57 => Ok(AwbwTerrain::Property(Property::HQ(
                PlayerFaction::YellowComet,
            ))),

            // Red Fire properties
            81 => Ok(AwbwTerrain::Property(Property::City(Faction::Player(
                PlayerFaction::RedFire,
            )))),
            82 => Ok(AwbwTerrain::Property(Property::Base(Faction::Player(
                PlayerFaction::RedFire,
            )))),
            83 => Ok(AwbwTerrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::RedFire,
            )))),
            84 => Ok(AwbwTerrain::Property(Property::Port(Faction::Player(
                PlayerFaction::RedFire,
            )))),
            85 => Ok(AwbwTerrain::Property(Property::HQ(PlayerFaction::RedFire))),

            // Grey Sky properties
            86 => Ok(AwbwTerrain::Property(Property::City(Faction::Player(
                PlayerFaction::GreySky,
            )))),
            87 => Ok(AwbwTerrain::Property(Property::Base(Faction::Player(
                PlayerFaction::GreySky,
            )))),
            88 => Ok(AwbwTerrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::GreySky,
            )))),
            89 => Ok(AwbwTerrain::Property(Property::Port(Faction::Player(
                PlayerFaction::GreySky,
            )))),
            90 => Ok(AwbwTerrain::Property(Property::HQ(PlayerFaction::GreySky))),

            // Black Hole properties
            91 => Ok(AwbwTerrain::Property(Property::City(Faction::Player(
                PlayerFaction::BlackHole,
            )))),
            92 => Ok(AwbwTerrain::Property(Property::Base(Faction::Player(
                PlayerFaction::BlackHole,
            )))),
            93 => Ok(AwbwTerrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::BlackHole,
            )))),
            94 => Ok(AwbwTerrain::Property(Property::Port(Faction::Player(
                PlayerFaction::BlackHole,
            )))),
            95 => Ok(AwbwTerrain::Property(Property::HQ(
                PlayerFaction::BlackHole,
            ))),

            // Brown Desert properties
            96 => Ok(AwbwTerrain::Property(Property::City(Faction::Player(
                PlayerFaction::BrownDesert,
            )))),
            97 => Ok(AwbwTerrain::Property(Property::Base(Faction::Player(
                PlayerFaction::BrownDesert,
            )))),
            98 => Ok(AwbwTerrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::BrownDesert,
            )))),
            99 => Ok(AwbwTerrain::Property(Property::Port(Faction::Player(
                PlayerFaction::BrownDesert,
            )))),
            100 => Ok(AwbwTerrain::Property(Property::HQ(
                PlayerFaction::BrownDesert,
            ))),

            // Pipes
            101 => Ok(AwbwTerrain::Pipe(PipeType::Vertical)),
            102 => Ok(AwbwTerrain::Pipe(PipeType::Horizontal)),
            103 => Ok(AwbwTerrain::Pipe(PipeType::NE)),
            104 => Ok(AwbwTerrain::Pipe(PipeType::ES)),
            105 => Ok(AwbwTerrain::Pipe(PipeType::SW)),
            106 => Ok(AwbwTerrain::Pipe(PipeType::WN)),
            107 => Ok(AwbwTerrain::Pipe(PipeType::NorthEnd)),
            108 => Ok(AwbwTerrain::Pipe(PipeType::EastEnd)),
            109 => Ok(AwbwTerrain::Pipe(PipeType::SouthEnd)),
            110 => Ok(AwbwTerrain::Pipe(PipeType::WestEnd)),

            // Missile Silos
            111 => Ok(AwbwTerrain::MissileSilo(MissileSiloStatus::Loaded)),
            112 => Ok(AwbwTerrain::MissileSilo(MissileSiloStatus::Unloaded)),

            // Pipe Seams
            113 => Ok(AwbwTerrain::PipeSeam(PipeSeamType::Horizontal)),
            114 => Ok(AwbwTerrain::PipeSeam(PipeSeamType::Vertical)),

            // Pipe Rubble
            115 => Ok(AwbwTerrain::PipeRubble(PipeRubbleType::Horizontal)),
            116 => Ok(AwbwTerrain::PipeRubble(PipeRubbleType::Vertical)),

            // Amber Blaze properties
            117 => Ok(AwbwTerrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::AmberBlaze,
            )))),
            118 => Ok(AwbwTerrain::Property(Property::Base(Faction::Player(
                PlayerFaction::AmberBlaze,
            )))),
            119 => Ok(AwbwTerrain::Property(Property::City(Faction::Player(
                PlayerFaction::AmberBlaze,
            )))),
            120 => Ok(AwbwTerrain::Property(Property::HQ(
                PlayerFaction::AmberBlaze,
            ))),
            121 => Ok(AwbwTerrain::Property(Property::Port(Faction::Player(
                PlayerFaction::AmberBlaze,
            )))),

            // Jade Sun properties
            122 => Ok(AwbwTerrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::JadeSun,
            )))),
            123 => Ok(AwbwTerrain::Property(Property::Base(Faction::Player(
                PlayerFaction::JadeSun,
            )))),
            124 => Ok(AwbwTerrain::Property(Property::City(Faction::Player(
                PlayerFaction::JadeSun,
            )))),
            125 => Ok(AwbwTerrain::Property(Property::HQ(PlayerFaction::JadeSun))),
            126 => Ok(AwbwTerrain::Property(Property::Port(Faction::Player(
                PlayerFaction::JadeSun,
            )))),

            // Com Towers
            127 => Ok(AwbwTerrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::AmberBlaze,
            )))),
            128 => Ok(AwbwTerrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::BlackHole,
            )))),
            129 => Ok(AwbwTerrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::BlueMoon,
            )))),
            130 => Ok(AwbwTerrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::BrownDesert,
            )))),
            131 => Ok(AwbwTerrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::GreenEarth,
            )))),
            132 => Ok(AwbwTerrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::JadeSun,
            )))),
            133 => Ok(AwbwTerrain::Property(Property::ComTower(Faction::Neutral))),
            134 => Ok(AwbwTerrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::OrangeStar,
            )))),
            135 => Ok(AwbwTerrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::RedFire,
            )))),
            136 => Ok(AwbwTerrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::YellowComet,
            )))),
            137 => Ok(AwbwTerrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::GreySky,
            )))),

            // Labs
            138 => Ok(AwbwTerrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::AmberBlaze,
            )))),
            139 => Ok(AwbwTerrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::BlackHole,
            )))),
            140 => Ok(AwbwTerrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::BlueMoon,
            )))),
            141 => Ok(AwbwTerrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::BrownDesert,
            )))),
            142 => Ok(AwbwTerrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::GreenEarth,
            )))),
            143 => Ok(AwbwTerrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::GreySky,
            )))),
            144 => Ok(AwbwTerrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::JadeSun,
            )))),
            145 => Ok(AwbwTerrain::Property(Property::Lab(Faction::Neutral))),
            146 => Ok(AwbwTerrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::OrangeStar,
            )))),
            147 => Ok(AwbwTerrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::RedFire,
            )))),
            148 => Ok(AwbwTerrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::YellowComet,
            )))),

            // Cobalt Ice properties
            149 => Ok(AwbwTerrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::CobaltIce,
            )))),
            150 => Ok(AwbwTerrain::Property(Property::Base(Faction::Player(
                PlayerFaction::CobaltIce,
            )))),
            151 => Ok(AwbwTerrain::Property(Property::City(Faction::Player(
                PlayerFaction::CobaltIce,
            )))),
            152 => Ok(AwbwTerrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::CobaltIce,
            )))),
            153 => Ok(AwbwTerrain::Property(Property::HQ(
                PlayerFaction::CobaltIce,
            ))),
            154 => Ok(AwbwTerrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::CobaltIce,
            )))),
            155 => Ok(AwbwTerrain::Property(Property::Port(Faction::Player(
                PlayerFaction::CobaltIce,
            )))),

            // Pink Cosmos properties
            156 => Ok(AwbwTerrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::PinkCosmos,
            )))),
            157 => Ok(AwbwTerrain::Property(Property::Base(Faction::Player(
                PlayerFaction::PinkCosmos,
            )))),
            158 => Ok(AwbwTerrain::Property(Property::City(Faction::Player(
                PlayerFaction::PinkCosmos,
            )))),
            159 => Ok(AwbwTerrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::PinkCosmos,
            )))),
            160 => Ok(AwbwTerrain::Property(Property::HQ(
                PlayerFaction::PinkCosmos,
            ))),
            161 => Ok(AwbwTerrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::PinkCosmos,
            )))),
            162 => Ok(AwbwTerrain::Property(Property::Port(Faction::Player(
                PlayerFaction::PinkCosmos,
            )))),

            // Teal Galaxy properties
            163 => Ok(AwbwTerrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::TealGalaxy,
            )))),
            164 => Ok(AwbwTerrain::Property(Property::Base(Faction::Player(
                PlayerFaction::TealGalaxy,
            )))),
            165 => Ok(AwbwTerrain::Property(Property::City(Faction::Player(
                PlayerFaction::TealGalaxy,
            )))),
            166 => Ok(AwbwTerrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::TealGalaxy,
            )))),
            167 => Ok(AwbwTerrain::Property(Property::HQ(
                PlayerFaction::TealGalaxy,
            ))),
            168 => Ok(AwbwTerrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::TealGalaxy,
            )))),
            169 => Ok(AwbwTerrain::Property(Property::Port(Faction::Player(
                PlayerFaction::TealGalaxy,
            )))),

            // Purple Lightning properties
            170 => Ok(AwbwTerrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::PurpleLightning,
            )))),
            171 => Ok(AwbwTerrain::Property(Property::Base(Faction::Player(
                PlayerFaction::PurpleLightning,
            )))),
            172 => Ok(AwbwTerrain::Property(Property::City(Faction::Player(
                PlayerFaction::PurpleLightning,
            )))),
            173 => Ok(AwbwTerrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::PurpleLightning,
            )))),
            174 => Ok(AwbwTerrain::Property(Property::HQ(
                PlayerFaction::PurpleLightning,
            ))),
            175 => Ok(AwbwTerrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::PurpleLightning,
            )))),
            176 => Ok(AwbwTerrain::Property(Property::Port(Faction::Player(
                PlayerFaction::PurpleLightning,
            )))),

            // Teleporter
            195 => Ok(AwbwTerrain::Teleporter),

            // Acid Rain properties
            181 => Ok(AwbwTerrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::AcidRain,
            )))),
            182 => Ok(AwbwTerrain::Property(Property::Base(Faction::Player(
                PlayerFaction::AcidRain,
            )))),
            183 => Ok(AwbwTerrain::Property(Property::City(Faction::Player(
                PlayerFaction::AcidRain,
            )))),
            184 => Ok(AwbwTerrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::AcidRain,
            )))),
            185 => Ok(AwbwTerrain::Property(Property::HQ(PlayerFaction::AcidRain))),
            186 => Ok(AwbwTerrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::AcidRain,
            )))),
            187 => Ok(AwbwTerrain::Property(Property::Port(Faction::Player(
                PlayerFaction::AcidRain,
            )))),

            // White Nova properties
            188 => Ok(AwbwTerrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::WhiteNova,
            )))),
            189 => Ok(AwbwTerrain::Property(Property::Base(Faction::Player(
                PlayerFaction::WhiteNova,
            )))),
            190 => Ok(AwbwTerrain::Property(Property::City(Faction::Player(
                PlayerFaction::WhiteNova,
            )))),
            191 => Ok(AwbwTerrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::WhiteNova,
            )))),
            192 => Ok(AwbwTerrain::Property(Property::HQ(
                PlayerFaction::WhiteNova,
            ))),
            193 => Ok(AwbwTerrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::WhiteNova,
            )))),
            194 => Ok(AwbwTerrain::Property(Property::Port(Faction::Player(
                PlayerFaction::WhiteNova,
            )))),

            // Azure Asteroid properties
            196 => Ok(AwbwTerrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::AzureAsteroid,
            )))),
            197 => Ok(AwbwTerrain::Property(Property::Base(Faction::Player(
                PlayerFaction::AzureAsteroid,
            )))),
            198 => Ok(AwbwTerrain::Property(Property::City(Faction::Player(
                PlayerFaction::AzureAsteroid,
            )))),
            199 => Ok(AwbwTerrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::AzureAsteroid,
            )))),
            200 => Ok(AwbwTerrain::Property(Property::HQ(
                PlayerFaction::AzureAsteroid,
            ))),
            201 => Ok(AwbwTerrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::AzureAsteroid,
            )))),
            202 => Ok(AwbwTerrain::Property(Property::Port(Faction::Player(
                PlayerFaction::AzureAsteroid,
            )))),

            // Noir Eclipse properties
            203 => Ok(AwbwTerrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::NoirEclipse,
            )))),
            204 => Ok(AwbwTerrain::Property(Property::Base(Faction::Player(
                PlayerFaction::NoirEclipse,
            )))),
            205 => Ok(AwbwTerrain::Property(Property::City(Faction::Player(
                PlayerFaction::NoirEclipse,
            )))),
            206 => Ok(AwbwTerrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::NoirEclipse,
            )))),
            207 => Ok(AwbwTerrain::Property(Property::HQ(
                PlayerFaction::NoirEclipse,
            ))),
            208 => Ok(AwbwTerrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::NoirEclipse,
            )))),
            209 => Ok(AwbwTerrain::Property(Property::Port(Faction::Player(
                PlayerFaction::NoirEclipse,
            )))),

            // Silver Claw properties
            210 => Ok(AwbwTerrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::SilverClaw,
            )))),
            211 => Ok(AwbwTerrain::Property(Property::Base(Faction::Player(
                PlayerFaction::SilverClaw,
            )))),
            212 => Ok(AwbwTerrain::Property(Property::City(Faction::Player(
                PlayerFaction::SilverClaw,
            )))),
            213 => Ok(AwbwTerrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::SilverClaw,
            )))),
            214 => Ok(AwbwTerrain::Property(Property::HQ(
                PlayerFaction::SilverClaw,
            ))),
            215 => Ok(AwbwTerrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::SilverClaw,
            )))),
            216 => Ok(AwbwTerrain::Property(Property::Port(Faction::Player(
                PlayerFaction::SilverClaw,
            )))),
            _ => Err(TryFromTerrainError::InvalidId(id)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missile_silo_status() {
        // Test conversion from TerrainType to Terrain ID
        assert_eq!(
            AwbwTerrainId::from(AwbwTerrain::MissileSilo(MissileSiloStatus::Loaded)).0,
            111
        );
        assert_eq!(
            AwbwTerrainId::from(AwbwTerrain::MissileSilo(MissileSiloStatus::Unloaded)).0,
            112
        );

        // Test names
        assert_eq!(
            AwbwTerrain::MissileSilo(MissileSiloStatus::Loaded).name(),
            "Missile Silo"
        );
        assert_eq!(
            AwbwTerrain::MissileSilo(MissileSiloStatus::Unloaded).name(),
            "Missile Silo Empty"
        );
    }

    #[test]
    fn test_terrain_type_name() {
        // Test basic terrain names
        assert_eq!(AwbwTerrain::Plain.name(), "Plain");
        assert_eq!(AwbwTerrain::Mountain.name(), "Mountain");
        assert_eq!(AwbwTerrain::Wood.name(), "Wood");
        assert_eq!(AwbwTerrain::Sea.name(), "Sea");
        assert_eq!(AwbwTerrain::Reef.name(), "Reef");

        // Test river names
        assert_eq!(AwbwTerrain::River(RiverType::Horizontal).name(), "HRiver");
        assert_eq!(AwbwTerrain::River(RiverType::Vertical).name(), "VRiver");

        // Test road names
        assert_eq!(AwbwTerrain::Road(RoadType::Horizontal).name(), "HRoad");
        assert_eq!(AwbwTerrain::Road(RoadType::Vertical).name(), "VRoad");

        // Test bridge names
        assert_eq!(
            AwbwTerrain::Bridge(BridgeType::Horizontal).name(),
            "HBridge"
        );
        assert_eq!(AwbwTerrain::Bridge(BridgeType::Vertical).name(), "VBridge");

        // Test pipe names
        assert_eq!(AwbwTerrain::Pipe(PipeType::Horizontal).name(), "HPipe");
        assert_eq!(AwbwTerrain::Pipe(PipeType::Vertical).name(), "VPipe");
    }

    #[test]
    fn test_property_type_name() {
        // Test neutral properties
        assert_eq!(Property::City(Faction::Neutral).name(), "Neutral City");
        assert_eq!(Property::Base(Faction::Neutral).name(), "Neutral Base");
        assert_eq!(
            Property::Airport(Faction::Neutral).name(),
            "Neutral Airport"
        );
        assert_eq!(Property::Port(Faction::Neutral).name(), "Neutral Port");

        // Test player faction properties
        assert_eq!(
            Property::City(Faction::Player(PlayerFaction::OrangeStar)).name(),
            "Orange Star City"
        );
        assert_eq!(
            Property::Base(Faction::Player(PlayerFaction::BlueMoon)).name(),
            "Blue Moon Base"
        );
        assert_eq!(
            Property::Airport(Faction::Player(PlayerFaction::GreenEarth)).name(),
            "Green Earth Airport"
        );
        assert_eq!(
            Property::Port(Faction::Player(PlayerFaction::YellowComet)).name(),
            "Yellow Comet Port"
        );
        assert_eq!(
            Property::HQ(PlayerFaction::BlackHole).name(),
            "Black Hole HQ"
        );

        // Test that TerrainType correctly delegates to PropertyType.name()
        assert_eq!(
            AwbwTerrain::Property(Property::City(Faction::Player(PlayerFaction::OrangeStar)))
                .name(),
            "Orange Star City"
        );
        assert_eq!(
            AwbwTerrain::Property(Property::HQ(PlayerFaction::BlueMoon)).name(),
            "Blue Moon HQ"
        );
    }

    #[test]
    fn test_player_faction_name() {
        assert_eq!(PlayerFaction::OrangeStar.name(), "Orange Star");
        assert_eq!(PlayerFaction::BlueMoon.name(), "Blue Moon");
        assert_eq!(PlayerFaction::GreenEarth.name(), "Green Earth");
        assert_eq!(PlayerFaction::YellowComet.name(), "Yellow Comet");
        assert_eq!(PlayerFaction::BlackHole.name(), "Black Hole");
    }

    #[test]
    fn test_terrain_owner() {
        // Non-property terrains should have no owner
        assert_eq!(AwbwTerrain::Plain.owner(), None);
        assert_eq!(AwbwTerrain::Mountain.owner(), None);
        assert_eq!(AwbwTerrain::Sea.owner(), None);

        // Properties should have the correct owner
        assert_eq!(
            AwbwTerrain::Property(Property::City(Faction::Neutral)).owner(),
            Some(Faction::Neutral)
        );

        assert_eq!(
            AwbwTerrain::Property(Property::HQ(PlayerFaction::OrangeStar)).owner(),
            Some(Faction::Player(PlayerFaction::OrangeStar))
        );

        assert_eq!(
            AwbwTerrain::Property(Property::Airport(Faction::Player(PlayerFaction::BlueMoon)))
                .owner(),
            Some(Faction::Player(PlayerFaction::BlueMoon))
        );
    }

    #[test]
    fn test_terrain_is_hq() {
        // Only HQ properties should return true
        assert!(AwbwTerrain::Property(Property::HQ(PlayerFaction::OrangeStar)).is_hq());

        // Other properties should return false
        assert!(!AwbwTerrain::Property(Property::City(Faction::Neutral)).is_hq());
        assert!(
            !AwbwTerrain::Property(Property::Base(Faction::Player(PlayerFaction::BlueMoon)))
                .is_hq()
        );

        // Non-property terrains should return false
        assert!(!AwbwTerrain::Plain.is_hq());
        assert!(!AwbwTerrain::Mountain.is_hq());
        assert!(!AwbwTerrain::Sea.is_hq());
    }

    #[test]
    fn test_terrain_defense_stars() {
        // Test defense stars values
        assert_eq!(AwbwTerrain::Plain.defense_stars(), 1);
        assert_eq!(AwbwTerrain::Mountain.defense_stars(), 4);
        assert_eq!(AwbwTerrain::Wood.defense_stars(), 2);
        assert_eq!(AwbwTerrain::River(RiverType::Horizontal).defense_stars(), 0);
        assert_eq!(AwbwTerrain::Sea.defense_stars(), 0);

        // Properties should have correct defense values
        assert_eq!(
            AwbwTerrain::Property(Property::City(Faction::Neutral)).defense_stars(),
            3
        );
        assert_eq!(
            AwbwTerrain::Property(Property::HQ(PlayerFaction::OrangeStar)).defense_stars(),
            4
        );
    }

    #[test]
    fn test_terrain_is_land() {
        // Land terrains
        assert!(AwbwTerrain::Plain.is_land());
        assert!(AwbwTerrain::Mountain.is_land());
        assert!(AwbwTerrain::Wood.is_land());
        assert!(AwbwTerrain::Property(Property::City(Faction::Neutral)).is_land());

        // Non-land terrains
        assert!(!AwbwTerrain::River(RiverType::Horizontal).is_land());
        assert!(!AwbwTerrain::Sea.is_land());
    }

    #[test]
    fn test_terrain_is_sea() {
        // Sea terrains
        assert!(AwbwTerrain::Sea.is_sea());
        assert!(AwbwTerrain::Property(Property::Port(Faction::Neutral)).is_sea());

        // Non-sea terrains
        assert!(!AwbwTerrain::Plain.is_sea());
        assert!(!AwbwTerrain::Mountain.is_sea());
        assert!(!AwbwTerrain::River(RiverType::Horizontal).is_sea());
        assert!(!AwbwTerrain::Property(Property::City(Faction::Neutral)).is_sea());
    }

    #[test]
    fn test_terrain_is_capturable() {
        // Capturable terrains (properties)
        assert!(AwbwTerrain::Property(Property::City(Faction::Neutral)).is_capturable());
        assert!(
            AwbwTerrain::Property(Property::Base(Faction::Player(PlayerFaction::OrangeStar)))
                .is_capturable()
        );
        assert!(AwbwTerrain::Property(Property::HQ(PlayerFaction::BlueMoon)).is_capturable());

        // Non-capturable terrains
        assert!(!AwbwTerrain::Plain.is_capturable());
        assert!(!AwbwTerrain::Mountain.is_capturable());
        assert!(!AwbwTerrain::Sea.is_capturable());
    }

    #[test]
    fn test_terrain_symbol() {
        // Test symbols for various terrain types
        assert_eq!(AwbwTerrain::Plain.symbol(), Some('.'));
        assert_eq!(AwbwTerrain::Mountain.symbol(), Some('^'));
        assert_eq!(AwbwTerrain::Wood.symbol(), Some('@'));
        assert_eq!(AwbwTerrain::Sea.symbol(), Some(','));

        // Test symbols for properties
        assert_eq!(
            AwbwTerrain::Property(Property::City(Faction::Neutral)).symbol(),
            Some('a')
        );
        assert_eq!(
            AwbwTerrain::Property(Property::HQ(PlayerFaction::OrangeStar)).symbol(),
            Some('i')
        );
    }

    #[test]
    fn test_gameplay_terrain_type() {
        // Test that gameplay type correctly abstracts visual differences
        assert_eq!(
            AwbwTerrain::River(RiverType::Horizontal).gameplay_type(),
            AwbwTerrain::River(RiverType::Vertical).gameplay_type()
        );

        assert_eq!(
            AwbwTerrain::Road(RoadType::Horizontal).gameplay_type(),
            AwbwTerrain::Road(RoadType::Cross).gameplay_type()
        );

        // Test that gameplay type preserves property categories
        assert_eq!(
            AwbwTerrain::Property(Property::City(Faction::Neutral)).gameplay_type(),
            GameplayTerrain::Property(Property::City(Faction::Neutral))
        );

        assert_eq!(
            AwbwTerrain::Property(Property::HQ(PlayerFaction::OrangeStar)).gameplay_type(),
            GameplayTerrain::Property(Property::HQ(PlayerFaction::OrangeStar))
        );

        // Test that MissileSiloStatus is preserved
        assert_eq!(
            AwbwTerrain::MissileSilo(MissileSiloStatus::Loaded).gameplay_type(),
            GameplayTerrain::MissileSilo(MissileSiloStatus::Loaded)
        );

        assert_eq!(
            AwbwTerrain::MissileSilo(MissileSiloStatus::Unloaded).gameplay_type(),
            GameplayTerrain::MissileSilo(MissileSiloStatus::Unloaded)
        );
    }

    #[test]
    fn test_u8_to_terrain_conversion() {
        // Test conversion from u8 to Terrain directly
        assert_eq!(AwbwTerrain::try_from(1).unwrap(), AwbwTerrain::Plain);
        assert_eq!(AwbwTerrain::try_from(2).unwrap(), AwbwTerrain::Mountain);
        assert_eq!(AwbwTerrain::try_from(3).unwrap(), AwbwTerrain::Wood);

        // Test rivers
        assert_eq!(
            AwbwTerrain::try_from(4).unwrap(),
            AwbwTerrain::River(RiverType::Horizontal)
        );
        assert_eq!(
            AwbwTerrain::try_from(14).unwrap(),
            AwbwTerrain::River(RiverType::NES)
        );

        // Test properties
        assert_eq!(
            AwbwTerrain::try_from(34).unwrap(),
            AwbwTerrain::Property(Property::City(Faction::Neutral))
        );
        assert_eq!(
            AwbwTerrain::try_from(42).unwrap(),
            AwbwTerrain::Property(Property::HQ(PlayerFaction::OrangeStar))
        );

        // Test other terrain types
        assert_eq!(AwbwTerrain::try_from(28).unwrap(), AwbwTerrain::Sea);
        assert_eq!(
            AwbwTerrain::try_from(111).unwrap(),
            AwbwTerrain::MissileSilo(MissileSiloStatus::Loaded)
        );

        // Test invalid IDs - specific error types
        assert_eq!(
            AwbwTerrain::try_from(58).unwrap_err(),
            TryFromTerrainError::InvalidId(58)
        );

        // Test round trip conversion (u8 -> Terrain -> TerrainId -> u8)
        for id in [1, 2, 3, 28, 34, 42, 111, 195] {
            let terrain = AwbwTerrain::try_from(id).unwrap();
            let terrain_id = AwbwTerrainId::from(terrain);
            assert_eq!(terrain_id.0, id);
        }
    }

    #[test]
    fn test_terrain_deserialize() {
        use serde_json::from_str;

        // Test deserializing basic terrains
        assert_eq!(from_str::<AwbwTerrain>("1").unwrap(), AwbwTerrain::Plain);
        assert_eq!(from_str::<AwbwTerrain>("2").unwrap(), AwbwTerrain::Mountain);
        assert_eq!(from_str::<AwbwTerrain>("3").unwrap(), AwbwTerrain::Wood);
        assert_eq!(from_str::<AwbwTerrain>("28").unwrap(), AwbwTerrain::Sea);

        // Test deserializing properties
        assert_eq!(
            from_str::<AwbwTerrain>("34").unwrap(),
            AwbwTerrain::Property(Property::City(Faction::Neutral))
        );
        assert_eq!(
            from_str::<AwbwTerrain>("42").unwrap(),
            AwbwTerrain::Property(Property::HQ(PlayerFaction::OrangeStar))
        );

        // Test deserializing special terrains
        assert_eq!(
            from_str::<AwbwTerrain>("111").unwrap(),
            AwbwTerrain::MissileSilo(MissileSiloStatus::Loaded)
        );
        assert_eq!(
            from_str::<AwbwTerrain>("112").unwrap(),
            AwbwTerrain::MissileSilo(MissileSiloStatus::Unloaded)
        );

        // Test deserializing invalid values
        assert!(from_str::<AwbwTerrain>("0").is_err());
        assert!(from_str::<AwbwTerrain>("999").is_err());
    }

    #[test]
    fn test_terrain_deserialize_in_structs() {
        use serde::{Deserialize, Serialize};
        use serde_json::from_str;

        #[derive(Debug, Serialize, Deserialize, PartialEq)]
        struct TerrainCell {
            terrain: AwbwTerrain,
            x: i32,
            y: i32,
        }

        // Test as part of a struct
        let json = r#"{"terrain":28,"x":5,"y":10}"#;
        let cell: TerrainCell = from_str(json).unwrap();

        assert_eq!(cell.terrain, AwbwTerrain::Sea);
        assert_eq!(cell.x, 5);
        assert_eq!(cell.y, 10);

        // Test multiple terrains in an array
        let json = r#"[{"terrain":1,"x":0,"y":0},{"terrain":34,"x":1,"y":0}]"#;
        let cells: Vec<TerrainCell> = from_str(json).unwrap();

        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0].terrain, AwbwTerrain::Plain);
        assert_eq!(
            cells[1].terrain,
            AwbwTerrain::Property(Property::City(Faction::Neutral))
        );

        // Test error handling with invalid terrain
        let json = r#"{"terrain":999,"x":5,"y":10}"#;
        let result: Result<TerrainCell, _> = from_str(json);
        assert!(result.is_err());
    }
}
