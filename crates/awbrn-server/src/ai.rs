//! A seat the server plays for.
//!
//! An AI seat is not a second way to run a match. It submits the same
//! [`GameCommand`]s a person's seat submits, through the same authority, and
//! leaves the same [`StoredActionEvent`](crate::StoredActionEvent) behind, so a
//! match with an opponent in it replays through the one path every match
//! replays through and its result is recorded the one way every result is.
//!
//! The agent decides in AWVM's own vocabulary, against the fog-limited
//! observation its seat is entitled to — never the true state. What it names is
//! spelled against the authority's position and then written back down as a
//! wire command, which is the record.
//!
//! The loop is [`play_ai_turn`]: ask for a command, submit it, say whether it
//! was taken, ask again. The seat says the turn is over by answering `None`,
//! which is what stops a host from having to know when an agent is done. Every
//! host plays a turn through that one function, so the turn a test records
//! natively is the turn the wasm build plays.

use awbrn_ai::{Agent, AiProfile, NodeBudget, REFUSAL_LIMIT};
use awbrn_game::{CommandError, GameCommand, PlayerId, StoredActionEvent, game_command};
use awvm::semantic::Observation;
use awvm::session::Session;

use crate::GameServer;

/// The most commands one turn may issue.
///
/// A turn is bounded by the units on the board, so this is not a rule of the
/// game but a stop on a host that cannot afford an agent that will not finish.
pub const MAX_COMMANDS_PER_TURN: u32 = 512;

/// One seat, playing its own turn.
pub struct AiSeat {
    player: PlayerId,
    agent: Box<dyn Agent>,
    budget: NodeBudget,
    issued: u32,
    refusals: u32,
    finished: bool,
}

impl std::fmt::Debug for AiSeat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiSeat")
            .field("player", &self.player)
            .field("issued", &self.issued)
            .field("refusals", &self.refusals)
            .field("finished", &self.finished)
            .finish()
    }
}

impl AiSeat {
    /// Seat one profile, ready to be asked for the turn it is on.
    ///
    /// The seed decides every choice the agent makes, so a caller that derives
    /// it from the match's own seed gets a seat that plays the same way whether
    /// the match has been open for an hour or was rebuilt from its log.
    pub fn new(player: PlayerId, profile: &AiProfile, seed: u64) -> Self {
        let mut agent = profile.agent(seed);
        agent.start_match();
        Self {
            player,
            agent,
            budget: profile.node_budget(),
            issued: 0,
            refusals: 0,
            finished: false,
        }
    }

    /// Open the turn on the position the seat can see.
    pub fn begin_turn(&mut self, server: &GameServer) {
        if let Some(view) = server.player_observation(self.player) {
            self.agent.start_turn(&view);
        }
    }

    /// The next command to submit, or `None` once the turn is over.
    ///
    /// A play the true state cannot spell is refused here, before it reaches
    /// the authority: the agent chose it against a board it could see, and it
    /// is told so and asked again, as it is for a command the authority
    /// refuses.
    pub fn next_command(&mut self, server: &GameServer) -> Option<GameCommand> {
        // A command can finish the match part way through the turn, and the
        // authority refuses everything after that, end-turn included. A seat
        // whose turn is no longer open has nothing to ask for.
        if self.finished || server.active_player() != Some(self.player) {
            self.finished = true;
            return None;
        }
        loop {
            if self.issued >= MAX_COMMANDS_PER_TURN || self.refusals >= REFUSAL_LIMIT {
                return Some(self.end_turn());
            }

            let Some(view) = server.player_observation(self.player) else {
                return Some(self.end_turn());
            };
            let Some(play) = self.agent.act(&view, self.budget) else {
                return Some(self.end_turn());
            };

            // Spelled against the authority's position, so the route and the
            // target are the board the reducer will validate against.
            let session = Session::new(server.state().clone());
            let Some(command) = play.command(&session) else {
                self.refuse(&view);
                continue;
            };
            let command = match game_command(&command, server.state()) {
                Ok(command) => command,
                Err(error) => {
                    // A command the board accepts but the wire cannot carry is
                    // a fault in the agent or in the conversion, not an answer
                    // from the board. It is logged as an error and fails a
                    // debug build; a release build counts it as a refusal, so
                    // the seat plays on rather than losing its turn.
                    tracing::error!(
                        %error,
                        player = self.player.0,
                        "an AI seat spelled a command with no wire form"
                    );
                    debug_assert!(
                        false,
                        "an AI seat spelled a command with no wire form: {error}"
                    );
                    self.refuse(&view);
                    continue;
                }
            };

            // A seat ends its turn by passing, and only the host may time a
            // seat out. Either spelled as a play is the end of the turn and
            // nothing more.
            if matches!(command, GameCommand::EndTurn | GameCommand::Timeout) {
                return Some(self.end_turn());
            }

            self.issued = self.issued.saturating_add(1);
            return Some(command);
        }
    }

    /// Tell the seat its last command was accepted.
    pub fn accepted(&mut self, server: &GameServer) {
        self.refusals = 0;
        if let Some(view) = server.player_observation(self.player) {
            self.agent.refresh(&view);
        }
    }

    /// Tell the seat its last command was refused.
    pub fn refused(&mut self, server: &GameServer) {
        match server.player_observation(self.player) {
            Some(view) => self.refuse(&view),
            None => self.refusals = self.refusals.saturating_add(1),
        }
    }

    /// Count a refusal and tell the agent, which drops the plan the refused
    /// play came from.
    fn refuse(&mut self, view: &Observation) {
        self.refusals = self.refusals.saturating_add(1);
        self.agent.reject(view);
    }

    fn end_turn(&mut self) -> GameCommand {
        self.finished = true;
        GameCommand::EndTurn
    }
}

/// What a seat's turn is played against: the authority, and the one way a
/// command reaches it.
pub trait SeatHost {
    /// What the host makes of a command the authority took.
    type Accepted;

    /// The authority the seat reads and spells against.
    fn server(&self) -> &GameServer;

    /// Put one command to the authority.
    fn submit(
        &mut self,
        player: PlayerId,
        command: GameCommand,
    ) -> Result<Self::Accepted, CommandError>;
}

/// A bare server keeps the record a match log keeps.
impl SeatHost for GameServer {
    type Accepted = StoredActionEvent;

    fn server(&self) -> &GameServer {
        self
    }

    fn submit(
        &mut self,
        player: PlayerId,
        command: GameCommand,
    ) -> Result<StoredActionEvent, CommandError> {
        self.submit_command(player, command.clone())?;
        Ok(StoredActionEvent {
            player,
            command,
            random: self.last_random().to_vec(),
        })
    }
}

/// Play one whole turn for a seat, and return what the host made of each
/// command the authority took, in the order it took them.
///
/// A refused command is not returned: the seat is told and decides again,
/// because a play chosen against what a seat can see may be refused by the
/// board it cannot, and that is an answer rather than a fault.
pub fn play_ai_turn<H: SeatHost>(
    host: &mut H,
    player: PlayerId,
    profile: &AiProfile,
    seed: u64,
) -> Vec<H::Accepted> {
    let mut seat = AiSeat::new(player, profile, seed);
    seat.begin_turn(host.server());

    let mut accepted = Vec::new();
    while let Some(command) = seat.next_command(host.server()) {
        match host.submit(player, command) {
            Ok(response) => {
                accepted.push(response);
                seat.accepted(host.server());
            }
            Err(error) => {
                tracing::debug!(slot = player.0, %error, "a played seat had a command refused");
                seat.refused(host.server());
            }
        }
    }
    accepted
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use awbrn_ai::board::{SEATS, amber_valley_map};
    use awbrn_ai::{EASY, Play};
    use awbrn_game::{GameSetup, PlayerSetup};
    use awbrn_types::Co;
    use awvm::semantic::UnitId;
    use awvm::session::OrderKind;

    use super::*;

    fn server() -> GameServer {
        GameServer::new(GameSetup {
            map: amber_valley_map(),
            players: SEATS
                .iter()
                .map(|&faction| PlayerSetup {
                    faction,
                    team: None,
                    starting_funds: 10_000,
                    co: Co::Andy,
                })
                .collect(),
            fog_enabled: false,
            rng_seed: 7,
        })
        .expect("the setup is valid")
    }

    /// Plays as Easy does, except that its first `unwritable` plays name a
    /// unit the board does not hold.
    struct Unwritable {
        inner: Box<dyn Agent>,
        unwritable: u32,
        rejects: Rc<Cell<u32>>,
    }

    impl Agent for Unwritable {
        fn act(&mut self, view: &Observation, budget: NodeBudget) -> Option<Play> {
            let play = self.inner.act(view, budget)?;
            if self.unwritable == 0 {
                return Some(play);
            }
            self.unwritable -= 1;
            Some(Play::new(
                UnitId::new(u32::MAX),
                play.destination(),
                OrderKind::Wait,
            ))
        }

        fn refresh(&mut self, view: &Observation) {
            self.inner.refresh(view);
        }

        fn reject(&mut self, view: &Observation) {
            self.rejects.set(self.rejects.get() + 1);
            self.inner.reject(view);
        }
    }

    fn seat(unwritable: u32) -> (AiSeat, Rc<Cell<u32>>) {
        let rejects = Rc::new(Cell::new(0));
        let mut seat = AiSeat::new(PlayerId(0), &EASY, 1);
        seat.agent = Box::new(Unwritable {
            inner: EASY.agent(1),
            unwritable,
            rejects: Rc::clone(&rejects),
        });
        (seat, rejects)
    }

    #[test]
    fn a_play_the_board_cannot_spell_is_refused_and_asked_again() {
        let server = server();
        let mut expected = AiSeat::new(PlayerId(0), &EASY, 1);
        expected.begin_turn(&server);
        let expected = expected.next_command(&server);

        let (mut seat, rejects) = seat(1);
        seat.begin_turn(&server);
        let command = seat.next_command(&server);

        assert_eq!(rejects.get(), 1, "the agent is told its play was refused");
        assert_eq!(seat.refusals, 1);
        assert_ne!(command, Some(GameCommand::EndTurn));
        assert_eq!(
            command, expected,
            "the turn goes on with the agent's own play"
        );
    }

    #[test]
    fn a_seat_whose_plays_cannot_be_spelled_ends_its_turn() {
        let server = server();
        let (mut seat, rejects) = seat(u32::MAX);
        seat.begin_turn(&server);

        assert_eq!(seat.next_command(&server), Some(GameCommand::EndTurn));
        assert_eq!(rejects.get(), REFUSAL_LIMIT);
        assert_eq!(seat.next_command(&server), None);
    }

    #[test]
    fn a_seat_stops_asking_once_its_turn_is_no_longer_open() {
        let mut server = server();
        let (mut seat, _) = seat(0);
        seat.begin_turn(&server);
        assert!(seat.next_command(&server).is_some());

        // The opponent resigns part way through the turn, which ends a duel.
        server
            .submit_command(PlayerId(1), GameCommand::Resign)
            .expect("a seat may resign off its own turn");
        assert_eq!(server.active_player(), None);

        assert_eq!(
            seat.next_command(&server),
            None,
            "no end-turn after the end"
        );
        assert_eq!(seat.next_command(&server), None);
    }

    #[test]
    fn a_seat_stops_asking_once_another_seat_holds_the_turn() {
        let mut server = server();
        let (mut seat, _) = seat(0);
        seat.begin_turn(&server);
        server
            .submit_command(PlayerId(0), GameCommand::EndTurn)
            .expect("the seat on the move may pass");
        assert_eq!(server.active_player(), Some(PlayerId(1)));

        assert_eq!(seat.next_command(&server), None);
    }

    #[test]
    fn a_refused_command_is_told_to_the_agent() {
        let server = server();
        let (mut seat, rejects) = seat(0);
        seat.begin_turn(&server);
        assert!(seat.next_command(&server).is_some());

        seat.refused(&server);

        assert_eq!(rejects.get(), 1);
        assert_eq!(seat.refusals, 1);
    }
}
