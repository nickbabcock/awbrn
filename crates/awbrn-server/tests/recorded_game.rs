//! One Hard game, which the native and the wasm build both have to play.
//!
//! The AI is measured on native builds, and a match runs it as
//! wasm32-unknown-unknown, whose `ln`, `exp`, `asinh` and `powf` come from
//! another libm. A score that differs in its last bit can change a choice. So
//! this plays one short Hard game through `play_ai_turn`, the turn
//! `WasmMatch::run_ai_turn` plays, and pins its digest; and
//! `web/src/matches/ai_parity.test.ts` holds the wasm build the web suite runs
//! to the same digest. That is the build under test, not the deployed one.

use awbrn_ai::{AiTier, profile_for_tier};
use awbrn_map::{AwbrnMap, AwbwMapData, ValidatedMapDocument};
use awbrn_server::{Co, GameServer, GameSetup, PlayerSetup, StoredActionEvent, play_ai_turn};
use awbrn_types::{CoExt, PlayerFaction};
use awbw_types::AwbwCoId;

/// Amber Valley, the board the arena plays.
const MAP: &str = include_str!("../../../assets/maps/61748.json");

/// The 64-bit FNV-1a hash of the played events as compact JSON with sorted
/// keys. `web/src/matches/ai_parity.test.ts` pins it too: change both together.
const DIGEST: &str = "98488491ef8f823d";

/// Turns played, both seats counted.
const TURNS: usize = 16;

const RNG_SEED: u64 = 1;

/// The setup `WasmMatch::new` builds from the web suite's match setup.
fn setup() -> GameSetup {
    let data: AwbwMapData = serde_json::from_str(MAP).expect("the map parses");
    let document = ValidatedMapDocument::try_from(&data).expect("the map is valid");
    let players: Vec<_> = [1, 2]
        .into_iter()
        .map(|faction_id| PlayerSetup {
            faction: PlayerFaction::from_awbw_id(faction_id).expect("an AWBW faction"),
            team: None,
            starting_funds: 0,
            co: Co::from_awbw_id(AwbwCoId::new(1)).expect("an AWBW commander"),
        })
        .collect();
    let armies: Vec<_> = players.iter().map(|player| player.faction).collect();
    GameSetup {
        map: AwbrnMap::from_map(&document.map().with_seat_armies(&armies)),
        players,
        fog_enabled: false,
        rng_seed: RNG_SEED,
        unit_bans: Vec::new(),
        lab_units: Vec::new(),
    }
}

/// Play the turns as `WasmMatch::run_ai_turn` plays them: the shared turn,
/// seeded from the match seed, the slot and the day.
fn play() -> Vec<StoredActionEvent> {
    let hard = profile_for_tier(AiTier::Hard);
    let mut server = GameServer::new(setup()).expect("the setup is valid");
    let mut events = Vec::new();
    for _ in 0..TURNS {
        let Some(player) = server.active_player() else {
            break;
        };
        let seed = hard.turn_seed(RNG_SEED, usize::from(player.0), server.state().turn.day);
        events.extend(play_ai_turn(&mut server, player, hard, seed));
    }
    events
}

#[test]
fn the_hard_game_is_the_pinned_one() {
    let mut played = serde_json::to_value(play()).expect("events serialize");
    played.sort_all_objects();
    let json = serde_json::to_string(&played).expect("events serialize");
    let hash = json.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    let digest = format!("{hash:016x}");
    if digest != DIGEST {
        let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("hard-game.json");
        let text = serde_json::to_string_pretty(&played).expect("events serialize");
        std::fs::write(&path, text + "\n").expect("the played game is written");
        panic!(
            "the Hard game, written to {}, has digest {digest}, not {DIGEST}",
            path.display()
        );
    }
}
