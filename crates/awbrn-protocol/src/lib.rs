//! Types shared by both ends of the browser/server wire contract.

use awbrn_map::Pos;
use serde::{Deserialize, Serialize};

/// An action to perform after a unit moves.
///
/// Unit identifiers use the server's wire width even when an AWBW-backed
/// client currently sources them from a 32-bit identifier space.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PostMoveAction {
    /// Attack a target at the given position.
    Attack {
        #[cfg_attr(target_family = "wasm", tsify(type = "{ x: number; y: number }"))]
        #[serde(with = "awbrn_map::xy")]
        target: Pos,
    },
    /// Begin or continue capturing the building at the destination.
    Capture,
    /// Load into a transport at the destination.
    Load { transport_id: u64 },
    /// Unload a carried unit after moving.
    ///
    /// New clients use the standalone unload command. This variant remains so
    /// the server can replay stored matches from before that command existed.
    Unload {
        cargo_id: u64,
        #[cfg_attr(target_family = "wasm", tsify(type = "{ x: number; y: number }"))]
        #[serde(with = "awbrn_map::xy")]
        position: Pos,
    },
    /// Supply adjacent friendly units.
    Supply,
    /// Repair and resupply one adjacent friendly unit.
    Repair { target_id: u64 },
    /// Dive or activate stealth.
    Hide,
    /// Surface or deactivate stealth.
    Unhide,
    /// Join with a friendly unit of the same type at the destination.
    Join { target_id: u64 },
    /// Launch the missile silo at the destination at a target tile.
    Launch {
        #[cfg_attr(target_family = "wasm", tsify(type = "{ x: number; y: number }"))]
        #[serde(with = "awbrn_map::xy")]
        target: Pos,
    },
    /// Self-destruct after moving.
    Explode,
    /// End the unit's turn without a further action.
    Wait,
}
