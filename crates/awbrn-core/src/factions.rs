pub use awbrn_types::PlayerFaction;

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
