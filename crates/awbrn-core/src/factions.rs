#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayerFaction {
    AcidRain,
    AmberBlaze,
    AzureAsteroid,
    BlackHole,
    BlueMoon,
    BrownDesert,
    CobaltIce,
    GreenEarth,
    GreySky,
    JadeSun,
    NoirEclipse,
    OrangeStar,
    PinkCosmos,
    PurpleLightning,
    RedFire,
    SilverClaw,
    TealGalaxy,
    WhiteNova,
    YellowComet,
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

    /// Parse a country code into a PlayerFaction
    pub fn from_country_code(code: &str) -> Option<Self> {
        match code {
            "ar" => Some(PlayerFaction::AcidRain),
            "ab" => Some(PlayerFaction::AmberBlaze),
            "aa" => Some(PlayerFaction::AzureAsteroid),
            "bh" => Some(PlayerFaction::BlackHole),
            "bm" => Some(PlayerFaction::BlueMoon),
            "bd" => Some(PlayerFaction::BrownDesert),
            "ci" => Some(PlayerFaction::CobaltIce),
            "ge" => Some(PlayerFaction::GreenEarth),
            "gs" => Some(PlayerFaction::GreySky),
            "js" => Some(PlayerFaction::JadeSun),
            "ne" => Some(PlayerFaction::NoirEclipse),
            "os" => Some(PlayerFaction::OrangeStar),
            "pc" => Some(PlayerFaction::PinkCosmos),
            "pl" => Some(PlayerFaction::PurpleLightning),
            "rf" => Some(PlayerFaction::RedFire),
            "sc" => Some(PlayerFaction::SilverClaw),
            "tg" => Some(PlayerFaction::TealGalaxy),
            "wn" => Some(PlayerFaction::WhiteNova),
            "yc" => Some(PlayerFaction::YellowComet),
            _ => None,
        }
    }

    /// Returns the faction's country code
    pub const fn to_country_code(&self) -> &'static str {
        match self {
            PlayerFaction::AcidRain => "ar",
            PlayerFaction::AmberBlaze => "ab",
            PlayerFaction::AzureAsteroid => "aa",
            PlayerFaction::BlackHole => "bh",
            PlayerFaction::BlueMoon => "bm",
            PlayerFaction::BrownDesert => "bd",
            PlayerFaction::CobaltIce => "ci",
            PlayerFaction::GreenEarth => "ge",
            PlayerFaction::GreySky => "gs",
            PlayerFaction::JadeSun => "js",
            PlayerFaction::NoirEclipse => "ne",
            PlayerFaction::OrangeStar => "os",
            PlayerFaction::PinkCosmos => "pc",
            PlayerFaction::PurpleLightning => "pl",
            PlayerFaction::RedFire => "rf",
            PlayerFaction::SilverClaw => "sc",
            PlayerFaction::TealGalaxy => "tg",
            PlayerFaction::WhiteNova => "wn",
            PlayerFaction::YellowComet => "yc",
        }
    }

    /// Get the previous player faction alphabetically
    #[inline]
    pub const fn prev(&self) -> PlayerFaction {
        match self {
            PlayerFaction::AcidRain => PlayerFaction::YellowComet,
            PlayerFaction::AmberBlaze => PlayerFaction::AcidRain,
            PlayerFaction::AzureAsteroid => PlayerFaction::AmberBlaze,
            PlayerFaction::BlackHole => PlayerFaction::AzureAsteroid,
            PlayerFaction::BlueMoon => PlayerFaction::BlackHole,
            PlayerFaction::BrownDesert => PlayerFaction::BlueMoon,
            PlayerFaction::CobaltIce => PlayerFaction::BrownDesert,
            PlayerFaction::GreenEarth => PlayerFaction::CobaltIce,
            PlayerFaction::GreySky => PlayerFaction::GreenEarth,
            PlayerFaction::JadeSun => PlayerFaction::GreySky,
            PlayerFaction::NoirEclipse => PlayerFaction::JadeSun,
            PlayerFaction::OrangeStar => PlayerFaction::NoirEclipse,
            PlayerFaction::PinkCosmos => PlayerFaction::OrangeStar,
            PlayerFaction::PurpleLightning => PlayerFaction::PinkCosmos,
            PlayerFaction::RedFire => PlayerFaction::PurpleLightning,
            PlayerFaction::SilverClaw => PlayerFaction::RedFire,
            PlayerFaction::TealGalaxy => PlayerFaction::SilverClaw,
            PlayerFaction::WhiteNova => PlayerFaction::TealGalaxy,
            PlayerFaction::YellowComet => PlayerFaction::WhiteNova,
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
