use crate::command::GameCommand;
use crate::error::CommandError;
use crate::player::PlayerId;
use crate::replay::{ReplayEventError, StoredActionEvent};
use crate::setup::{GameSetup, SetupError};
use crate::unit_id::ServerUnitId;
use crate::view::{self, CommandResult, PlayerView, SpectatorView};

use awbrn_map::Position;
use awbrn_types::{PlayerFaction, Unit};

/// Authoritative game server driven by AWVM.
pub struct GameServer {
    authority: crate::awvm_adapter::Authority,
}

impl GameServer {
    /// Create a new game server with the given configuration.
    pub fn new(setup: GameSetup) -> Result<Self, SetupError> {
        let authority = crate::awvm_adapter::Authority::new(&setup)?;
        Ok(Self { authority })
    }

    /// Submit a command from a player. Returns per-player updates on success.
    pub fn submit_command(
        &mut self,
        player: PlayerId,
        command: GameCommand,
    ) -> Result<CommandResult, CommandError> {
        let transition = self.authority.execute(player, &command)?;
        Ok(view::build_command_result(&self.authority, &transition))
    }

    pub(crate) fn replay_stored_action_event(
        &mut self,
        event: &StoredActionEvent,
    ) -> Result<(), ReplayEventError> {
        self.authority
            .execute_recorded(event.player, &event.command, &event.random)?;
        Ok(())
    }

    /// Get the full visible state for a player (for initial load or reconnection).
    pub fn player_view(&self, player: PlayerId) -> Option<PlayerView> {
        view::build_player_view(&self.authority, player)
    }

    /// Get the full public state for a non-fog spectator.
    pub fn spectator_view(&self) -> SpectatorView {
        view::build_spectator_view(&self.authority)
    }

    pub fn has_player(&self, player: PlayerId) -> bool {
        self.authority
            .state()
            .find_player(&self.authority.player(player))
            .is_some()
    }

    /// Every random token drawn by AWVM since this server was created.
    pub fn recorded_random(&self) -> &[awvm::random::RandomToken] {
        self.authority.random_tokens()
    }

    /// Random tokens drawn while executing the most recently accepted command.
    pub fn last_random(&self) -> &[awvm::random::RandomToken] {
        self.authority.last_random_tokens()
    }

    /// Spawn a unit into the game world. Returns the assigned [`ServerUnitId`].
    pub fn spawn_unit(
        &mut self,
        position: Position,
        unit_type: Unit,
        faction: PlayerFaction,
    ) -> ServerUnitId {
        let id = ServerUnitId(
            self.authority
                .state()
                .next_unit_id
                .expect("server states allocate unit ids")
                .into(),
        );
        self.authority
            .spawn_unit(id, position, unit_type, faction, true);
        id
    }
}
