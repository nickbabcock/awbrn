use crate::{AwbwTerrain, Faction, PlayerFaction};

/// Status of the missile silo
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[cfg_attr(feature = "bevy", derive(bevy::reflect::Reflect))]
pub enum MissileSiloStatus {
    Loaded,
    Unloaded,
}

/// River configurations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[cfg_attr(feature = "bevy", derive(bevy::reflect::Reflect))]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[cfg_attr(feature = "bevy", derive(bevy::reflect::Reflect))]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[cfg_attr(feature = "bevy", derive(bevy::reflect::Reflect))]
pub enum BridgeType {
    Horizontal,
    Vertical,
}

/// Shoal types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[cfg_attr(feature = "bevy", derive(bevy::reflect::Reflect))]
pub enum ShoalType {
    Horizontal,
    HorizontalNorth,
    Vertical,
    VerticalEast,
}

impl ShoalType {
    /// The shoal that lies against the land on these sides.
    ///
    /// The client draws a shoal from the tiles around it, so this variant is
    /// what the map document records and not what the player sees. It names
    /// the same side the picture does, which keeps a mirrored board equal to
    /// its original tile for tile.
    ///
    /// AWBW has one variant for each of the four sides and no more, so a shoal
    /// with land on more than one side is recorded against one of them: north
    /// before south, and east before west.
    pub const fn from_land(north: bool, east: bool, south: bool, west: bool) -> ShoalType {
        match (north, east, south, west) {
            (true, false, false, false) => ShoalType::HorizontalNorth,
            (false, true, false, false) => ShoalType::VerticalEast,
            (false, false, false, true) => ShoalType::Vertical,
            (false, false, true, false) => ShoalType::Horizontal,
            (true, _, false, _) => ShoalType::HorizontalNorth,
            (false, _, true, _) => ShoalType::Horizontal,
            (_, true, _, false) => ShoalType::VerticalEast,
            _ => ShoalType::Vertical,
        }
    }

    /// The variant that records the shoal the client draws.
    ///
    /// The picture and the document then name the same land, because they are
    /// the same answer read twice.
    pub const fn from_direction(direction: ShoalDirection) -> ShoalType {
        let sides = direction.land_sides();
        ShoalType::from_land(sides[0], sides[1], sides[2], sides[3])
    }
}

/// Sea configurations based on the variants file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[cfg_attr(feature = "bevy", derive(bevy::reflect::Reflect))]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[cfg_attr(feature = "bevy", derive(bevy::reflect::Reflect))]
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

impl ShoalDirection {
    /// The sides of the tile that land lies against, as north, east, south and
    /// west.
    ///
    /// The name of a variant lists the sides in that order: a bare side letter
    /// is land, a side after an `A` is open water, and a side that is absent
    /// holds more shoal.
    pub const fn land_sides(self) -> [bool; 4] {
        match self {
            ShoalDirection::AE => [false, false, false, false],
            ShoalDirection::AEAS => [false, false, false, false],
            ShoalDirection::AEASAW => [false, false, false, false],
            ShoalDirection::AEASW => [false, false, false, true],
            ShoalDirection::AEAW => [false, false, false, false],
            ShoalDirection::AES => [false, false, true, false],
            ShoalDirection::AESAW => [false, false, true, false],
            ShoalDirection::AESW => [false, false, true, true],
            ShoalDirection::AEW => [false, false, false, true],
            ShoalDirection::AN => [false, false, false, false],
            ShoalDirection::ANAE => [false, false, false, false],
            ShoalDirection::ANAEAS => [false, false, false, false],
            ShoalDirection::ANAEASAW => [false, false, false, false],
            ShoalDirection::ANAEASW => [false, false, false, true],
            ShoalDirection::ANAEAW => [false, false, false, false],
            ShoalDirection::ANAES => [false, false, true, false],
            ShoalDirection::ANAESAW => [false, false, true, false],
            ShoalDirection::ANAESW => [false, false, true, true],
            ShoalDirection::ANAEW => [false, false, false, true],
            ShoalDirection::ANAS => [false, false, false, false],
            ShoalDirection::ANASAW => [false, false, false, false],
            ShoalDirection::ANASW => [false, false, false, true],
            ShoalDirection::ANAW => [false, false, false, false],
            ShoalDirection::ANE => [false, true, false, false],
            ShoalDirection::ANEAS => [false, true, false, false],
            ShoalDirection::ANEASAW => [false, true, false, false],
            ShoalDirection::ANEASW => [false, true, false, true],
            ShoalDirection::ANEAW => [false, true, false, false],
            ShoalDirection::ANES => [false, true, true, false],
            ShoalDirection::ANESAW => [false, true, true, false],
            ShoalDirection::ANESW => [false, true, true, true],
            ShoalDirection::ANEW => [false, true, false, true],
            ShoalDirection::ANS => [false, false, true, false],
            ShoalDirection::ANSAW => [false, false, true, false],
            ShoalDirection::ANSW => [false, false, true, true],
            ShoalDirection::ANW => [false, false, false, true],
            ShoalDirection::AS => [false, false, false, false],
            ShoalDirection::ASAW => [false, false, false, false],
            ShoalDirection::ASW => [false, false, false, true],
            ShoalDirection::AW => [false, false, false, false],
            ShoalDirection::C => [false, false, false, false],
            ShoalDirection::E => [false, true, false, false],
            ShoalDirection::EAS => [false, true, false, false],
            ShoalDirection::EASAW => [false, true, false, false],
            ShoalDirection::EASW => [false, true, false, true],
            ShoalDirection::EAW => [false, true, false, false],
            ShoalDirection::ES => [false, true, true, false],
            ShoalDirection::ESAW => [false, true, true, false],
            ShoalDirection::ESW => [false, true, true, true],
            ShoalDirection::EW => [false, true, false, true],
            ShoalDirection::N => [true, false, false, false],
            ShoalDirection::NAE => [true, false, false, false],
            ShoalDirection::NAEAS => [true, false, false, false],
            ShoalDirection::NAEASAW => [true, false, false, false],
            ShoalDirection::NAEASW => [true, false, false, true],
            ShoalDirection::NAEAW => [true, false, false, false],
            ShoalDirection::NAES => [true, false, true, false],
            ShoalDirection::NAESAW => [true, false, true, false],
            ShoalDirection::NAESW => [true, false, true, true],
            ShoalDirection::NAEW => [true, false, false, true],
            ShoalDirection::NAS => [true, false, false, false],
            ShoalDirection::NASAW => [true, false, false, false],
            ShoalDirection::NASW => [true, false, false, true],
            ShoalDirection::NAW => [true, false, false, false],
            ShoalDirection::NE => [true, true, false, false],
            ShoalDirection::NEAS => [true, true, false, false],
            ShoalDirection::NEASAW => [true, true, false, false],
            ShoalDirection::NEASW => [true, true, false, true],
            ShoalDirection::NEAW => [true, true, false, false],
            ShoalDirection::NES => [true, true, true, false],
            ShoalDirection::NESAW => [true, true, true, false],
            ShoalDirection::NESW => [true, true, true, true],
            ShoalDirection::NEW => [true, true, false, true],
            ShoalDirection::NS => [true, false, true, false],
            ShoalDirection::NSAW => [true, false, true, false],
            ShoalDirection::NSW => [true, false, true, true],
            ShoalDirection::NW => [true, false, false, true],
            ShoalDirection::S => [false, false, true, false],
            ShoalDirection::SAW => [false, false, true, false],
            ShoalDirection::SW => [false, false, true, true],
            ShoalDirection::W => [false, false, false, true],
        }
    }
}

/// Pipe configurations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[cfg_attr(feature = "bevy", derive(bevy::reflect::Reflect))]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[cfg_attr(feature = "bevy", derive(bevy::reflect::Reflect))]
pub enum PipeSeamType {
    Horizontal,
    Vertical,
}

/// Pipe rubble types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[cfg_attr(feature = "bevy", derive(bevy::reflect::Reflect))]
pub enum PipeRubbleType {
    Horizontal,
    Vertical,
}

/// Property types combining building type and owner
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[cfg_attr(feature = "bevy", derive(bevy::reflect::Reflect))]
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

    /// Return this property owned by `faction`, preserving the building kind.
    ///
    /// An HQ can never be neutral, so remapping an HQ to [`Faction::Neutral`]
    /// leaves its owner unchanged.
    pub const fn with_owner(&self, faction: Faction) -> Property {
        match self {
            Property::City(_) => Property::City(faction),
            Property::Base(_) => Property::Base(faction),
            Property::Airport(_) => Property::Airport(faction),
            Property::Port(_) => Property::Port(faction),
            Property::ComTower(_) => Property::ComTower(faction),
            Property::Lab(_) => Property::Lab(faction),
            Property::HQ(existing) => match faction {
                Faction::Player(player) => Property::HQ(player),
                Faction::Neutral => Property::HQ(*existing),
            },
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

    /// Defense star bonus for units occupying this property tile.
    pub const fn defense_stars(&self) -> u8 {
        match self {
            Property::HQ(_) => 4,
            _ => 3,
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
                PlayerFaction::AmberBlossom => "Amber Blossom City",
                PlayerFaction::JadeSun => "Jade Sun City",
                PlayerFaction::CobaltIce => "Cobalt Ice City",
                PlayerFaction::PinkCosmos => "Pink Cosmos City",
                PlayerFaction::TealGalaxy => "Teal Galaxy City",
                PlayerFaction::PurpleLightning => "Purple Lightning City",
                PlayerFaction::AcidRain => "Acid Rain City",
                PlayerFaction::UmberWilds => "Umber Wilds City",
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
                PlayerFaction::AmberBlossom => "Amber Blossom Base",
                PlayerFaction::JadeSun => "Jade Sun Base",
                PlayerFaction::CobaltIce => "Cobalt Ice Base",
                PlayerFaction::PinkCosmos => "Pink Cosmos Base",
                PlayerFaction::TealGalaxy => "Teal Galaxy Base",
                PlayerFaction::PurpleLightning => "Purple Lightning Base",
                PlayerFaction::AcidRain => "Acid Rain Base",
                PlayerFaction::UmberWilds => "Umber Wilds Base",
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
                PlayerFaction::AmberBlossom => "Amber Blossom Airport",
                PlayerFaction::JadeSun => "Jade Sun Airport",
                PlayerFaction::CobaltIce => "Cobalt Ice Airport",
                PlayerFaction::PinkCosmos => "Pink Cosmos Airport",
                PlayerFaction::TealGalaxy => "Teal Galaxy Airport",
                PlayerFaction::PurpleLightning => "Purple Lightning Airport",
                PlayerFaction::AcidRain => "Acid Rain Airport",
                PlayerFaction::UmberWilds => "Umber Wilds Airport",
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
                PlayerFaction::AmberBlossom => "Amber Blossom Port",
                PlayerFaction::JadeSun => "Jade Sun Port",
                PlayerFaction::CobaltIce => "Cobalt Ice Port",
                PlayerFaction::PinkCosmos => "Pink Cosmos Port",
                PlayerFaction::TealGalaxy => "Teal Galaxy Port",
                PlayerFaction::PurpleLightning => "Purple Lightning Port",
                PlayerFaction::AcidRain => "Acid Rain Port",
                PlayerFaction::UmberWilds => "Umber Wilds Port",
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
                PlayerFaction::AmberBlossom => "Amber Blossom Com Tower",
                PlayerFaction::JadeSun => "Jade Sun Com Tower",
                PlayerFaction::CobaltIce => "Cobalt Ice Com Tower",
                PlayerFaction::PinkCosmos => "Pink Cosmos Com Tower",
                PlayerFaction::TealGalaxy => "Teal Galaxy Com Tower",
                PlayerFaction::PurpleLightning => "Purple Lightning Com Tower",
                PlayerFaction::AcidRain => "Acid Rain Com Tower",
                PlayerFaction::UmberWilds => "Umber Wilds Com Tower",
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
                PlayerFaction::AmberBlossom => "Amber Blossom Lab",
                PlayerFaction::JadeSun => "Jade Sun Lab",
                PlayerFaction::CobaltIce => "Cobalt Ice Lab",
                PlayerFaction::PinkCosmos => "Pink Cosmos Lab",
                PlayerFaction::TealGalaxy => "Teal Galaxy Lab",
                PlayerFaction::PurpleLightning => "Purple Lightning Lab",
                PlayerFaction::AcidRain => "Acid Rain Lab",
                PlayerFaction::UmberWilds => "Umber Wilds Lab",
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
                PlayerFaction::AmberBlossom => "Amber Blossom HQ",
                PlayerFaction::JadeSun => "Jade Sun HQ",
                PlayerFaction::CobaltIce => "Cobalt Ice HQ",
                PlayerFaction::PinkCosmos => "Pink Cosmos HQ",
                PlayerFaction::TealGalaxy => "Teal Galaxy HQ",
                PlayerFaction::PurpleLightning => "Purple Lightning HQ",
                PlayerFaction::AcidRain => "Acid Rain HQ",
                PlayerFaction::UmberWilds => "Umber Wilds HQ",
                PlayerFaction::WhiteNova => "White Nova HQ",
                PlayerFaction::AzureAsteroid => "Azure Asteroid HQ",
                PlayerFaction::NoirEclipse => "Noir Eclipse HQ",
                PlayerFaction::SilverClaw => "Silver Claw HQ",
            },
        }
    }
}

/// Property types
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    strum::VariantArray,
)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
#[serde(rename_all = "kebab-case")]
pub enum PropertyKind {
    Airport,
    Base,
    City,
    ComTower,
    // Two letters that stand for two words, which kebab case would break in
    // half.
    #[serde(rename = "hq")]
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

    /// The name of the building, without an owner in front of it.
    pub const fn name(&self) -> &'static str {
        match self {
            PropertyKind::Airport => "Airport",
            PropertyKind::Base => "Base",
            PropertyKind::City => "City",
            PropertyKind::ComTower => "Com Tower",
            PropertyKind::HQ => "HQ",
            PropertyKind::Lab => "Lab",
            PropertyKind::Port => "Port",
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

impl GameplayTerrain {
    /// The kind of terrain, with no drawing detail and no owner.
    ///
    /// This is the name a player thinks in. The map format writes the tile it
    /// draws, so it calls one shoal `HShoal` and one headquarters
    /// `Orange Star HQ`; a readout that names the tile says `Shoal` and `HQ`,
    /// and lets the sprite carry the shape and the army colour.
    pub const fn type_name(&self) -> &'static str {
        match self {
            GameplayTerrain::Plain => "Plain",
            GameplayTerrain::Mountain => "Mountain",
            GameplayTerrain::Wood => "Wood",
            GameplayTerrain::River => "River",
            GameplayTerrain::Road => "Road",
            GameplayTerrain::Bridge => "Bridge",
            GameplayTerrain::Sea => "Sea",
            GameplayTerrain::Shoal => "Shoal",
            GameplayTerrain::Reef => "Reef",
            GameplayTerrain::Property(property) => property.kind().name(),
            GameplayTerrain::Pipe => "Pipe",
            GameplayTerrain::PipeSeam => "Pipe Seam",
            GameplayTerrain::PipeRubble => "Pipe Rubble",
            // A silo that has fired keeps its name. The empty silo is drawn as
            // its own tile, so the art says which one this is.
            GameplayTerrain::MissileSilo(_) => "Silo",
            GameplayTerrain::Teleporter => "Teleporter",
        }
    }
}

/// Terrain that represents the graphical representation. One can have tall
/// mountains and stubby mountains, but functionally they act the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[cfg_attr(feature = "bevy", derive(bevy::reflect::Reflect))]
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

    /// Defense star bonus applied when a unit is on this terrain type.
    pub const fn defense_stars(self) -> u8 {
        match self {
            GraphicalTerrain::Plain | GraphicalTerrain::Reef => 1,
            GraphicalTerrain::Mountain | GraphicalTerrain::StubbyMoutain => 4,
            GraphicalTerrain::Wood => 2,
            GraphicalTerrain::Property(p) => p.defense_stars(),
            GraphicalTerrain::MissileSilo(_) => 3,
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_defense_stars() {
        assert_eq!(
            Property::HQ(PlayerFaction::OrangeStar).defense_stars(),
            4,
            "HQ gives 4 defense stars"
        );
        assert_eq!(
            Property::City(Faction::Neutral).defense_stars(),
            3,
            "City gives 3 defense stars"
        );
        assert_eq!(
            Property::Base(Faction::Neutral).defense_stars(),
            3,
            "Base gives 3 defense stars"
        );
        assert_eq!(
            Property::Airport(Faction::Neutral).defense_stars(),
            3,
            "Airport gives 3 defense stars"
        );
        assert_eq!(
            Property::Port(Faction::Neutral).defense_stars(),
            3,
            "Port gives 3 defense stars"
        );
    }

    #[test]
    fn graphical_terrain_defense_stars() {
        assert_eq!(GraphicalTerrain::Plain.defense_stars(), 1);
        assert_eq!(GraphicalTerrain::Reef.defense_stars(), 1);
        assert_eq!(GraphicalTerrain::Mountain.defense_stars(), 4);
        assert_eq!(GraphicalTerrain::StubbyMoutain.defense_stars(), 4);
        assert_eq!(GraphicalTerrain::Wood.defense_stars(), 2);
        assert_eq!(
            GraphicalTerrain::Property(Property::HQ(PlayerFaction::OrangeStar)).defense_stars(),
            4
        );
        assert_eq!(
            GraphicalTerrain::Property(Property::City(Faction::Neutral)).defense_stars(),
            3
        );
        assert_eq!(
            GraphicalTerrain::MissileSilo(MissileSiloStatus::Loaded).defense_stars(),
            3
        );
        assert_eq!(
            GraphicalTerrain::Sea(SeaDirection::N_E_S_W).defense_stars(),
            0
        );
        assert_eq!(
            GraphicalTerrain::River(RiverType::Horizontal).defense_stars(),
            0
        );
        assert_eq!(
            GraphicalTerrain::Road(RoadType::Horizontal).defense_stars(),
            0
        );
    }
}
