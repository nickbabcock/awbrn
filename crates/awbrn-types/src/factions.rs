use crate::AwbwFactionId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, strum::VariantArray)]
#[cfg_attr(feature = "bevy", derive(bevy::reflect::Reflect))]
pub enum PlayerFaction {
    AcidRain = 0,
    AmberBlossom,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerFactionMetadata {
    faction: PlayerFaction,
    awbw_id: AwbwFactionId,
    country_code: &'static str,
    name: &'static str,
    faces_right: bool,
}

impl PlayerFactionMetadata {
    pub const fn new(
        faction: PlayerFaction,
        awbw_id: AwbwFactionId,
        country_code: &'static str,
        name: &'static str,
        faces_right: bool,
    ) -> Self {
        Self {
            faction,
            awbw_id,
            country_code,
            name,
            faces_right,
        }
    }

    pub const fn faction(&self) -> PlayerFaction {
        self.faction
    }

    pub const fn awbw_id(&self) -> AwbwFactionId {
        self.awbw_id
    }

    pub const fn country_code(&self) -> &'static str {
        self.country_code
    }

    pub const fn name(&self) -> &'static str {
        self.name
    }

    pub const fn faces_right(&self) -> bool {
        self.faces_right
    }
}

include!("generated/factions.rs");

impl PlayerFaction {
    /// Get the display name of this faction
    pub const fn name(&self) -> &'static str {
        player_faction_name(*self)
    }

    /// Get the canonical app faction id.
    ///
    /// This id is currently numerically equivalent to the AWBW faction id.
    pub const fn id(&self) -> u8 {
        player_faction_id(*self)
    }

    /// Create a PlayerFaction from the canonical app faction id.
    ///
    /// This currently accepts the same values as [`Self::from_awbw_id`].
    pub fn from_id(id: u8) -> Option<Self> {
        player_faction_from_id(id)
    }

    /// Parse a country code into a PlayerFaction
    pub fn from_country_code(code: &str) -> Option<Self> {
        player_faction_from_country_code(code)
    }

    /// Returns the faction's country code
    pub const fn country_code(&self) -> &'static str {
        player_faction_country_code(*self)
    }

    /// Get the AWBW country id
    ///
    /// Ref: https://github.com/DeamonHunter/AWBW-Replay-Player/blob/245879fd2b7d6286476fc8b21619dab25128daf0/AWBWApp.Resources/Json/Countries.json
    pub const fn awbw_id(&self) -> AwbwFactionId {
        player_faction_awbw_id(*self)
    }

    /// Create a PlayerFaction from an AWBW faction ID
    pub fn from_awbw_id(id: u8) -> Option<Self> {
        player_faction_from_awbw_id(id)
    }

    /// Get the previous player faction alphabetically
    #[inline]
    pub const fn prev(&self) -> PlayerFaction {
        match self {
            PlayerFaction::AcidRain => PlayerFaction::YellowComet,
            PlayerFaction::AmberBlossom => PlayerFaction::AcidRain,
            PlayerFaction::AzureAsteroid => PlayerFaction::AmberBlossom,
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

    /// Returns whether this faction's default unit facing direction is right.
    ///
    /// Ref: `AWBW-Replay-Player/AWBWApp.Resources/Json/Countries.json`
    pub const fn faces_right(&self) -> bool {
        player_faction_faces_right(*self)
    }
}

/// Army factions in the game
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "bevy", derive(bevy::reflect::Reflect))]
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

#[cfg(test)]
mod tests {
    use super::PlayerFaction;
    use strum::VariantArray;

    #[test]
    fn awbw_id_round_trips_for_all_factions() {
        for faction in PlayerFaction::VARIANTS {
            assert_eq!(
                PlayerFaction::from_awbw_id(faction.awbw_id().as_u8()),
                Some(*faction)
            );
        }
    }

    #[test]
    fn canonical_id_round_trips_for_all_factions() {
        for faction in PlayerFaction::VARIANTS {
            assert_eq!(PlayerFaction::from_id(faction.id()), Some(*faction));
        }
    }

    #[test]
    fn country_code_round_trips_for_all_factions() {
        for faction in PlayerFaction::VARIANTS {
            assert_eq!(
                PlayerFaction::from_country_code(faction.country_code()),
                Some(*faction)
            );
        }
    }
}
