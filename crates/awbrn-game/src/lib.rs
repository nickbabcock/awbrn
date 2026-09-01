//! Deterministic, platform-neutral AWBRN game execution.

mod authority;
pub mod command;
pub mod error;
mod player;
mod replay;
mod setup;
mod setup_state;
mod unit_id;

pub use authority::{AcceptedTransition, Authority};
pub use awbrn_map::AwbrnMap;
pub use command::{GameCommand, PostMoveAction, PowerLevel, UnmappedCommand, game_command};
pub use error::CommandError;
pub use player::PlayerId;
pub use replay::{ReplayEventError, StoredActionEvent};
pub use setup::{GameSetup, PlayerSetup, SetupError};
pub use setup_state::{faction_players, semantic_player_id, semantic_terrain, state_from_setup};
pub use unit_id::ServerUnitId;
