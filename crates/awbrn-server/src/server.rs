use bevy::prelude::*;

use crate::apply;
use crate::command::GameCommand;
use crate::error::CommandError;
use crate::player::PlayerId;
use crate::setup::{GameSetup, SetupError, initialize_server_world};
use crate::unit_id::ServerUnitId;
use crate::validate;
use crate::view::{self, CommandResult, PlayerView};
use awbrn_game::world::StrongIdMap;
use awbrn_map::Position;

/// Authoritative game server that owns a Bevy World and processes player commands.
pub struct GameServer {
    world: World,
}

impl GameServer {
    /// Create a new game server with the given configuration.
    pub fn new(setup: GameSetup) -> Result<Self, SetupError> {
        let world = initialize_server_world(setup)?;
        Ok(Self { world })
    }

    /// Submit a command from a player. Returns per-player updates on success.
    pub fn submit_command(
        &mut self,
        player: PlayerId,
        command: GameCommand,
    ) -> Result<CommandResult, CommandError> {
        // Validate the command.
        validate::validate_command(&mut self.world, player, &command)?;

        // Snapshot fog state before applying.
        let pre_fog = view::snapshot_pre_fog(&mut self.world);

        // Apply the command.
        let outcome = apply::apply_command(&mut self.world, &command);

        // Build per-player updates.
        let result = view::build_command_result(&mut self.world, &outcome, &pre_fog);
        Ok(result)
    }

    /// Get the full visible state for a player (for initial load or reconnection).
    pub fn player_view(&mut self, player: PlayerId) -> PlayerView {
        view::build_player_view(&mut self.world, player)
    }

    /// Spawn a unit into the game world. Returns the assigned [`ServerUnitId`].
    pub fn spawn_unit(
        &mut self,
        position: Position,
        unit_type: awbrn_types::Unit,
        faction: awbrn_types::PlayerFaction,
    ) -> ServerUnitId {
        let id = self
            .world
            .resource_mut::<crate::state::ServerGameState>()
            .allocate_unit_id();

        let entity = self
            .world
            .spawn((
                awbrn_game::MapPosition::from(position),
                awbrn_game::world::Unit(unit_type),
                awbrn_game::world::Faction(faction),
                awbrn_game::world::GraphicalHp(10),
                awbrn_game::world::Fuel(unit_type.max_fuel()),
                awbrn_game::world::Ammo(unit_type.max_ammo()),
                awbrn_game::world::VisionRange(unit_type.base_vision()),
                awbrn_game::world::UnitActive,
                id,
            ))
            .id();

        self.world
            .resource_mut::<StrongIdMap<ServerUnitId>>()
            .insert(id, entity);

        id
    }
}
