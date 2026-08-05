mod awvm_adapter;
pub mod command;
pub mod error;
mod player;
pub mod replay;
pub mod server;
mod setup;
mod state;
mod unit_id;
mod view;
mod wasm;

pub use awbrn_types::Co;
pub use command::{GameCommand, PostMoveAction, PowerLevel};
pub use error::CommandError;
pub use player::{PlayerId, PlayerRegistry};
pub use replay::{ReplayError, ReplayEventError, StoredActionEvent, reconstruct_from_events};
pub use server::GameServer;
pub use setup::{GameSetup, PlayerSetup, SetupError};
pub use state::ServerGameState;
pub use unit_id::ServerUnitId;
pub use view::{
    CaptureEvent, CombatOutcome, CommandResult, PlayerUpdate, PlayerView, SpectatorView,
};
pub use wasm::WasmMatch;
