use crate::AwbwFactionId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "bevy", derive(bevy::reflect::Reflect))]
pub enum PlayerFaction {
    AcidRain = 0,
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
    UmberWilds,
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
            PlayerFaction::UmberWilds => "Umber Wilds",
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
            "uw" => Some(PlayerFaction::UmberWilds),
            "wn" => Some(PlayerFaction::WhiteNova),
            "yc" => Some(PlayerFaction::YellowComet),
            _ => None,
        }
    }

    /// Returns the faction's country code
    pub const fn country_code(&self) -> &'static str {
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
            PlayerFaction::UmberWilds => "uw",
            PlayerFaction::WhiteNova => "wn",
            PlayerFaction::YellowComet => "yc",
        }
    }

    /// Get the AWBW country id
    ///
    /// Ref: https://github.com/DeamonHunter/AWBW-Replay-Player/blob/245879fd2b7d6286476fc8b21619dab25128daf0/AWBWApp.Resources/Json/Countries.json
    pub const fn awbw_id(&self) -> AwbwFactionId {
        match self {
            PlayerFaction::OrangeStar => AwbwFactionId::new(1),
            PlayerFaction::BlueMoon => AwbwFactionId::new(2),
            PlayerFaction::GreenEarth => AwbwFactionId::new(3),
            PlayerFaction::YellowComet => AwbwFactionId::new(4),
            PlayerFaction::BlackHole => AwbwFactionId::new(5),
            PlayerFaction::RedFire => AwbwFactionId::new(6),
            PlayerFaction::GreySky => AwbwFactionId::new(7),
            PlayerFaction::BrownDesert => AwbwFactionId::new(8),
            PlayerFaction::AmberBlaze => AwbwFactionId::new(9),
            PlayerFaction::JadeSun => AwbwFactionId::new(10),
            PlayerFaction::CobaltIce => AwbwFactionId::new(16),
            PlayerFaction::PinkCosmos => AwbwFactionId::new(17),
            PlayerFaction::TealGalaxy => AwbwFactionId::new(19),
            PlayerFaction::PurpleLightning => AwbwFactionId::new(20),
            PlayerFaction::AcidRain => AwbwFactionId::new(21),
            PlayerFaction::WhiteNova => AwbwFactionId::new(22),
            PlayerFaction::AzureAsteroid => AwbwFactionId::new(23),
            PlayerFaction::NoirEclipse => AwbwFactionId::new(24),
            PlayerFaction::SilverClaw => AwbwFactionId::new(25),
            PlayerFaction::UmberWilds => AwbwFactionId::new(26),
        }
    }

    /// Create a PlayerFaction from an AWBW faction ID
    pub fn from_awbw_id(id: u8) -> Option<Self> {
        match id {
            1 => Some(PlayerFaction::OrangeStar),
            2 => Some(PlayerFaction::BlueMoon),
            3 => Some(PlayerFaction::GreenEarth),
            4 => Some(PlayerFaction::YellowComet),
            5 => Some(PlayerFaction::BlackHole),
            6 => Some(PlayerFaction::RedFire),
            7 => Some(PlayerFaction::GreySky),
            8 => Some(PlayerFaction::BrownDesert),
            9 => Some(PlayerFaction::AmberBlaze),
            10 => Some(PlayerFaction::JadeSun),
            16 => Some(PlayerFaction::CobaltIce),
            17 => Some(PlayerFaction::PinkCosmos),
            19 => Some(PlayerFaction::TealGalaxy),
            20 => Some(PlayerFaction::PurpleLightning),
            21 => Some(PlayerFaction::AcidRain),
            22 => Some(PlayerFaction::WhiteNova),
            23 => Some(PlayerFaction::AzureAsteroid),
            24 => Some(PlayerFaction::NoirEclipse),
            25 => Some(PlayerFaction::SilverClaw),
            26 => Some(PlayerFaction::UmberWilds),
            _ => None,
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
            PlayerFaction::UmberWilds => PlayerFaction::TealGalaxy,
            PlayerFaction::WhiteNova => PlayerFaction::UmberWilds,
            PlayerFaction::YellowComet => PlayerFaction::WhiteNova,
        }
    }

    #[inline]
    pub const fn index(&self) -> u8 {
        *self as u8
    }
}

/// Army factions in the game
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Faction {
    Neutral,
    Player(PlayerFaction),
}

impl From<PlayerFaction> for Faction {
    fn from(faction: PlayerFaction) -> Self {
        Faction::Player(faction)
    }
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
