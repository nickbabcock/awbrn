#[cfg(target_family = "wasm")]
mod console_writer;
mod player;
pub mod replay;
pub mod results;
pub mod server;
mod state;
#[cfg(any(target_family = "wasm", test))]
mod subscriber;
mod view;
#[cfg(target_family = "wasm")]
mod wasm;

pub use awbrn_game::{
    CommandError, GameCommand, GameSetup, PlayerId, PlayerSetup, PostMoveAction, PowerLevel,
    ReplayEventError, ServerUnitId, SetupError, StoredActionEvent, state_from_setup,
};
pub use awbrn_types::Co;
pub use player::PlayerRegistry;
pub use replay::{ReplayError, reconstruct_from_events};
pub use results::{MatchResults, SeatOutcome, SeatResult, SeatResultReason};
pub use server::GameServer;
pub use state::ServerGameState;
pub use view::{CaptureEvent, CommandResult, PlayerUpdate, PlayerView, SpectatorView};
#[cfg(target_family = "wasm")]
pub use wasm::{LogLevel, LoggingOptions, WasmMatch, init_logging};
