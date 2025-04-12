use std::convert::TryFrom;
use std::fmt;

/// Main terrain type enum that categorizes terrain by its primary function
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Terrain {
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

impl Terrain {
    /// Get the symbol character for this terrain
    pub fn symbol(&self) -> Option<char> {
        match self {
            Terrain::Plain => Some('.'),
            Terrain::Mountain => Some('^'),
            Terrain::Wood => Some('@'),
            Terrain::River(RiverType::Horizontal) => Some('{'),
            Terrain::River(RiverType::Vertical) => Some('}'),
            Terrain::River(RiverType::Cross) => Some('~'),
            Terrain::River(RiverType::ES) => Some('I'),
            Terrain::River(RiverType::SW) => Some('J'),
            Terrain::River(RiverType::WN) => Some('K'),
            Terrain::River(RiverType::NE) => Some('L'),
            Terrain::River(RiverType::ESW) => Some('M'),
            Terrain::River(RiverType::SWN) => Some('N'),
            Terrain::River(RiverType::WNE) => Some('O'),
            Terrain::River(RiverType::NES) => Some('P'),
            Terrain::Road(RoadType::Horizontal) => Some('-'),
            Terrain::Road(RoadType::Vertical) => Some('='),
            Terrain::Road(RoadType::Cross) => Some('+'),
            Terrain::Road(RoadType::ES) => Some('A'),
            Terrain::Road(RoadType::SW) => Some('B'),
            Terrain::Road(RoadType::WN) => Some('C'),
            Terrain::Road(RoadType::NE) => Some('D'),
            Terrain::Road(RoadType::ESW) => Some('E'),
            Terrain::Road(RoadType::SWN) => Some('F'),
            Terrain::Road(RoadType::WNE) => Some('G'),
            Terrain::Road(RoadType::NES) => Some('H'),
            Terrain::Bridge(BridgeType::Horizontal) => Some('['),
            Terrain::Bridge(BridgeType::Vertical) => Some(']'),
            Terrain::Sea => Some(','),
            Terrain::Shoal(ShoalType::Horizontal) => Some('<'),
            Terrain::Shoal(ShoalType::HorizontalNorth) => Some('('),
            Terrain::Shoal(ShoalType::Vertical) => Some('>'),
            Terrain::Shoal(ShoalType::VerticalEast) => Some(')'),
            Terrain::Reef => Some('%'),
            Terrain::Property(Property::City(Faction::Neutral)) => Some('a'),
            Terrain::Property(Property::Base(Faction::Neutral)) => Some('b'),
            Terrain::Property(Property::Airport(Faction::Neutral)) => Some('c'),
            Terrain::Property(Property::Port(Faction::Neutral)) => Some('d'),
            Terrain::Property(Property::City(Faction::Player(PlayerFaction::OrangeStar))) => {
                Some('e')
            }
            Terrain::Property(Property::Base(Faction::Player(PlayerFaction::OrangeStar))) => {
                Some('f')
            }
            Terrain::Property(Property::Airport(Faction::Player(PlayerFaction::OrangeStar))) => {
                Some('g')
            }
            Terrain::Property(Property::Port(Faction::Player(PlayerFaction::OrangeStar))) => {
                Some('h')
            }
            Terrain::Property(Property::HQ(PlayerFaction::OrangeStar)) => Some('i'),
            Terrain::Property(Property::City(Faction::Player(PlayerFaction::BlueMoon))) => {
                Some('j')
            }
            Terrain::Property(Property::Base(Faction::Player(PlayerFaction::BlueMoon))) => {
                Some('l')
            }
            Terrain::Property(Property::Airport(Faction::Player(PlayerFaction::BlueMoon))) => {
                Some('m')
            }
            Terrain::Property(Property::Port(Faction::Player(PlayerFaction::BlueMoon))) => {
                Some('n')
            }
            Terrain::Property(Property::HQ(PlayerFaction::BlueMoon)) => Some('o'),
            Terrain::Property(Property::City(Faction::Player(PlayerFaction::GreenEarth))) => {
                Some('p')
            }
            Terrain::Property(Property::Base(Faction::Player(PlayerFaction::GreenEarth))) => {
                Some('q')
            }
            Terrain::Property(Property::Airport(Faction::Player(PlayerFaction::GreenEarth))) => {
                Some('r')
            }
            Terrain::Property(Property::Port(Faction::Player(PlayerFaction::GreenEarth))) => {
                Some('s')
            }
            Terrain::Property(Property::HQ(PlayerFaction::GreenEarth)) => Some('t'),
            Terrain::Property(Property::City(Faction::Player(PlayerFaction::YellowComet))) => {
                Some('u')
            }
            Terrain::Property(Property::Base(Faction::Player(PlayerFaction::YellowComet))) => {
                Some('v')
            }
            Terrain::Property(Property::Airport(Faction::Player(PlayerFaction::YellowComet))) => {
                Some('w')
            }
            Terrain::Property(Property::Port(Faction::Player(PlayerFaction::YellowComet))) => {
                Some('x')
            }
            Terrain::Property(Property::HQ(PlayerFaction::YellowComet)) => Some('y'),
            Terrain::Property(Property::City(Faction::Player(PlayerFaction::RedFire))) => Some('U'),
            Terrain::Property(Property::Base(Faction::Player(PlayerFaction::RedFire))) => Some('T'),
            Terrain::Property(Property::Airport(Faction::Player(PlayerFaction::RedFire))) => {
                Some('S')
            }
            Terrain::Property(Property::Port(Faction::Player(PlayerFaction::RedFire))) => Some('R'),
            Terrain::Property(Property::HQ(PlayerFaction::RedFire)) => Some('Q'),
            Terrain::Property(Property::City(Faction::Player(PlayerFaction::GreySky))) => Some('Z'),
            Terrain::Property(Property::Base(Faction::Player(PlayerFaction::GreySky))) => Some('Y'),
            Terrain::Property(Property::Airport(Faction::Player(PlayerFaction::GreySky))) => {
                Some('X')
            }
            Terrain::Property(Property::Port(Faction::Player(PlayerFaction::GreySky))) => Some('W'),
            Terrain::Property(Property::HQ(PlayerFaction::GreySky)) => Some('V'),
            Terrain::Property(Property::City(Faction::Player(PlayerFaction::BlackHole))) => {
                Some('1')
            }
            Terrain::Property(Property::Base(Faction::Player(PlayerFaction::BlackHole))) => {
                Some('2')
            }
            Terrain::Property(Property::Airport(Faction::Player(PlayerFaction::BlackHole))) => {
                Some('3')
            }
            Terrain::Property(Property::Port(Faction::Player(PlayerFaction::BlackHole))) => {
                Some('4')
            }
            Terrain::Property(Property::HQ(PlayerFaction::BlackHole)) => Some('5'),
            Terrain::Pipe(PipeType::Vertical) => Some('k'),
            Terrain::Pipe(PipeType::Horizontal) => Some('z'),
            Terrain::Pipe(PipeType::NE) => Some('!'),
            Terrain::Pipe(PipeType::ES) => Some('#'),
            Terrain::Pipe(PipeType::SW) => Some('$'),
            Terrain::Pipe(PipeType::WN) => Some('&'),
            Terrain::Pipe(PipeType::NorthEnd) => Some('*'),
            Terrain::Pipe(PipeType::EastEnd) => Some('|'),
            Terrain::Pipe(PipeType::SouthEnd) => Some('`'),
            Terrain::Pipe(PipeType::WestEnd) => Some('\''),
            Terrain::MissileSilo(MissileSiloStatus::Loaded) => Some('"'),
            Terrain::MissileSilo(MissileSiloStatus::Unloaded) => Some(';'),
            Terrain::PipeSeam(PipeSeamType::Horizontal) => Some(':'),
            Terrain::PipeSeam(PipeSeamType::Vertical) => Some('?'),
            Terrain::PipeRubble(PipeRubbleType::Horizontal) => Some('/'),
            Terrain::PipeRubble(PipeRubbleType::Vertical) => Some('0'),
            Terrain::Property(Property::ComTower(Faction::Neutral)) => Some('_'),
            Terrain::Property(Property::Lab(Faction::Neutral)) => Some('6'),
            _ => None,
        }
    }

    /// Get the name of this terrain
    pub const fn name(&self) -> &'static str {
        match self {
            // Basic terrains
            Terrain::Plain => "Plain",
            Terrain::Mountain => "Mountain",
            Terrain::Wood => "Wood",

            // Rivers
            Terrain::River(RiverType::Horizontal) => "HRiver",
            Terrain::River(RiverType::Vertical) => "VRiver",
            Terrain::River(RiverType::Cross) => "CRiver",
            Terrain::River(RiverType::ES) => "ESRiver",
            Terrain::River(RiverType::SW) => "SWRiver",
            Terrain::River(RiverType::WN) => "WNRiver",
            Terrain::River(RiverType::NE) => "NERiver",
            Terrain::River(RiverType::ESW) => "ESWRiver",
            Terrain::River(RiverType::SWN) => "SWNRiver",
            Terrain::River(RiverType::WNE) => "WNERiver",
            Terrain::River(RiverType::NES) => "NESRiver",

            // Roads
            Terrain::Road(RoadType::Horizontal) => "HRoad",
            Terrain::Road(RoadType::Vertical) => "VRoad",
            Terrain::Road(RoadType::Cross) => "CRoad",
            Terrain::Road(RoadType::ES) => "ESRoad",
            Terrain::Road(RoadType::SW) => "SWRoad",
            Terrain::Road(RoadType::WN) => "WNRoad",
            Terrain::Road(RoadType::NE) => "NERoad",
            Terrain::Road(RoadType::ESW) => "ESWRoad",
            Terrain::Road(RoadType::SWN) => "SWNRoad",
            Terrain::Road(RoadType::WNE) => "WNERoad",
            Terrain::Road(RoadType::NES) => "NESRoad",

            // Bridges
            Terrain::Bridge(BridgeType::Horizontal) => "HBridge",
            Terrain::Bridge(BridgeType::Vertical) => "VBridge",

            // Sea and coastal
            Terrain::Sea => "Sea",
            Terrain::Shoal(ShoalType::Horizontal) => "HShoal",
            Terrain::Shoal(ShoalType::HorizontalNorth) => "HShoalN",
            Terrain::Shoal(ShoalType::Vertical) => "VShoal",
            Terrain::Shoal(ShoalType::VerticalEast) => "VShoalE",
            Terrain::Reef => "Reef",

            Terrain::Property(x) => x.name(),

            // Pipes
            Terrain::Pipe(PipeType::Vertical) => "VPipe",
            Terrain::Pipe(PipeType::Horizontal) => "HPipe",
            Terrain::Pipe(PipeType::NE) => "NEPipe",
            Terrain::Pipe(PipeType::ES) => "ESPipe",
            Terrain::Pipe(PipeType::SW) => "SWPipe",
            Terrain::Pipe(PipeType::WN) => "WNPipe",
            Terrain::Pipe(PipeType::NorthEnd) => "NPipe End",
            Terrain::Pipe(PipeType::EastEnd) => "EPipe End",
            Terrain::Pipe(PipeType::SouthEnd) => "SPipe End",
            Terrain::Pipe(PipeType::WestEnd) => "WPipe End",

            // Missile silos
            Terrain::MissileSilo(MissileSiloStatus::Loaded) => "Missile Silo",
            Terrain::MissileSilo(MissileSiloStatus::Unloaded) => "Missile Silo Empty",

            // Pipe seams and rubble
            Terrain::PipeSeam(PipeSeamType::Horizontal) => "HPipe Seam",
            Terrain::PipeSeam(PipeSeamType::Vertical) => "VPipe Seam",
            Terrain::PipeRubble(PipeRubbleType::Horizontal) => "HPipe Rubble",
            Terrain::PipeRubble(PipeRubbleType::Vertical) => "VPipe Rubble",

            // Teleporter
            Terrain::Teleporter => "Teleporter",
        }
    }

    /// Get the faction that owns this property (if applicable)
    pub fn owner(&self) -> Option<Faction> {
        match self {
            Terrain::Property(property_type) => Some(match property_type {
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
        matches!(self, Terrain::Property(Property::HQ(_)))
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
                PropertyCategory::HQ(_) => 4,
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
            GameplayTerrain::Sea | GameplayTerrain::Property(PropertyCategory::Port(_))
        )
    }

    /// Check if terrain can be captured by infantry
    pub fn is_capturable(&self) -> bool {
        matches!(self, Terrain::Property(_))
    }

    /// Get the gameplay-relevant terrain type
    pub fn gameplay_type(&self) -> GameplayTerrain {
        match self {
            Terrain::Plain => GameplayTerrain::Plain,
            Terrain::Mountain => GameplayTerrain::Mountain,
            Terrain::Wood => GameplayTerrain::Wood,
            Terrain::River(_) => GameplayTerrain::River,
            Terrain::Road(_) => GameplayTerrain::Road,
            Terrain::Bridge(_) => GameplayTerrain::Bridge,
            Terrain::Sea => GameplayTerrain::Sea,
            Terrain::Shoal(_) => GameplayTerrain::Shoal,
            Terrain::Reef => GameplayTerrain::Reef,
            Terrain::Property(property) => GameplayTerrain::Property(match property {
                Property::City(faction) => PropertyCategory::City(*faction),
                Property::Base(faction) => PropertyCategory::Base(*faction),
                Property::Airport(faction) => PropertyCategory::Airport(*faction),
                Property::Port(faction) => PropertyCategory::Port(*faction),
                Property::HQ(faction) => PropertyCategory::HQ(*faction),
                Property::ComTower(faction) => PropertyCategory::ComTower(*faction),
                Property::Lab(faction) => PropertyCategory::Lab(*faction),
            }),
            Terrain::Pipe(_) => GameplayTerrain::Pipe,
            Terrain::PipeSeam(_) => GameplayTerrain::PipeSeam,
            Terrain::PipeRubble(_) => GameplayTerrain::PipeRubble,
            Terrain::MissileSilo(status) => GameplayTerrain::MissileSilo(*status),
            Terrain::Teleporter => GameplayTerrain::Teleporter,
        }
    }
}

/// Status of the missile silo
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MissileSiloStatus {
    Loaded,
    Unloaded,
}

/// River configurations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RiverType {
    Horizontal, // HRiver
    Vertical,   // VRiver
    Cross,      // CRiver
    ES,         // East-South
    SW,         // South-West
    WN,         // West-North
    NE,         // North-East
    ESW,        // East-South-West
    SWN,        // South-West-North
    WNE,        // West-North-East
    NES,        // North-East-South
}

/// Road configurations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RoadType {
    Horizontal, // HRoad
    Vertical,   // VRoad
    Cross,      // CRoad
    ES,         // East-South
    SW,         // South-West
    WN,         // West-North
    NE,         // North-East
    ESW,        // East-South-West
    SWN,        // South-West-North
    WNE,        // West-North-East
    NES,        // North-East-South
}

/// Bridge types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BridgeType {
    Horizontal,
    Vertical,
}

/// Shoal types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShoalType {
    Horizontal,
    HorizontalNorth,
    Vertical,
    VerticalEast,
}

/// Pipe configurations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PipeType {
    Vertical,
    Horizontal,
    NE,
    ES,
    SW,
    WN,
    NorthEnd,
    EastEnd,
    SouthEnd,
    WestEnd,
}

/// Pipe seam types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PipeSeamType {
    Horizontal,
    Vertical,
}

/// Pipe rubble types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PipeRubbleType {
    Horizontal,
    Vertical,
}

/// Property types combining building type and owner
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Property {
    // Regular properties that can be neutral
    City(Faction),
    Base(Faction),
    Airport(Faction),
    Port(Faction),
    ComTower(Faction),
    Lab(Faction),

    // HQ can never be neutral - must be owned by a specific faction
    HQ(PlayerFaction),
}

impl Property {
    /// Get the name of this property type
    pub const fn name(&self) -> &'static str {
        match self {
            Property::City(Faction::Neutral) => "Neutral City",
            Property::Base(Faction::Neutral) => "Neutral Base",
            Property::Airport(Faction::Neutral) => "Neutral Airport",
            Property::Port(Faction::Neutral) => "Neutral Port",
            Property::ComTower(Faction::Neutral) => "Neutral Com Tower",
            Property::Lab(Faction::Neutral) => "Neutral Lab",

            Property::City(Faction::Player(player_faction)) => match player_faction {
                PlayerFaction::OrangeStar => "Orange Star City",
                PlayerFaction::BlueMoon => "Blue Moon City",
                PlayerFaction::GreenEarth => "Green Earth City",
                PlayerFaction::YellowComet => "Yellow Comet City",
                PlayerFaction::BlackHole => "Black Hole City",
                PlayerFaction::RedFire => "Red Fire City",
                PlayerFaction::GreySky => "Grey Sky City",
                PlayerFaction::BrownDesert => "Brown Desert City",
                PlayerFaction::AmberBlaze => "Amber Blaze City",
                PlayerFaction::JadeSun => "Jade Sun City",
                PlayerFaction::CobaltIce => "Cobalt Ice City",
                PlayerFaction::PinkCosmos => "Pink Cosmos City",
                PlayerFaction::TealGalaxy => "Teal Galaxy City",
                PlayerFaction::PurpleLightning => "Purple Lightning City",
                PlayerFaction::AcidRain => "Acid Rain City",
                PlayerFaction::WhiteNova => "White Nova City",
                PlayerFaction::AzureAsteroid => "Azure Asteroid City",
                PlayerFaction::NoirEclipse => "Noir Eclipse City",
                PlayerFaction::SilverClaw => "Silver Claw City",
            },
            Property::Base(Faction::Player(player_faction)) => match player_faction {
                PlayerFaction::OrangeStar => "Orange Star Base",
                PlayerFaction::BlueMoon => "Blue Moon Base",
                PlayerFaction::GreenEarth => "Green Earth Base",
                PlayerFaction::YellowComet => "Yellow Comet Base",
                PlayerFaction::BlackHole => "Black Hole Base",
                PlayerFaction::RedFire => "Red Fire Base",
                PlayerFaction::GreySky => "Grey Sky Base",
                PlayerFaction::BrownDesert => "Brown Desert Base",
                PlayerFaction::AmberBlaze => "Amber Blaze Base",
                PlayerFaction::JadeSun => "Jade Sun Base",
                PlayerFaction::CobaltIce => "Cobalt Ice Base",
                PlayerFaction::PinkCosmos => "Pink Cosmos Base",
                PlayerFaction::TealGalaxy => "Teal Galaxy Base",
                PlayerFaction::PurpleLightning => "Purple Lightning Base",
                PlayerFaction::AcidRain => "Acid Rain Base",
                PlayerFaction::WhiteNova => "White Nova Base",
                PlayerFaction::AzureAsteroid => "Azure Asteroid Base",
                PlayerFaction::NoirEclipse => "Noir Eclipse Base",
                PlayerFaction::SilverClaw => "Silver Claw Base",
            },
            Property::Airport(Faction::Player(player_faction)) => match player_faction {
                PlayerFaction::OrangeStar => "Orange Star Airport",
                PlayerFaction::BlueMoon => "Blue Moon Airport",
                PlayerFaction::GreenEarth => "Green Earth Airport",
                PlayerFaction::YellowComet => "Yellow Comet Airport",
                PlayerFaction::BlackHole => "Black Hole Airport",
                PlayerFaction::RedFire => "Red Fire Airport",
                PlayerFaction::GreySky => "Grey Sky Airport",
                PlayerFaction::BrownDesert => "Brown Desert Airport",
                PlayerFaction::AmberBlaze => "Amber Blaze Airport",
                PlayerFaction::JadeSun => "Jade Sun Airport",
                PlayerFaction::CobaltIce => "Cobalt Ice Airport",
                PlayerFaction::PinkCosmos => "Pink Cosmos Airport",
                PlayerFaction::TealGalaxy => "Teal Galaxy Airport",
                PlayerFaction::PurpleLightning => "Purple Lightning Airport",
                PlayerFaction::AcidRain => "Acid Rain Airport",
                PlayerFaction::WhiteNova => "White Nova Airport",
                PlayerFaction::AzureAsteroid => "Azure Asteroid Airport",
                PlayerFaction::NoirEclipse => "Noir Eclipse Airport",
                PlayerFaction::SilverClaw => "Silver Claw Airport",
            },
            Property::Port(Faction::Player(player_faction)) => match player_faction {
                PlayerFaction::OrangeStar => "Orange Star Port",
                PlayerFaction::BlueMoon => "Blue Moon Port",
                PlayerFaction::GreenEarth => "Green Earth Port",
                PlayerFaction::YellowComet => "Yellow Comet Port",
                PlayerFaction::BlackHole => "Black Hole Port",
                PlayerFaction::RedFire => "Red Fire Port",
                PlayerFaction::GreySky => "Grey Sky Port",
                PlayerFaction::BrownDesert => "Brown Desert Port",
                PlayerFaction::AmberBlaze => "Amber Blaze Port",
                PlayerFaction::JadeSun => "Jade Sun Port",
                PlayerFaction::CobaltIce => "Cobalt Ice Port",
                PlayerFaction::PinkCosmos => "Pink Cosmos Port",
                PlayerFaction::TealGalaxy => "Teal Galaxy Port",
                PlayerFaction::PurpleLightning => "Purple Lightning Port",
                PlayerFaction::AcidRain => "Acid Rain Port",
                PlayerFaction::WhiteNova => "White Nova Port",
                PlayerFaction::AzureAsteroid => "Azure Asteroid Port",
                PlayerFaction::NoirEclipse => "Noir Eclipse Port",
                PlayerFaction::SilverClaw => "Silver Claw Port",
            },
            Property::ComTower(Faction::Player(player_faction)) => match player_faction {
                PlayerFaction::OrangeStar => "Orange Star Com Tower",
                PlayerFaction::BlueMoon => "Blue Moon Com Tower",
                PlayerFaction::GreenEarth => "Green Earth Com Tower",
                PlayerFaction::YellowComet => "Yellow Comet Com Tower",
                PlayerFaction::BlackHole => "Black Hole Com Tower",
                PlayerFaction::RedFire => "Red Fire Com Tower",
                PlayerFaction::GreySky => "Grey Sky Com Tower",
                PlayerFaction::BrownDesert => "Brown Desert Com Tower",
                PlayerFaction::AmberBlaze => "Amber Blaze Com Tower",
                PlayerFaction::JadeSun => "Jade Sun Com Tower",
                PlayerFaction::CobaltIce => "Cobalt Ice Com Tower",
                PlayerFaction::PinkCosmos => "Pink Cosmos Com Tower",
                PlayerFaction::TealGalaxy => "Teal Galaxy Com Tower",
                PlayerFaction::PurpleLightning => "Purple Lightning Com Tower",
                PlayerFaction::AcidRain => "Acid Rain Com Tower",
                PlayerFaction::WhiteNova => "White Nova Com Tower",
                PlayerFaction::AzureAsteroid => "Azure Asteroid Com Tower",
                PlayerFaction::NoirEclipse => "Noir Eclipse Com Tower",
                PlayerFaction::SilverClaw => "Silver Claw Com Tower",
            },
            Property::Lab(Faction::Player(player_faction)) => match player_faction {
                PlayerFaction::OrangeStar => "Orange Star Lab",
                PlayerFaction::BlueMoon => "Blue Moon Lab",
                PlayerFaction::GreenEarth => "Green Earth Lab",
                PlayerFaction::YellowComet => "Yellow Comet Lab",
                PlayerFaction::BlackHole => "Black Hole Lab",
                PlayerFaction::RedFire => "Red Fire Lab",
                PlayerFaction::GreySky => "Grey Sky Lab",
                PlayerFaction::BrownDesert => "Brown Desert Lab",
                PlayerFaction::AmberBlaze => "Amber Blaze Lab",
                PlayerFaction::JadeSun => "Jade Sun Lab",
                PlayerFaction::CobaltIce => "Cobalt Ice Lab",
                PlayerFaction::PinkCosmos => "Pink Cosmos Lab",
                PlayerFaction::TealGalaxy => "Teal Galaxy Lab",
                PlayerFaction::PurpleLightning => "Purple Lightning Lab",
                PlayerFaction::AcidRain => "Acid Rain Lab",
                PlayerFaction::WhiteNova => "White Nova Lab",
                PlayerFaction::AzureAsteroid => "Azure Asteroid Lab",
                PlayerFaction::NoirEclipse => "Noir Eclipse Lab",
                PlayerFaction::SilverClaw => "Silver Claw Lab",
            },
            Property::HQ(player_faction) => match player_faction {
                PlayerFaction::OrangeStar => "Orange Star HQ",
                PlayerFaction::BlueMoon => "Blue Moon HQ",
                PlayerFaction::GreenEarth => "Green Earth HQ",
                PlayerFaction::YellowComet => "Yellow Comet HQ",
                PlayerFaction::BlackHole => "Black Hole HQ",
                PlayerFaction::RedFire => "Red Fire HQ",
                PlayerFaction::GreySky => "Grey Sky HQ",
                PlayerFaction::BrownDesert => "Brown Desert HQ",
                PlayerFaction::AmberBlaze => "Amber Blaze HQ",
                PlayerFaction::JadeSun => "Jade Sun HQ",
                PlayerFaction::CobaltIce => "Cobalt Ice HQ",
                PlayerFaction::PinkCosmos => "Pink Cosmos HQ",
                PlayerFaction::TealGalaxy => "Teal Galaxy HQ",
                PlayerFaction::PurpleLightning => "Purple Lightning HQ",
                PlayerFaction::AcidRain => "Acid Rain HQ",
                PlayerFaction::WhiteNova => "White Nova HQ",
                PlayerFaction::AzureAsteroid => "Azure Asteroid HQ",
                PlayerFaction::NoirEclipse => "Noir Eclipse HQ",
                PlayerFaction::SilverClaw => "Silver Claw HQ",
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayerFaction {
    OrangeStar,
    BlueMoon,
    GreenEarth,
    YellowComet,
    BlackHole,
    RedFire,
    GreySky,
    BrownDesert,
    AmberBlaze,
    JadeSun,
    CobaltIce,
    PinkCosmos,
    TealGalaxy,
    PurpleLightning,
    AcidRain,
    WhiteNova,
    AzureAsteroid,
    NoirEclipse,
    SilverClaw,
}

impl PlayerFaction {
    /// Get the display name of this faction
    pub const fn name(&self) -> &'static str {
        match self {
            PlayerFaction::OrangeStar => "Orange Star",
            PlayerFaction::BlueMoon => "Blue Moon",
            PlayerFaction::GreenEarth => "Green Earth",
            PlayerFaction::YellowComet => "Yellow Comet",
            PlayerFaction::BlackHole => "Black Hole",
            PlayerFaction::RedFire => "Red Fire",
            PlayerFaction::GreySky => "Grey Sky",
            PlayerFaction::BrownDesert => "Brown Desert",
            PlayerFaction::AmberBlaze => "Amber Blaze",
            PlayerFaction::JadeSun => "Jade Sun",
            PlayerFaction::CobaltIce => "Cobalt Ice",
            PlayerFaction::PinkCosmos => "Pink Cosmos",
            PlayerFaction::TealGalaxy => "Teal Galaxy",
            PlayerFaction::PurpleLightning => "Purple Lightning",
            PlayerFaction::AcidRain => "Acid Rain",
            PlayerFaction::WhiteNova => "White Nova",
            PlayerFaction::AzureAsteroid => "Azure Asteroid",
            PlayerFaction::NoirEclipse => "Noir Eclipse",
            PlayerFaction::SilverClaw => "Silver Claw",
        }
    }
}

// Add conversion between NonNeutralFaction and ArmyFaction
impl From<PlayerFaction> for Faction {
    fn from(faction: PlayerFaction) -> Self {
        Faction::Player(faction)
    }
}

/// Army factions in the game
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Faction {
    Neutral,
    Player(PlayerFaction),
}

impl Faction {
    /// Get the display name of this faction
    pub const fn name(&self) -> &'static str {
        match self {
            Faction::Neutral => "Neutral",
            Faction::Player(faction) => faction.name(),
        }
    }
}

/// Newtype wrapper around u8 for terrain ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TerrainId(u8);

/// GameplayTerrain represents the terrain's gameplay characteristics,
/// abstracting away visual differences that don't affect mechanics
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameplayTerrain {
    Plain,
    Mountain,
    Wood,
    River,
    Road,
    Bridge,
    Sea,
    Shoal,
    Reef,
    Property(PropertyCategory),
    Pipe,
    PipeSeam,
    PipeRubble,
    MissileSilo(MissileSiloStatus),
    Teleporter,
}

/// PropertyCategory abstracts properties by their gameplay function
/// and preserves ownership information and domain constraints
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PropertyCategory {
    City(Faction),
    Base(Faction),
    Airport(Faction),
    Port(Faction),
    ComTower(Faction),
    Lab(Faction),
    // HQ can never be neutral - we enforce this with PlayerFaction
    HQ(PlayerFaction),
}

impl TerrainId {
    /// Create a new Terrain with the given ID
    pub fn new(id: u8) -> Self {
        Self(id)
    }
}

impl From<Terrain> for TerrainId {
    fn from(terrain_type: Terrain) -> Self {
        match terrain_type {
            Terrain::Plain => TerrainId(1),
            Terrain::Mountain => TerrainId(2),
            Terrain::Wood => TerrainId(3),

            // Rivers
            Terrain::River(river_type) => match river_type {
                RiverType::Horizontal => TerrainId(4),
                RiverType::Vertical => TerrainId(5),
                RiverType::Cross => TerrainId(6),
                RiverType::ES => TerrainId(7),
                RiverType::SW => TerrainId(8),
                RiverType::WN => TerrainId(9),
                RiverType::NE => TerrainId(10),
                RiverType::ESW => TerrainId(11),
                RiverType::SWN => TerrainId(12),
                RiverType::WNE => TerrainId(13),
                RiverType::NES => TerrainId(14),
            },

            // Roads
            Terrain::Road(road_type) => match road_type {
                RoadType::Horizontal => TerrainId(15),
                RoadType::Vertical => TerrainId(16),
                RoadType::Cross => TerrainId(17),
                RoadType::ES => TerrainId(18),
                RoadType::SW => TerrainId(19),
                RoadType::WN => TerrainId(20),
                RoadType::NE => TerrainId(21),
                RoadType::ESW => TerrainId(22),
                RoadType::SWN => TerrainId(23),
                RoadType::WNE => TerrainId(24),
                RoadType::NES => TerrainId(25),
            },

            // Bridges
            Terrain::Bridge(bridge_type) => match bridge_type {
                BridgeType::Horizontal => TerrainId(26),
                BridgeType::Vertical => TerrainId(27),
            },

            Terrain::Sea => TerrainId(28),

            // Shoals
            Terrain::Shoal(shoal_type) => match shoal_type {
                ShoalType::Horizontal => TerrainId(29),
                ShoalType::HorizontalNorth => TerrainId(30),
                ShoalType::Vertical => TerrainId(31),
                ShoalType::VerticalEast => TerrainId(32),
            },

            Terrain::Reef => TerrainId(33),

            // Properties
            Terrain::Property(property_type) => match property_type {
                // Neutral properties
                Property::City(Faction::Neutral) => TerrainId(34),
                Property::Base(Faction::Neutral) => TerrainId(35),
                Property::Airport(Faction::Neutral) => TerrainId(36),
                Property::Port(Faction::Neutral) => TerrainId(37),

                // Orange Star properties
                Property::City(Faction::Player(PlayerFaction::OrangeStar)) => TerrainId(38),
                Property::Base(Faction::Player(PlayerFaction::OrangeStar)) => TerrainId(39),
                Property::Airport(Faction::Player(PlayerFaction::OrangeStar)) => TerrainId(40),
                Property::Port(Faction::Player(PlayerFaction::OrangeStar)) => TerrainId(41),
                Property::HQ(PlayerFaction::OrangeStar) => TerrainId(42),

                // Blue Moon properties
                Property::City(Faction::Player(PlayerFaction::BlueMoon)) => TerrainId(43),
                Property::Base(Faction::Player(PlayerFaction::BlueMoon)) => TerrainId(44),
                Property::Airport(Faction::Player(PlayerFaction::BlueMoon)) => TerrainId(45),
                Property::Port(Faction::Player(PlayerFaction::BlueMoon)) => TerrainId(46),
                Property::HQ(PlayerFaction::BlueMoon) => TerrainId(47),

                // Green Earth properties
                Property::City(Faction::Player(PlayerFaction::GreenEarth)) => TerrainId(48),
                Property::Base(Faction::Player(PlayerFaction::GreenEarth)) => TerrainId(49),
                Property::Airport(Faction::Player(PlayerFaction::GreenEarth)) => TerrainId(50),
                Property::Port(Faction::Player(PlayerFaction::GreenEarth)) => TerrainId(51),
                Property::HQ(PlayerFaction::GreenEarth) => TerrainId(52),

                // Yellow Comet properties
                Property::City(Faction::Player(PlayerFaction::YellowComet)) => TerrainId(53),
                Property::Base(Faction::Player(PlayerFaction::YellowComet)) => TerrainId(54),
                Property::Airport(Faction::Player(PlayerFaction::YellowComet)) => TerrainId(55),
                Property::Port(Faction::Player(PlayerFaction::YellowComet)) => TerrainId(56),
                Property::HQ(PlayerFaction::YellowComet) => TerrainId(57),

                // Red Fire properties
                Property::City(Faction::Player(PlayerFaction::RedFire)) => TerrainId(81),
                Property::Base(Faction::Player(PlayerFaction::RedFire)) => TerrainId(82),
                Property::Airport(Faction::Player(PlayerFaction::RedFire)) => TerrainId(83),
                Property::Port(Faction::Player(PlayerFaction::RedFire)) => TerrainId(84),
                Property::HQ(PlayerFaction::RedFire) => TerrainId(85),

                // Grey Sky properties
                Property::City(Faction::Player(PlayerFaction::GreySky)) => TerrainId(86),
                Property::Base(Faction::Player(PlayerFaction::GreySky)) => TerrainId(87),
                Property::Airport(Faction::Player(PlayerFaction::GreySky)) => TerrainId(88),
                Property::Port(Faction::Player(PlayerFaction::GreySky)) => TerrainId(89),
                Property::HQ(PlayerFaction::GreySky) => TerrainId(90),

                // Black Hole properties
                Property::City(Faction::Player(PlayerFaction::BlackHole)) => TerrainId(91),
                Property::Base(Faction::Player(PlayerFaction::BlackHole)) => TerrainId(92),
                Property::Airport(Faction::Player(PlayerFaction::BlackHole)) => TerrainId(93),
                Property::Port(Faction::Player(PlayerFaction::BlackHole)) => TerrainId(94),
                Property::HQ(PlayerFaction::BlackHole) => TerrainId(95),

                // Brown Desert properties
                Property::City(Faction::Player(PlayerFaction::BrownDesert)) => TerrainId(96),
                Property::Base(Faction::Player(PlayerFaction::BrownDesert)) => TerrainId(97),
                Property::Airport(Faction::Player(PlayerFaction::BrownDesert)) => TerrainId(98),
                Property::Port(Faction::Player(PlayerFaction::BrownDesert)) => TerrainId(99),
                Property::HQ(PlayerFaction::BrownDesert) => TerrainId(100),

                // Amber Blaze properties
                Property::Airport(Faction::Player(PlayerFaction::AmberBlaze)) => TerrainId(117),
                Property::Base(Faction::Player(PlayerFaction::AmberBlaze)) => TerrainId(118),
                Property::City(Faction::Player(PlayerFaction::AmberBlaze)) => TerrainId(119),
                Property::HQ(PlayerFaction::AmberBlaze) => TerrainId(120),
                Property::Port(Faction::Player(PlayerFaction::AmberBlaze)) => TerrainId(121),

                // Jade Sun properties
                Property::Airport(Faction::Player(PlayerFaction::JadeSun)) => TerrainId(122),
                Property::Base(Faction::Player(PlayerFaction::JadeSun)) => TerrainId(123),
                Property::City(Faction::Player(PlayerFaction::JadeSun)) => TerrainId(124),
                Property::HQ(PlayerFaction::JadeSun) => TerrainId(125),
                Property::Port(Faction::Player(PlayerFaction::JadeSun)) => TerrainId(126),

                // Com Towers
                Property::ComTower(Faction::Player(PlayerFaction::AmberBlaze)) => TerrainId(127),
                Property::ComTower(Faction::Player(PlayerFaction::BlackHole)) => TerrainId(128),
                Property::ComTower(Faction::Player(PlayerFaction::BlueMoon)) => TerrainId(129),
                Property::ComTower(Faction::Player(PlayerFaction::BrownDesert)) => TerrainId(130),
                Property::ComTower(Faction::Player(PlayerFaction::GreenEarth)) => TerrainId(131),
                Property::ComTower(Faction::Player(PlayerFaction::JadeSun)) => TerrainId(132),
                Property::ComTower(Faction::Neutral) => TerrainId(133),
                Property::ComTower(Faction::Player(PlayerFaction::OrangeStar)) => TerrainId(134),
                Property::ComTower(Faction::Player(PlayerFaction::RedFire)) => TerrainId(135),
                Property::ComTower(Faction::Player(PlayerFaction::YellowComet)) => TerrainId(136),
                Property::ComTower(Faction::Player(PlayerFaction::GreySky)) => TerrainId(137),

                // Labs
                Property::Lab(Faction::Player(PlayerFaction::AmberBlaze)) => TerrainId(138),
                Property::Lab(Faction::Player(PlayerFaction::BlackHole)) => TerrainId(139),
                Property::Lab(Faction::Player(PlayerFaction::BlueMoon)) => TerrainId(140),
                Property::Lab(Faction::Player(PlayerFaction::BrownDesert)) => TerrainId(141),
                Property::Lab(Faction::Player(PlayerFaction::GreenEarth)) => TerrainId(142),
                Property::Lab(Faction::Player(PlayerFaction::GreySky)) => TerrainId(143),
                Property::Lab(Faction::Player(PlayerFaction::JadeSun)) => TerrainId(144),
                Property::Lab(Faction::Neutral) => TerrainId(145),
                Property::Lab(Faction::Player(PlayerFaction::OrangeStar)) => TerrainId(146),
                Property::Lab(Faction::Player(PlayerFaction::RedFire)) => TerrainId(147),
                Property::Lab(Faction::Player(PlayerFaction::YellowComet)) => TerrainId(148),

                // Cobalt Ice properties
                Property::Airport(Faction::Player(PlayerFaction::CobaltIce)) => TerrainId(149),
                Property::Base(Faction::Player(PlayerFaction::CobaltIce)) => TerrainId(150),
                Property::City(Faction::Player(PlayerFaction::CobaltIce)) => TerrainId(151),
                Property::ComTower(Faction::Player(PlayerFaction::CobaltIce)) => TerrainId(152),
                Property::HQ(PlayerFaction::CobaltIce) => TerrainId(153),
                Property::Lab(Faction::Player(PlayerFaction::CobaltIce)) => TerrainId(154),
                Property::Port(Faction::Player(PlayerFaction::CobaltIce)) => TerrainId(155),

                // Pink Cosmos properties
                Property::Airport(Faction::Player(PlayerFaction::PinkCosmos)) => TerrainId(156),
                Property::Base(Faction::Player(PlayerFaction::PinkCosmos)) => TerrainId(157),
                Property::City(Faction::Player(PlayerFaction::PinkCosmos)) => TerrainId(158),
                Property::ComTower(Faction::Player(PlayerFaction::PinkCosmos)) => TerrainId(159),
                Property::HQ(PlayerFaction::PinkCosmos) => TerrainId(160),
                Property::Lab(Faction::Player(PlayerFaction::PinkCosmos)) => TerrainId(161),
                Property::Port(Faction::Player(PlayerFaction::PinkCosmos)) => TerrainId(162),

                // Teal Galaxy properties
                Property::Airport(Faction::Player(PlayerFaction::TealGalaxy)) => TerrainId(163),
                Property::Base(Faction::Player(PlayerFaction::TealGalaxy)) => TerrainId(164),
                Property::City(Faction::Player(PlayerFaction::TealGalaxy)) => TerrainId(165),
                Property::ComTower(Faction::Player(PlayerFaction::TealGalaxy)) => TerrainId(166),
                Property::HQ(PlayerFaction::TealGalaxy) => TerrainId(167),
                Property::Lab(Faction::Player(PlayerFaction::TealGalaxy)) => TerrainId(168),
                Property::Port(Faction::Player(PlayerFaction::TealGalaxy)) => TerrainId(169),

                // Purple Lightning properties
                Property::Airport(Faction::Player(PlayerFaction::PurpleLightning)) => {
                    TerrainId(170)
                }
                Property::Base(Faction::Player(PlayerFaction::PurpleLightning)) => TerrainId(171),
                Property::City(Faction::Player(PlayerFaction::PurpleLightning)) => TerrainId(172),
                Property::ComTower(Faction::Player(PlayerFaction::PurpleLightning)) => {
                    TerrainId(173)
                }
                Property::HQ(PlayerFaction::PurpleLightning) => TerrainId(174),
                Property::Lab(Faction::Player(PlayerFaction::PurpleLightning)) => TerrainId(175),
                Property::Port(Faction::Player(PlayerFaction::PurpleLightning)) => TerrainId(176),

                // Acid Rain properties
                Property::Airport(Faction::Player(PlayerFaction::AcidRain)) => TerrainId(181),
                Property::Base(Faction::Player(PlayerFaction::AcidRain)) => TerrainId(182),
                Property::City(Faction::Player(PlayerFaction::AcidRain)) => TerrainId(183),
                Property::ComTower(Faction::Player(PlayerFaction::AcidRain)) => TerrainId(184),
                Property::HQ(PlayerFaction::AcidRain) => TerrainId(185),
                Property::Lab(Faction::Player(PlayerFaction::AcidRain)) => TerrainId(186),
                Property::Port(Faction::Player(PlayerFaction::AcidRain)) => TerrainId(187),

                // White Nova properties
                Property::Airport(Faction::Player(PlayerFaction::WhiteNova)) => TerrainId(188),
                Property::Base(Faction::Player(PlayerFaction::WhiteNova)) => TerrainId(189),
                Property::City(Faction::Player(PlayerFaction::WhiteNova)) => TerrainId(190),
                Property::ComTower(Faction::Player(PlayerFaction::WhiteNova)) => TerrainId(191),
                Property::HQ(PlayerFaction::WhiteNova) => TerrainId(192),
                Property::Lab(Faction::Player(PlayerFaction::WhiteNova)) => TerrainId(193),
                Property::Port(Faction::Player(PlayerFaction::WhiteNova)) => TerrainId(194),

                // Azure Asteroid properties
                Property::Airport(Faction::Player(PlayerFaction::AzureAsteroid)) => TerrainId(196),
                Property::Base(Faction::Player(PlayerFaction::AzureAsteroid)) => TerrainId(197),
                Property::City(Faction::Player(PlayerFaction::AzureAsteroid)) => TerrainId(198),
                Property::ComTower(Faction::Player(PlayerFaction::AzureAsteroid)) => TerrainId(199),
                Property::HQ(PlayerFaction::AzureAsteroid) => TerrainId(200),
                Property::Lab(Faction::Player(PlayerFaction::AzureAsteroid)) => TerrainId(201),
                Property::Port(Faction::Player(PlayerFaction::AzureAsteroid)) => TerrainId(202),

                // Noir Eclipse properties
                Property::Airport(Faction::Player(PlayerFaction::NoirEclipse)) => TerrainId(203),
                Property::Base(Faction::Player(PlayerFaction::NoirEclipse)) => TerrainId(204),
                Property::City(Faction::Player(PlayerFaction::NoirEclipse)) => TerrainId(205),
                Property::ComTower(Faction::Player(PlayerFaction::NoirEclipse)) => TerrainId(206),
                Property::HQ(PlayerFaction::NoirEclipse) => TerrainId(207),
                Property::Lab(Faction::Player(PlayerFaction::NoirEclipse)) => TerrainId(208),
                Property::Port(Faction::Player(PlayerFaction::NoirEclipse)) => TerrainId(209),

                // Silver Claw properties
                Property::Airport(Faction::Player(PlayerFaction::SilverClaw)) => TerrainId(210),
                Property::Base(Faction::Player(PlayerFaction::SilverClaw)) => TerrainId(211),
                Property::City(Faction::Player(PlayerFaction::SilverClaw)) => TerrainId(212),
                Property::ComTower(Faction::Player(PlayerFaction::SilverClaw)) => TerrainId(213),
                Property::HQ(PlayerFaction::SilverClaw) => TerrainId(214),
                Property::Lab(Faction::Player(PlayerFaction::SilverClaw)) => TerrainId(215),
                Property::Port(Faction::Player(PlayerFaction::SilverClaw)) => TerrainId(216),
            },

            // Pipes
            Terrain::Pipe(pipe_type) => match pipe_type {
                PipeType::Vertical => TerrainId(101),
                PipeType::Horizontal => TerrainId(102),
                PipeType::NE => TerrainId(103),
                PipeType::ES => TerrainId(104),
                PipeType::SW => TerrainId(105),
                PipeType::WN => TerrainId(106),
                PipeType::NorthEnd => TerrainId(107),
                PipeType::EastEnd => TerrainId(108),
                PipeType::SouthEnd => TerrainId(109),
                PipeType::WestEnd => TerrainId(110),
            },

            // Missile Silos
            Terrain::MissileSilo(status) => match status {
                MissileSiloStatus::Loaded => TerrainId(111),
                MissileSiloStatus::Unloaded => TerrainId(112),
            },

            // Pipe Seams
            Terrain::PipeSeam(pipe_seam_type) => match pipe_seam_type {
                PipeSeamType::Horizontal => TerrainId(113),
                PipeSeamType::Vertical => TerrainId(114),
            },

            // Pipe Rubble
            Terrain::PipeRubble(pipe_rubble_type) => match pipe_rubble_type {
                PipeRubbleType::Horizontal => TerrainId(115),
                PipeRubbleType::Vertical => TerrainId(116),
            },

            // Teleporter
            Terrain::Teleporter => TerrainId(195),
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

impl TryFrom<u8> for Terrain {
    type Error = TryFromTerrainError;

    fn try_from(id: u8) -> Result<Self, Self::Error> {
        match id {
            // Basic terrains
            1 => Ok(Terrain::Plain),
            2 => Ok(Terrain::Mountain),
            3 => Ok(Terrain::Wood),

            // Rivers
            4 => Ok(Terrain::River(RiverType::Horizontal)),
            5 => Ok(Terrain::River(RiverType::Vertical)),
            6 => Ok(Terrain::River(RiverType::Cross)),
            7 => Ok(Terrain::River(RiverType::ES)),
            8 => Ok(Terrain::River(RiverType::SW)),
            9 => Ok(Terrain::River(RiverType::WN)),
            10 => Ok(Terrain::River(RiverType::NE)),
            11 => Ok(Terrain::River(RiverType::ESW)),
            12 => Ok(Terrain::River(RiverType::SWN)),
            13 => Ok(Terrain::River(RiverType::WNE)),
            14 => Ok(Terrain::River(RiverType::NES)),

            // Roads
            15 => Ok(Terrain::Road(RoadType::Horizontal)),
            16 => Ok(Terrain::Road(RoadType::Vertical)),
            17 => Ok(Terrain::Road(RoadType::Cross)),
            18 => Ok(Terrain::Road(RoadType::ES)),
            19 => Ok(Terrain::Road(RoadType::SW)),
            20 => Ok(Terrain::Road(RoadType::WN)),
            21 => Ok(Terrain::Road(RoadType::NE)),
            22 => Ok(Terrain::Road(RoadType::ESW)),
            23 => Ok(Terrain::Road(RoadType::SWN)),
            24 => Ok(Terrain::Road(RoadType::WNE)),
            25 => Ok(Terrain::Road(RoadType::NES)),

            // Bridges
            26 => Ok(Terrain::Bridge(BridgeType::Horizontal)),
            27 => Ok(Terrain::Bridge(BridgeType::Vertical)),

            // Sea
            28 => Ok(Terrain::Sea),

            // Shoals
            29 => Ok(Terrain::Shoal(ShoalType::Horizontal)),
            30 => Ok(Terrain::Shoal(ShoalType::HorizontalNorth)),
            31 => Ok(Terrain::Shoal(ShoalType::Vertical)),
            32 => Ok(Terrain::Shoal(ShoalType::VerticalEast)),
            33 => Ok(Terrain::Reef),

            // Properties
            34 => Ok(Terrain::Property(Property::City(Faction::Neutral))),
            35 => Ok(Terrain::Property(Property::Base(Faction::Neutral))),
            36 => Ok(Terrain::Property(Property::Airport(Faction::Neutral))),
            37 => Ok(Terrain::Property(Property::Port(Faction::Neutral))),

            // Orange Star properties
            38 => Ok(Terrain::Property(Property::City(Faction::Player(
                PlayerFaction::OrangeStar,
            )))),
            39 => Ok(Terrain::Property(Property::Base(Faction::Player(
                PlayerFaction::OrangeStar,
            )))),
            40 => Ok(Terrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::OrangeStar,
            )))),
            41 => Ok(Terrain::Property(Property::Port(Faction::Player(
                PlayerFaction::OrangeStar,
            )))),
            42 => Ok(Terrain::Property(Property::HQ(PlayerFaction::OrangeStar))),

            // Blue Moon properties
            43 => Ok(Terrain::Property(Property::City(Faction::Player(
                PlayerFaction::BlueMoon,
            )))),
            44 => Ok(Terrain::Property(Property::Base(Faction::Player(
                PlayerFaction::BlueMoon,
            )))),
            45 => Ok(Terrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::BlueMoon,
            )))),
            46 => Ok(Terrain::Property(Property::Port(Faction::Player(
                PlayerFaction::BlueMoon,
            )))),
            47 => Ok(Terrain::Property(Property::HQ(PlayerFaction::BlueMoon))),

            // Green Earth properties
            48 => Ok(Terrain::Property(Property::City(Faction::Player(
                PlayerFaction::GreenEarth,
            )))),
            49 => Ok(Terrain::Property(Property::Base(Faction::Player(
                PlayerFaction::GreenEarth,
            )))),
            50 => Ok(Terrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::GreenEarth,
            )))),
            51 => Ok(Terrain::Property(Property::Port(Faction::Player(
                PlayerFaction::GreenEarth,
            )))),
            52 => Ok(Terrain::Property(Property::HQ(PlayerFaction::GreenEarth))),

            // Yellow Comet properties
            53 => Ok(Terrain::Property(Property::City(Faction::Player(
                PlayerFaction::YellowComet,
            )))),
            54 => Ok(Terrain::Property(Property::Base(Faction::Player(
                PlayerFaction::YellowComet,
            )))),
            55 => Ok(Terrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::YellowComet,
            )))),
            56 => Ok(Terrain::Property(Property::Port(Faction::Player(
                PlayerFaction::YellowComet,
            )))),
            57 => Ok(Terrain::Property(Property::HQ(PlayerFaction::YellowComet))),

            // Red Fire properties
            81 => Ok(Terrain::Property(Property::City(Faction::Player(
                PlayerFaction::RedFire,
            )))),
            82 => Ok(Terrain::Property(Property::Base(Faction::Player(
                PlayerFaction::RedFire,
            )))),
            83 => Ok(Terrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::RedFire,
            )))),
            84 => Ok(Terrain::Property(Property::Port(Faction::Player(
                PlayerFaction::RedFire,
            )))),
            85 => Ok(Terrain::Property(Property::HQ(PlayerFaction::RedFire))),

            // Grey Sky properties
            86 => Ok(Terrain::Property(Property::City(Faction::Player(
                PlayerFaction::GreySky,
            )))),
            87 => Ok(Terrain::Property(Property::Base(Faction::Player(
                PlayerFaction::GreySky,
            )))),
            88 => Ok(Terrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::GreySky,
            )))),
            89 => Ok(Terrain::Property(Property::Port(Faction::Player(
                PlayerFaction::GreySky,
            )))),
            90 => Ok(Terrain::Property(Property::HQ(PlayerFaction::GreySky))),

            // Black Hole properties
            91 => Ok(Terrain::Property(Property::City(Faction::Player(
                PlayerFaction::BlackHole,
            )))),
            92 => Ok(Terrain::Property(Property::Base(Faction::Player(
                PlayerFaction::BlackHole,
            )))),
            93 => Ok(Terrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::BlackHole,
            )))),
            94 => Ok(Terrain::Property(Property::Port(Faction::Player(
                PlayerFaction::BlackHole,
            )))),
            95 => Ok(Terrain::Property(Property::HQ(PlayerFaction::BlackHole))),

            // Brown Desert properties
            96 => Ok(Terrain::Property(Property::City(Faction::Player(
                PlayerFaction::BrownDesert,
            )))),
            97 => Ok(Terrain::Property(Property::Base(Faction::Player(
                PlayerFaction::BrownDesert,
            )))),
            98 => Ok(Terrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::BrownDesert,
            )))),
            99 => Ok(Terrain::Property(Property::Port(Faction::Player(
                PlayerFaction::BrownDesert,
            )))),
            100 => Ok(Terrain::Property(Property::HQ(PlayerFaction::BrownDesert))),

            // Pipes
            101 => Ok(Terrain::Pipe(PipeType::Vertical)),
            102 => Ok(Terrain::Pipe(PipeType::Horizontal)),
            103 => Ok(Terrain::Pipe(PipeType::NE)),
            104 => Ok(Terrain::Pipe(PipeType::ES)),
            105 => Ok(Terrain::Pipe(PipeType::SW)),
            106 => Ok(Terrain::Pipe(PipeType::WN)),
            107 => Ok(Terrain::Pipe(PipeType::NorthEnd)),
            108 => Ok(Terrain::Pipe(PipeType::EastEnd)),
            109 => Ok(Terrain::Pipe(PipeType::SouthEnd)),
            110 => Ok(Terrain::Pipe(PipeType::WestEnd)),

            // Missile Silos
            111 => Ok(Terrain::MissileSilo(MissileSiloStatus::Loaded)),
            112 => Ok(Terrain::MissileSilo(MissileSiloStatus::Unloaded)),

            // Pipe Seams
            113 => Ok(Terrain::PipeSeam(PipeSeamType::Horizontal)),
            114 => Ok(Terrain::PipeSeam(PipeSeamType::Vertical)),

            // Pipe Rubble
            115 => Ok(Terrain::PipeRubble(PipeRubbleType::Horizontal)),
            116 => Ok(Terrain::PipeRubble(PipeRubbleType::Vertical)),

            // Amber Blaze properties
            117 => Ok(Terrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::AmberBlaze,
            )))),
            118 => Ok(Terrain::Property(Property::Base(Faction::Player(
                PlayerFaction::AmberBlaze,
            )))),
            119 => Ok(Terrain::Property(Property::City(Faction::Player(
                PlayerFaction::AmberBlaze,
            )))),
            120 => Ok(Terrain::Property(Property::HQ(PlayerFaction::AmberBlaze))),
            121 => Ok(Terrain::Property(Property::Port(Faction::Player(
                PlayerFaction::AmberBlaze,
            )))),

            // Jade Sun properties
            122 => Ok(Terrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::JadeSun,
            )))),
            123 => Ok(Terrain::Property(Property::Base(Faction::Player(
                PlayerFaction::JadeSun,
            )))),
            124 => Ok(Terrain::Property(Property::City(Faction::Player(
                PlayerFaction::JadeSun,
            )))),
            125 => Ok(Terrain::Property(Property::HQ(PlayerFaction::JadeSun))),
            126 => Ok(Terrain::Property(Property::Port(Faction::Player(
                PlayerFaction::JadeSun,
            )))),

            // Com Towers
            127 => Ok(Terrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::AmberBlaze,
            )))),
            128 => Ok(Terrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::BlackHole,
            )))),
            129 => Ok(Terrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::BlueMoon,
            )))),
            130 => Ok(Terrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::BrownDesert,
            )))),
            131 => Ok(Terrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::GreenEarth,
            )))),
            132 => Ok(Terrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::JadeSun,
            )))),
            133 => Ok(Terrain::Property(Property::ComTower(Faction::Neutral))),
            134 => Ok(Terrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::OrangeStar,
            )))),
            135 => Ok(Terrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::RedFire,
            )))),
            136 => Ok(Terrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::YellowComet,
            )))),
            137 => Ok(Terrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::GreySky,
            )))),

            // Labs
            138 => Ok(Terrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::AmberBlaze,
            )))),
            139 => Ok(Terrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::BlackHole,
            )))),
            140 => Ok(Terrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::BlueMoon,
            )))),
            141 => Ok(Terrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::BrownDesert,
            )))),
            142 => Ok(Terrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::GreenEarth,
            )))),
            143 => Ok(Terrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::GreySky,
            )))),
            144 => Ok(Terrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::JadeSun,
            )))),
            145 => Ok(Terrain::Property(Property::Lab(Faction::Neutral))),
            146 => Ok(Terrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::OrangeStar,
            )))),
            147 => Ok(Terrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::RedFire,
            )))),
            148 => Ok(Terrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::YellowComet,
            )))),

            // Cobalt Ice properties
            149 => Ok(Terrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::CobaltIce,
            )))),
            150 => Ok(Terrain::Property(Property::Base(Faction::Player(
                PlayerFaction::CobaltIce,
            )))),
            151 => Ok(Terrain::Property(Property::City(Faction::Player(
                PlayerFaction::CobaltIce,
            )))),
            152 => Ok(Terrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::CobaltIce,
            )))),
            153 => Ok(Terrain::Property(Property::HQ(PlayerFaction::CobaltIce))),
            154 => Ok(Terrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::CobaltIce,
            )))),
            155 => Ok(Terrain::Property(Property::Port(Faction::Player(
                PlayerFaction::CobaltIce,
            )))),

            // Pink Cosmos properties
            156 => Ok(Terrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::PinkCosmos,
            )))),
            157 => Ok(Terrain::Property(Property::Base(Faction::Player(
                PlayerFaction::PinkCosmos,
            )))),
            158 => Ok(Terrain::Property(Property::City(Faction::Player(
                PlayerFaction::PinkCosmos,
            )))),
            159 => Ok(Terrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::PinkCosmos,
            )))),
            160 => Ok(Terrain::Property(Property::HQ(PlayerFaction::PinkCosmos))),
            161 => Ok(Terrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::PinkCosmos,
            )))),
            162 => Ok(Terrain::Property(Property::Port(Faction::Player(
                PlayerFaction::PinkCosmos,
            )))),

            // Teal Galaxy properties
            163 => Ok(Terrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::TealGalaxy,
            )))),
            164 => Ok(Terrain::Property(Property::Base(Faction::Player(
                PlayerFaction::TealGalaxy,
            )))),
            165 => Ok(Terrain::Property(Property::City(Faction::Player(
                PlayerFaction::TealGalaxy,
            )))),
            166 => Ok(Terrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::TealGalaxy,
            )))),
            167 => Ok(Terrain::Property(Property::HQ(PlayerFaction::TealGalaxy))),
            168 => Ok(Terrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::TealGalaxy,
            )))),
            169 => Ok(Terrain::Property(Property::Port(Faction::Player(
                PlayerFaction::TealGalaxy,
            )))),

            // Purple Lightning properties
            170 => Ok(Terrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::PurpleLightning,
            )))),
            171 => Ok(Terrain::Property(Property::Base(Faction::Player(
                PlayerFaction::PurpleLightning,
            )))),
            172 => Ok(Terrain::Property(Property::City(Faction::Player(
                PlayerFaction::PurpleLightning,
            )))),
            173 => Ok(Terrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::PurpleLightning,
            )))),
            174 => Ok(Terrain::Property(Property::HQ(
                PlayerFaction::PurpleLightning,
            ))),
            175 => Ok(Terrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::PurpleLightning,
            )))),
            176 => Ok(Terrain::Property(Property::Port(Faction::Player(
                PlayerFaction::PurpleLightning,
            )))),

            // Teleporter
            195 => Ok(Terrain::Teleporter),

            // Acid Rain properties
            181 => Ok(Terrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::AcidRain,
            )))),
            182 => Ok(Terrain::Property(Property::Base(Faction::Player(
                PlayerFaction::AcidRain,
            )))),
            183 => Ok(Terrain::Property(Property::City(Faction::Player(
                PlayerFaction::AcidRain,
            )))),
            184 => Ok(Terrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::AcidRain,
            )))),
            185 => Ok(Terrain::Property(Property::HQ(PlayerFaction::AcidRain))),
            186 => Ok(Terrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::AcidRain,
            )))),
            187 => Ok(Terrain::Property(Property::Port(Faction::Player(
                PlayerFaction::AcidRain,
            )))),

            // White Nova properties
            188 => Ok(Terrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::WhiteNova,
            )))),
            189 => Ok(Terrain::Property(Property::Base(Faction::Player(
                PlayerFaction::WhiteNova,
            )))),
            190 => Ok(Terrain::Property(Property::City(Faction::Player(
                PlayerFaction::WhiteNova,
            )))),
            191 => Ok(Terrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::WhiteNova,
            )))),
            192 => Ok(Terrain::Property(Property::HQ(PlayerFaction::WhiteNova))),
            193 => Ok(Terrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::WhiteNova,
            )))),
            194 => Ok(Terrain::Property(Property::Port(Faction::Player(
                PlayerFaction::WhiteNova,
            )))),

            // Azure Asteroid properties
            196 => Ok(Terrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::AzureAsteroid,
            )))),
            197 => Ok(Terrain::Property(Property::Base(Faction::Player(
                PlayerFaction::AzureAsteroid,
            )))),
            198 => Ok(Terrain::Property(Property::City(Faction::Player(
                PlayerFaction::AzureAsteroid,
            )))),
            199 => Ok(Terrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::AzureAsteroid,
            )))),
            200 => Ok(Terrain::Property(Property::HQ(
                PlayerFaction::AzureAsteroid,
            ))),
            201 => Ok(Terrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::AzureAsteroid,
            )))),
            202 => Ok(Terrain::Property(Property::Port(Faction::Player(
                PlayerFaction::AzureAsteroid,
            )))),

            // Noir Eclipse properties
            203 => Ok(Terrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::NoirEclipse,
            )))),
            204 => Ok(Terrain::Property(Property::Base(Faction::Player(
                PlayerFaction::NoirEclipse,
            )))),
            205 => Ok(Terrain::Property(Property::City(Faction::Player(
                PlayerFaction::NoirEclipse,
            )))),
            206 => Ok(Terrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::NoirEclipse,
            )))),
            207 => Ok(Terrain::Property(Property::HQ(PlayerFaction::NoirEclipse))),
            208 => Ok(Terrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::NoirEclipse,
            )))),
            209 => Ok(Terrain::Property(Property::Port(Faction::Player(
                PlayerFaction::NoirEclipse,
            )))),

            // Silver Claw properties
            210 => Ok(Terrain::Property(Property::Airport(Faction::Player(
                PlayerFaction::SilverClaw,
            )))),
            211 => Ok(Terrain::Property(Property::Base(Faction::Player(
                PlayerFaction::SilverClaw,
            )))),
            212 => Ok(Terrain::Property(Property::City(Faction::Player(
                PlayerFaction::SilverClaw,
            )))),
            213 => Ok(Terrain::Property(Property::ComTower(Faction::Player(
                PlayerFaction::SilverClaw,
            )))),
            214 => Ok(Terrain::Property(Property::HQ(PlayerFaction::SilverClaw))),
            215 => Ok(Terrain::Property(Property::Lab(Faction::Player(
                PlayerFaction::SilverClaw,
            )))),
            216 => Ok(Terrain::Property(Property::Port(Faction::Player(
                PlayerFaction::SilverClaw,
            )))),
            _ => Err(TryFromTerrainError::InvalidId(id)),
        }
    }
}

/// Terrain that represents the graphical representation. One can have tall
/// mountains and stubby mountains, but functionally they act the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GraphicalTerrain {
    StubbyMoutain,
    Terrain(Terrain),
}

impl GraphicalTerrain {
    pub fn as_terrain(self) -> Terrain {
        match self {
            GraphicalTerrain::StubbyMoutain => Terrain::Mountain,
            GraphicalTerrain::Terrain(terrain) => terrain,
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
            TerrainId::from(Terrain::MissileSilo(MissileSiloStatus::Loaded)).0,
            111
        );
        assert_eq!(
            TerrainId::from(Terrain::MissileSilo(MissileSiloStatus::Unloaded)).0,
            112
        );

        // Test names
        assert_eq!(
            Terrain::MissileSilo(MissileSiloStatus::Loaded).name(),
            "Missile Silo"
        );
        assert_eq!(
            Terrain::MissileSilo(MissileSiloStatus::Unloaded).name(),
            "Missile Silo Empty"
        );
    }

    #[test]
    fn test_terrain_type_name() {
        // Test basic terrain names
        assert_eq!(Terrain::Plain.name(), "Plain");
        assert_eq!(Terrain::Mountain.name(), "Mountain");
        assert_eq!(Terrain::Wood.name(), "Wood");
        assert_eq!(Terrain::Sea.name(), "Sea");
        assert_eq!(Terrain::Reef.name(), "Reef");

        // Test river names
        assert_eq!(Terrain::River(RiverType::Horizontal).name(), "HRiver");
        assert_eq!(Terrain::River(RiverType::Vertical).name(), "VRiver");

        // Test road names
        assert_eq!(Terrain::Road(RoadType::Horizontal).name(), "HRoad");
        assert_eq!(Terrain::Road(RoadType::Vertical).name(), "VRoad");

        // Test bridge names
        assert_eq!(Terrain::Bridge(BridgeType::Horizontal).name(), "HBridge");
        assert_eq!(Terrain::Bridge(BridgeType::Vertical).name(), "VBridge");

        // Test pipe names
        assert_eq!(Terrain::Pipe(PipeType::Horizontal).name(), "HPipe");
        assert_eq!(Terrain::Pipe(PipeType::Vertical).name(), "VPipe");
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
            Terrain::Property(Property::City(Faction::Player(PlayerFaction::OrangeStar))).name(),
            "Orange Star City"
        );
        assert_eq!(
            Terrain::Property(Property::HQ(PlayerFaction::BlueMoon)).name(),
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
        assert_eq!(Terrain::Plain.owner(), None);
        assert_eq!(Terrain::Mountain.owner(), None);
        assert_eq!(Terrain::Sea.owner(), None);

        // Properties should have the correct owner
        assert_eq!(
            Terrain::Property(Property::City(Faction::Neutral)).owner(),
            Some(Faction::Neutral)
        );

        assert_eq!(
            Terrain::Property(Property::HQ(PlayerFaction::OrangeStar)).owner(),
            Some(Faction::Player(PlayerFaction::OrangeStar))
        );

        assert_eq!(
            Terrain::Property(Property::Airport(Faction::Player(PlayerFaction::BlueMoon))).owner(),
            Some(Faction::Player(PlayerFaction::BlueMoon))
        );
    }

    #[test]
    fn test_terrain_is_hq() {
        // Only HQ properties should return true
        assert!(Terrain::Property(Property::HQ(PlayerFaction::OrangeStar)).is_hq());

        // Other properties should return false
        assert!(!Terrain::Property(Property::City(Faction::Neutral)).is_hq());
        assert!(
            !Terrain::Property(Property::Base(Faction::Player(PlayerFaction::BlueMoon))).is_hq()
        );

        // Non-property terrains should return false
        assert!(!Terrain::Plain.is_hq());
        assert!(!Terrain::Mountain.is_hq());
        assert!(!Terrain::Sea.is_hq());
    }

    #[test]
    fn test_terrain_defense_stars() {
        // Test defense stars values
        assert_eq!(Terrain::Plain.defense_stars(), 1);
        assert_eq!(Terrain::Mountain.defense_stars(), 4);
        assert_eq!(Terrain::Wood.defense_stars(), 2);
        assert_eq!(Terrain::River(RiverType::Horizontal).defense_stars(), 0);
        assert_eq!(Terrain::Sea.defense_stars(), 0);

        // Properties should have correct defense values
        assert_eq!(
            Terrain::Property(Property::City(Faction::Neutral)).defense_stars(),
            3
        );
        assert_eq!(
            Terrain::Property(Property::HQ(PlayerFaction::OrangeStar)).defense_stars(),
            4
        );
    }

    #[test]
    fn test_terrain_is_land() {
        // Land terrains
        assert!(Terrain::Plain.is_land());
        assert!(Terrain::Mountain.is_land());
        assert!(Terrain::Wood.is_land());
        assert!(Terrain::Property(Property::City(Faction::Neutral)).is_land());

        // Non-land terrains
        assert!(!Terrain::River(RiverType::Horizontal).is_land());
        assert!(!Terrain::Sea.is_land());
    }

    #[test]
    fn test_terrain_is_sea() {
        // Sea terrains
        assert!(Terrain::Sea.is_sea());
        assert!(Terrain::Property(Property::Port(Faction::Neutral)).is_sea());

        // Non-sea terrains
        assert!(!Terrain::Plain.is_sea());
        assert!(!Terrain::Mountain.is_sea());
        assert!(!Terrain::River(RiverType::Horizontal).is_sea());
        assert!(!Terrain::Property(Property::City(Faction::Neutral)).is_sea());
    }

    #[test]
    fn test_terrain_is_capturable() {
        // Capturable terrains (properties)
        assert!(Terrain::Property(Property::City(Faction::Neutral)).is_capturable());
        assert!(
            Terrain::Property(Property::Base(Faction::Player(PlayerFaction::OrangeStar)))
                .is_capturable()
        );
        assert!(Terrain::Property(Property::HQ(PlayerFaction::BlueMoon)).is_capturable());

        // Non-capturable terrains
        assert!(!Terrain::Plain.is_capturable());
        assert!(!Terrain::Mountain.is_capturable());
        assert!(!Terrain::Sea.is_capturable());
    }

    #[test]
    fn test_terrain_symbol() {
        // Test symbols for various terrain types
        assert_eq!(Terrain::Plain.symbol(), Some('.'));
        assert_eq!(Terrain::Mountain.symbol(), Some('^'));
        assert_eq!(Terrain::Wood.symbol(), Some('@'));
        assert_eq!(Terrain::Sea.symbol(), Some(','));

        // Test symbols for properties
        assert_eq!(
            Terrain::Property(Property::City(Faction::Neutral)).symbol(),
            Some('a')
        );
        assert_eq!(
            Terrain::Property(Property::HQ(PlayerFaction::OrangeStar)).symbol(),
            Some('i')
        );
    }

    #[test]
    fn test_gameplay_terrain_type() {
        // Test that gameplay type correctly abstracts visual differences
        assert_eq!(
            Terrain::River(RiverType::Horizontal).gameplay_type(),
            Terrain::River(RiverType::Vertical).gameplay_type()
        );

        assert_eq!(
            Terrain::Road(RoadType::Horizontal).gameplay_type(),
            Terrain::Road(RoadType::Cross).gameplay_type()
        );

        // Test that gameplay type preserves property categories
        assert_eq!(
            Terrain::Property(Property::City(Faction::Neutral)).gameplay_type(),
            GameplayTerrain::Property(PropertyCategory::City(Faction::Neutral))
        );

        assert_eq!(
            Terrain::Property(Property::HQ(PlayerFaction::OrangeStar)).gameplay_type(),
            GameplayTerrain::Property(PropertyCategory::HQ(PlayerFaction::OrangeStar))
        );

        // Test that MissileSiloStatus is preserved
        assert_eq!(
            Terrain::MissileSilo(MissileSiloStatus::Loaded).gameplay_type(),
            GameplayTerrain::MissileSilo(MissileSiloStatus::Loaded)
        );

        assert_eq!(
            Terrain::MissileSilo(MissileSiloStatus::Unloaded).gameplay_type(),
            GameplayTerrain::MissileSilo(MissileSiloStatus::Unloaded)
        );
    }

    #[test]
    fn test_u8_to_terrain_conversion() {
        // Test conversion from u8 to Terrain directly
        assert_eq!(Terrain::try_from(1).unwrap(), Terrain::Plain);
        assert_eq!(Terrain::try_from(2).unwrap(), Terrain::Mountain);
        assert_eq!(Terrain::try_from(3).unwrap(), Terrain::Wood);

        // Test rivers
        assert_eq!(
            Terrain::try_from(4).unwrap(),
            Terrain::River(RiverType::Horizontal)
        );
        assert_eq!(
            Terrain::try_from(14).unwrap(),
            Terrain::River(RiverType::NES)
        );

        // Test properties
        assert_eq!(
            Terrain::try_from(34).unwrap(),
            Terrain::Property(Property::City(Faction::Neutral))
        );
        assert_eq!(
            Terrain::try_from(42).unwrap(),
            Terrain::Property(Property::HQ(PlayerFaction::OrangeStar))
        );

        // Test other terrain types
        assert_eq!(Terrain::try_from(28).unwrap(), Terrain::Sea);
        assert_eq!(
            Terrain::try_from(111).unwrap(),
            Terrain::MissileSilo(MissileSiloStatus::Loaded)
        );

        // Test invalid IDs - specific error types
        assert_eq!(
            Terrain::try_from(58).unwrap_err(),
            TryFromTerrainError::InvalidId(58)
        );

        // Test round trip conversion (u8 -> Terrain -> TerrainId -> u8)
        for id in [1, 2, 3, 28, 34, 42, 111, 195] {
            let terrain = Terrain::try_from(id).unwrap();
            let terrain_id = TerrainId::from(terrain);
            assert_eq!(terrain_id.0, id);
        }
    }
}
