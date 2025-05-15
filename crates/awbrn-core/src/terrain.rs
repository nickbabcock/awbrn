use crate::{AwbwTerrain, Faction, PlayerFaction};

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

/// Sea configurations based on the variants file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[expect(non_camel_case_types)]
pub enum SeaDirection {
    E,
    E_NW,
    E_NW_SW,
    E_S,
    E_S_NW,
    E_S_W,
    E_SW,
    E_W,
    N,
    N_E,
    N_E_S,
    N_E_S_W,
    N_E_SW,
    N_E_W,
    N_S,
    N_S_W,
    N_SE,
    N_SE_SW,
    N_SW,
    N_W,
    N_W_SE,
    NE,
    NE_SE,
    NE_SE_SW,
    NE_SW,
    NW,
    NW_NE,
    NW_NE_SE,
    NW_NE_SE_SW,
    NW_NE_SW,
    NW_SE,
    NW_SE_SW,
    NW_SW,
    S,
    S_E,
    S_NE,
    S_NW,
    S_NW_NE,
    S_W,
    S_W_NE,
    SE,
    SE_SW,
    SW,
    Sea,
    W,
    W_E,
    W_NE,
    W_NE_SE,
    W_SE,
}

/// Shoal configurations based on the variants file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShoalDirection {
    AE,
    AEAS,
    AEASAW,
    AEASW,
    AEAW,
    AES,
    AESAW,
    AESW,
    AEW,
    AN,
    ANAE,
    ANAEAS,
    ANAEASAW,
    ANAEASW,
    ANAEAW,
    ANAES,
    ANAESAW,
    ANAESW,
    ANAEW,
    ANAS,
    ANASAW,
    ANASW,
    ANAW,
    ANE,
    ANEAS,
    ANEASAW,
    ANEASW,
    ANEAW,
    ANES,
    ANESAW,
    ANESW,
    ANEW,
    ANS,
    ANSAW,
    ANSW,
    ANW,
    AS,
    ASAW,
    ASW,
    AW,
    C,
    E,
    EAS,
    EASAW,
    EASW,
    EAW,
    ES,
    ESAW,
    ESW,
    EW,
    N,
    NAE,
    NAEAS,
    NAEASAW,
    NAEASW,
    NAEAW,
    NAES,
    NAESAW,
    NAESW,
    NAEW,
    NAS,
    NASAW,
    NASW,
    NAW,
    NE,
    NEAS,
    NEASAW,
    NEASW,
    NEAW,
    NES,
    NESAW,
    NESW,
    NEW,
    NS,
    NSAW,
    NSW,
    NW,
    S,
    SAW,
    SW,
    W,
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
    pub const fn faction(&self) -> Faction {
        match self {
            Property::City(faction) => *faction,
            Property::Base(faction) => *faction,
            Property::Airport(faction) => *faction,
            Property::Port(faction) => *faction,
            Property::ComTower(faction) => *faction,
            Property::Lab(faction) => *faction,
            Property::HQ(faction) => Faction::Player(*faction),
        }
    }

    pub const fn kind(&self) -> PropertyKind {
        match self {
            Property::Airport(_) => PropertyKind::Airport,
            Property::Base(_) => PropertyKind::Base,
            Property::City(_) => PropertyKind::City,
            Property::ComTower(_) => PropertyKind::ComTower,
            Property::HQ(_) => PropertyKind::HQ,
            Property::Lab(_) => PropertyKind::Lab,
            Property::Port(_) => PropertyKind::Port,
        }
    }

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

/// Property types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PropertyKind {
    Airport,
    Base,
    City,
    ComTower,
    HQ,
    Lab,
    Port,
}

impl PropertyKind {
    /// Get the previous property kind in the order of the property types
    pub const fn prev(&self) -> PropertyKind {
        match self {
            PropertyKind::Airport => PropertyKind::Port,
            PropertyKind::Base => PropertyKind::Airport,
            PropertyKind::City => PropertyKind::Base,
            PropertyKind::ComTower => PropertyKind::City,
            PropertyKind::HQ => PropertyKind::ComTower,
            PropertyKind::Lab => PropertyKind::HQ,
            PropertyKind::Port => PropertyKind::Lab,
        }
    }
}

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
    Property(Property),
    Pipe,
    PipeSeam,
    PipeRubble,
    MissileSilo(MissileSiloStatus),
    Teleporter,
}

/// Terrain that represents the graphical representation. One can have tall
/// mountains and stubby mountains, but functionally they act the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GraphicalTerrain {
    // Basic terrains
    StubbyMoutain,
    Plain,
    Mountain,
    Wood,
    Reef,

    // Rivers with different configurations
    River(RiverType),

    // Roads with different configurations
    Road(RoadType),

    // Bridges
    Bridge(BridgeType),

    // Properties
    Property(Property),

    // Pipes and related structures
    Pipe(PipeType),
    PipeSeam(PipeSeamType),
    PipeRubble(PipeRubbleType),

    // Special terrains
    MissileSilo(MissileSiloStatus),
    Teleporter,

    // Sea and Shoal variants from the variants file
    Sea(SeaDirection),
    Shoal(ShoalDirection),
}

impl GraphicalTerrain {
    pub const fn as_terrain(self) -> AwbwTerrain {
        match self {
            // Basic terrains
            GraphicalTerrain::StubbyMoutain => AwbwTerrain::Mountain,
            GraphicalTerrain::Plain => AwbwTerrain::Plain,
            GraphicalTerrain::Mountain => AwbwTerrain::Mountain,
            GraphicalTerrain::Wood => AwbwTerrain::Wood,
            GraphicalTerrain::Reef => AwbwTerrain::Reef,

            // Rivers
            GraphicalTerrain::River(river_type) => AwbwTerrain::River(river_type),

            // Roads
            GraphicalTerrain::Road(road_type) => AwbwTerrain::Road(road_type),

            // Bridges
            GraphicalTerrain::Bridge(bridge_type) => AwbwTerrain::Bridge(bridge_type),

            // Properties
            GraphicalTerrain::Property(property) => AwbwTerrain::Property(property),

            // Pipes and related
            GraphicalTerrain::Pipe(pipe_type) => AwbwTerrain::Pipe(pipe_type),
            GraphicalTerrain::PipeSeam(pipe_seam_type) => AwbwTerrain::PipeSeam(pipe_seam_type),
            GraphicalTerrain::PipeRubble(pipe_rubble_type) => {
                AwbwTerrain::PipeRubble(pipe_rubble_type)
            }

            // Special terrains
            GraphicalTerrain::MissileSilo(status) => AwbwTerrain::MissileSilo(status),
            GraphicalTerrain::Teleporter => AwbwTerrain::Teleporter,

            // Sea variants
            GraphicalTerrain::Sea(_) => AwbwTerrain::Sea,
            // Shoal variants - for simplicity, mapping all to Horizontal for now
            // This would need refinement based on actual requirements
            GraphicalTerrain::Shoal(_) => AwbwTerrain::Shoal(ShoalType::Horizontal),
        }
    }
}

/// Movement terrain represents terrain types from a movement perspective,
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum MovementTerrain {
    Plains,         // Basic open terrain
    Mountains,      // High elevation terrain
    Woods,          // Forested terrain
    Rivers,         // Water passages
    Infrastructure, // Roads, bridges, properties
    Sea,            // Ocean tiles
    Shoals,         // Beach/coast tiles
    Reefs,          // Shallow water with coral
    Pipes,          // Pipe terrain
    Teleport,       // Teleporter tiles
}

impl From<AwbwTerrain> for MovementTerrain {
    /// Convert from detailed Terrain type to simplified MovementTerrain
    fn from(terrain: AwbwTerrain) -> Self {
        match terrain {
            AwbwTerrain::Plain | AwbwTerrain::PipeRubble(_) => MovementTerrain::Plains,
            AwbwTerrain::Mountain => MovementTerrain::Mountains,
            AwbwTerrain::Wood => MovementTerrain::Woods,
            AwbwTerrain::River(_) => MovementTerrain::Rivers,
            AwbwTerrain::Sea => MovementTerrain::Sea,
            AwbwTerrain::Shoal(_) => MovementTerrain::Shoals,
            AwbwTerrain::Reef => MovementTerrain::Reefs,
            AwbwTerrain::Road(_)
            | AwbwTerrain::Bridge(_)
            | AwbwTerrain::Property(_)
            | AwbwTerrain::MissileSilo(_) => MovementTerrain::Infrastructure,
            AwbwTerrain::Pipe(_) | AwbwTerrain::PipeSeam(_) => MovementTerrain::Pipes,
            AwbwTerrain::Teleporter => MovementTerrain::Teleport,
        }
    }
}

impl From<GraphicalTerrain> for MovementTerrain {
    /// Convert from GraphicalTerrain to simplified MovementTerrain
    fn from(terrain: GraphicalTerrain) -> Self {
        match terrain {
            // Basic terrains
            GraphicalTerrain::Plain => MovementTerrain::Plains,
            GraphicalTerrain::StubbyMoutain | GraphicalTerrain::Mountain => {
                MovementTerrain::Mountains
            }
            GraphicalTerrain::Wood => MovementTerrain::Woods,
            GraphicalTerrain::Reef => MovementTerrain::Reefs,

            // Rivers
            GraphicalTerrain::River(_) => MovementTerrain::Rivers,

            // Roads, Bridges, Properties, MissileSilos
            GraphicalTerrain::Road(_)
            | GraphicalTerrain::Bridge(_)
            | GraphicalTerrain::Property(_)
            | GraphicalTerrain::MissileSilo(_) => MovementTerrain::Infrastructure,

            // Pipes
            GraphicalTerrain::Pipe(_) | GraphicalTerrain::PipeSeam(_) => MovementTerrain::Pipes,
            GraphicalTerrain::PipeRubble(_) => MovementTerrain::Plains,

            // Special terrains
            GraphicalTerrain::Teleporter => MovementTerrain::Teleport,

            // Sea variants
            GraphicalTerrain::Sea(_) => MovementTerrain::Sea,

            // Shoal variants
            GraphicalTerrain::Shoal(_) => MovementTerrain::Shoals,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_movement_terrain() {
        assert_eq!(
            MovementTerrain::from(AwbwTerrain::Plain),
            MovementTerrain::Plains
        );
        assert_eq!(
            MovementTerrain::from(AwbwTerrain::Mountain),
            MovementTerrain::Mountains
        );
        assert_eq!(
            MovementTerrain::from(AwbwTerrain::Wood),
            MovementTerrain::Woods
        );
        assert_eq!(
            MovementTerrain::from(AwbwTerrain::River(RiverType::Horizontal)),
            MovementTerrain::Rivers
        );
        assert_eq!(
            MovementTerrain::from(AwbwTerrain::Sea),
            MovementTerrain::Sea
        );
        assert_eq!(
            MovementTerrain::from(AwbwTerrain::Shoal(ShoalType::Horizontal)),
            MovementTerrain::Shoals
        );
        assert_eq!(
            MovementTerrain::from(AwbwTerrain::Reef),
            MovementTerrain::Reefs
        );
        assert_eq!(
            MovementTerrain::from(AwbwTerrain::Road(RoadType::Horizontal)),
            MovementTerrain::Infrastructure
        );
        assert_eq!(
            MovementTerrain::from(AwbwTerrain::Bridge(BridgeType::Vertical)),
            MovementTerrain::Infrastructure
        );
        assert_eq!(
            MovementTerrain::from(AwbwTerrain::MissileSilo(MissileSiloStatus::Loaded)),
            MovementTerrain::Infrastructure
        );
        assert_eq!(
            MovementTerrain::from(AwbwTerrain::MissileSilo(MissileSiloStatus::Unloaded)),
            MovementTerrain::Infrastructure
        );
        assert_eq!(
            MovementTerrain::from(AwbwTerrain::Pipe(PipeType::Vertical)),
            MovementTerrain::Pipes
        );
        assert_eq!(
            MovementTerrain::from(AwbwTerrain::PipeSeam(PipeSeamType::Horizontal)),
            MovementTerrain::Pipes
        );
        assert_eq!(
            MovementTerrain::from(AwbwTerrain::PipeRubble(PipeRubbleType::Horizontal)),
            MovementTerrain::Plains
        );
        assert_eq!(
            MovementTerrain::from(AwbwTerrain::Teleporter),
            MovementTerrain::Teleport
        );
    }
}
