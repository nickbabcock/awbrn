use std::num::NonZeroU8;

use awbrn_map::AwbrnMap;
use awbrn_types::{Co, PlayerFaction};
use bevy::prelude::*;

use crate::player::{PlayerId, PlayerRegistry, PlayerSlot};
use crate::state::{ServerGameState, TurnPhase};
use crate::unit_id::ServerUnitId;
use awbrn_game::world::{GameMap, StrongIdMap};

/// Configuration for a single player joining a game.
#[derive(Debug, Clone)]
pub struct PlayerSetup {
    pub faction: PlayerFaction,
    /// Team identifier. `None` means FFA (no team).
    pub team: Option<NonZeroU8>,
    pub starting_funds: u32,
    pub co: Co,
}

/// Configuration for creating a new game.
#[derive(Debug, Clone)]
pub struct GameSetup {
    pub map: AwbrnMap,
    pub players: Vec<PlayerSetup>,
    pub fog_enabled: bool,
    pub rng_seed: u64,
}

#[derive(Resource)]
pub struct GameRng {
    state: u64,
}

impl GameRng {
    pub fn from_seed(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9e37_79b9_7f4a_7c15
            } else {
                seed
            },
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }

    /// Returns a uniformly distributed value in `0..=max`.
    pub fn roll(&mut self, max: u8) -> u8 {
        if max == 0 {
            return 0;
        }

        let range = u64::from(max) + 1;
        let max_usable = u64::MAX - (u64::MAX % range);

        loop {
            let sample = self.next_u64();
            if sample < max_usable {
                return (sample % range) as u8;
            }
        }
    }
}

/// Error returned when a game cannot be initialized from the provided setup.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SetupError {
    InvalidPlayers { reason: String },
}

impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPlayers { reason } => write!(f, "invalid game setup: {reason}"),
        }
    }
}

impl std::error::Error for SetupError {}

/// Initialize the server world with all required resources and terrain entities.
pub(crate) fn initialize_server_world(setup: GameSetup) -> Result<World, SetupError> {
    if setup.players.is_empty() {
        return Err(SetupError::InvalidPlayers {
            reason: "game must contain at least one player".into(),
        });
    }
    if setup.players.len() > u8::MAX as usize {
        return Err(SetupError::InvalidPlayers {
            reason: format!(
                "game supports at most {} players, got {}",
                u8::MAX,
                setup.players.len()
            ),
        });
    }

    // Use a temporary App to apply GameWorldPlugin (registers types, observers, resources).
    let mut app = App::new();
    app.add_plugins(awbrn_game::GameWorldPlugin);
    app.finish();
    app.cleanup();

    let mut world = std::mem::take(app.world_mut());

    // Set up the map.
    world.resource_mut::<GameMap>().set(setup.map);
    awbrn_game::world::initialize_terrain_semantic_world(&mut world);

    // Build player registry.
    let players: Vec<PlayerSlot> = setup
        .players
        .iter()
        .enumerate()
        .map(|(i, p)| PlayerSlot {
            id: PlayerId(i as u8),
            faction: p.faction,
            team: p.team,
            funds: p.starting_funds,
            eliminated: false,
            co: p.co,
        })
        .collect();

    let first_player = players
        .first()
        .expect("game must have at least one player")
        .id;

    world.insert_resource(PlayerRegistry::new(players));
    world.insert_resource(GameRng::from_seed(setup.rng_seed));

    // Server game state.
    world.insert_resource(ServerGameState {
        day: 1,
        active_player: first_player,
        phase: TurnPhase::PlayerTurn,
        next_unit_id: 1,
    });

    // Unit ID index.
    world.insert_resource(StrongIdMap::<ServerUnitId>::default());

    // Fog configuration.
    world.resource_mut::<awbrn_game::world::FogActive>().0 = setup.fog_enabled;

    Ok(world)
}
