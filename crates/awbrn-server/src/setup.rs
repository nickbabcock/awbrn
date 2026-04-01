use std::num::NonZeroU8;

use awbrn_map::AwbrnMap;
use awbrn_types::PlayerFaction;
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
}

/// Configuration for creating a new game.
#[derive(Debug)]
pub struct GameSetup {
    pub map: AwbrnMap,
    pub players: Vec<PlayerSetup>,
    pub fog_enabled: bool,
}

/// Error returned when a game cannot be initialized from the provided setup.
#[derive(Debug, Clone, PartialEq, Eq)]
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
        })
        .collect();

    let first_player = players
        .first()
        .expect("game must have at least one player")
        .id;

    world.insert_resource(PlayerRegistry::new(players));

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
