use crate::results::{MatchResults, SeatExits, match_results};
use crate::view::{self, CommandResult, PlayerView, SpectatorView};
use awbrn_game::{
    Authority, CommandError, GameCommand, GameSetup, PlayerId, ReplayEventError, ServerUnitId,
    SetupError, StoredActionEvent,
};
use awbrn_map::Pos;
use awbrn_types::{PlayerFaction, Unit};
use awvm::semantic::{AwbwVisibility, Observation, observe};

/// Authoritative game server driven by AWVM.
///
/// A clone is a position a caller can return to. It copies the recipient ID
/// allocators as well as the board, so a unit an opponent was shown keeps the
/// opaque ID it was shown under.
#[derive(Clone, Debug)]
pub struct GameServer {
    authority: Authority,
    unit_ids: view::RecipientUnitIds,
    exits: SeatExits,
}

impl GameServer {
    /// Create a new game server with the given configuration.
    pub fn new(setup: GameSetup) -> Result<Self, SetupError> {
        let authority = Authority::new(&setup)?;
        Ok(Self {
            authority,
            unit_ids: view::RecipientUnitIds::default(),
            exits: SeatExits::default(),
        })
    }

    /// Submit a command from a player. Returns per-player updates on success.
    pub fn submit_command(
        &mut self,
        player: PlayerId,
        command: GameCommand,
    ) -> Result<CommandResult, CommandError> {
        let transition = self.authority.execute(player, &command)?;
        self.exits
            .observe(self.authority.state(), &transition.events);
        Ok(view::build_command_result(
            &self.authority,
            &transition,
            &mut self.unit_ids,
        ))
    }

    pub(crate) fn replay_stored_action_event(
        &mut self,
        event: &StoredActionEvent,
    ) -> Result<(), ReplayEventError> {
        let transition =
            self.authority
                .execute_recorded(event.player, &event.command, &event.random)?;
        self.exits
            .observe(self.authority.state(), &transition.events);
        // Discard the result, but advance recipient ID allocators. This keeps
        // opaque IDs equal between replay and live servers.
        view::build_command_result(&self.authority, &transition, &mut self.unit_ids);
        Ok(())
    }

    /// Replay one stored action and return the same typed projections that a
    /// live submission produces.
    pub fn replay_stored_action_event_observations(
        &mut self,
        event: &StoredActionEvent,
    ) -> Result<Vec<(PlayerId, awvm::semantic::ObservedTransition)>, ReplayEventError> {
        let transition =
            self.authority
                .execute_recorded(event.player, &event.command, &event.random)?;
        self.exits
            .observe(self.authority.state(), &transition.events);
        let result = view::build_command_result(&self.authority, &transition, &mut self.unit_ids);
        Ok(result.observed_transitions)
    }

    /// Return the opening observation for every player in slot order.
    pub fn player_observations(&self) -> Vec<(PlayerId, Observation)> {
        self.authority
            .players()
            .filter_map(|player| Some((player, self.player_observation(player)?)))
            .collect()
    }

    /// Return results for a finished non-cancelled match.
    pub fn results(&self) -> Option<MatchResults> {
        match_results(self.authority.state(), &self.exits)
    }

    /// Get the full visible state for a player (for initial load or reconnection).
    pub fn player_view(&mut self, player: PlayerId) -> Option<PlayerView> {
        view::build_player_view(&self.authority, &mut self.unit_ids, player)
    }

    /// The seat whose turn is open, or `None` once the match is over.
    ///
    /// The host asks this to find out whether it owes a turn to a seat it
    /// plays itself. A finished match owes nobody a turn, which is why the
    /// answer is one value rather than a slot and a phase the caller has to
    /// read together.
    pub fn active_player(&self) -> Option<PlayerId> {
        let state = self.authority.state();
        if matches!(state.match_state, awvm::semantic::Match::Finished { .. }) {
            return None;
        }
        let active = &state.turn.active_player;
        self.authority
            .players()
            .find(|player| self.authority.player(*player) == *active)
    }

    /// The authoritative position.
    ///
    /// For a host that has to spell a command against the true board, which a
    /// server-played seat does. A recipient's own board is
    /// [`Self::player_observation`], and nothing that answers a client may
    /// read this one.
    pub fn state(&self) -> &awvm::semantic::State {
        self.authority.state()
    }

    /// Get the typed recipient-safe state used by presentation clients.
    pub fn player_observation(&self, player: PlayerId) -> Option<Observation> {
        let recipient = self.authority.player(player);
        self.authority.state().find_player(&recipient)?;
        observe(&AwbwVisibility, self.authority.state(), &recipient).ok()
    }

    /// The player slot whose recipient projection stands in for the spectator
    /// view. Callers that pair an observation with a transition must use this
    /// slot for both.
    pub fn spectator_player(&self) -> Option<PlayerId> {
        if self.authority.state().settings.fog {
            return None;
        }
        self.authority.players().next()
    }

    /// A non-fog spectator may use any recipient projection: all board and unit
    /// facts are public when fog is disabled. Fog matches have no spectator
    /// projection, because every recipient view hides something.
    pub fn spectator_observation(&self) -> Option<Observation> {
        self.spectator_player()
            .and_then(|player| self.player_observation(player))
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
        position: Pos,
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
