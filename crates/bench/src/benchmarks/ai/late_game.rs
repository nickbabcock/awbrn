//! Fixed AWBW positions and complete-turn benchmarks.

use std::path::{Path, PathBuf};

use awbrn_ai::agent::{Agent, NodeBudget};
use awbrn_ai::baseline::{BaselineConfig, production_agent};
use awbrn_ai::harness::{TurnResult, run_agent_turn};
use awbrn_ai::rng::Rng;
use awbrn_map::AwbwMapData;
use awbw_replay::ReplayParser;
use awbw_replay::turn_models::Action;
use awvm::ruleset::profile;
use awvm::semantic::{AwbwVisibility, Location, Observation, PlayerId, State, observe};
use awvm_awbw::RecordedAdapter;
use serde::Deserialize;
use sha2::{Digest, Sha256};

pub const GAME_ID: u32 = 1_507_289;
pub const MAP_ID: u32 = 143_619;
pub const MAP_WIDTH: u8 = 25;
pub const MAP_HEIGHT: u8 = 19;
pub const PLAYER_ONE: &str = "3691303";
pub const PLAYER_TWO: &str = "3588610";

const FIXTURE_DIRECTORY: &str = "fixtures/ai/late-game-standard-1507289";

const FIXED_SEED: u64 = 0x1507_2890;

#[derive(Clone, Debug)]
pub struct LateGameFixture {
    pub manifest: FixtureManifest,
    pub day_15_player_one: FixturePosition,
    pub day_15_player_two: FixturePosition,
}

#[derive(Clone, Debug)]
pub struct FixturePosition {
    pub identity: String,
    pub state: State,
    pub observation: Observation,
    pub active_player: String,
    pub active_units: usize,
    pub enemy_units: usize,
    pub total_units: usize,
    pub active_funds: u64,
    pub state_fingerprint: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct FixtureManifest {
    pub schema_version: u16,
    pub game_id: u32,
    pub map_id: u32,
    pub replay_sha256: String,
    pub map_sha256: String,
    pub replay_scope: String,
    pub map_dimensions: [u8; 2],
    pub visibility: String,
    pub fog_disabled: bool,
    pub day_boundary_identities: DayBoundaryIdentities,
    pub day_15_player_one: PositionManifest,
    pub day_15_player_two: PositionManifest,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DayBoundaryIdentities {
    pub player_one: String,
    pub player_two: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PositionManifest {
    pub active_player: String,
    pub active_units: usize,
    pub enemy_units: usize,
    pub total_units: usize,
    pub active_funds: u64,
    pub state_fingerprint: String,
}

#[derive(Debug, thiserror::Error)]
pub enum LateGameError {
    #[error("late-game fixture I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("late-game fixture JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("late-game replay error: {0}")]
    Replay(String),
    #[error("late-game recorded transition error: {0}")]
    Recorded(#[from] awvm_awbw::RecordedAdapterError),
    #[error("late-game fixture is invalid: {0}")]
    Invalid(String),
}

impl LateGameFixture {
    pub fn load() -> Result<Self, LateGameError> {
        Self::load_from(Self::directory())
    }

    pub fn load_from(directory: impl AsRef<Path>) -> Result<Self, LateGameError> {
        let directory = directory.as_ref();
        let manifest: FixtureManifest =
            serde_json::from_slice(&std::fs::read(directory.join("manifest.json"))?)?;
        let replay_bytes = std::fs::read(directory.join("replay.zip"))?;
        let map_bytes = std::fs::read(directory.join("map-143619.json"))?;
        validate_manifest(&manifest, &replay_bytes, &map_bytes)?;

        let replay = ReplayParser::new()
            .parse(&replay_bytes)
            .map_err(|error| LateGameError::Replay(error.to_string()))?;
        let map: AwbwMapData = serde_json::from_slice(&map_bytes)?;
        let game = replay
            .games
            .first()
            .ok_or_else(|| LateGameError::Invalid("replay has no game entry".into()))?;
        if game.id.as_u32() != GAME_ID || game.maps_id.as_u32() != MAP_ID {
            return Err(LateGameError::Invalid(
                "replay game or map ID does not match the fixture".into(),
            ));
        }

        let mut adapter = RecordedAdapter::new(&replay, &map)?;
        let player_one = PlayerId::from(PLAYER_ONE);
        let player_two = PlayerId::from(PLAYER_TWO);
        let mut positions = [None, None];
        for (index, action) in replay.turns.iter().enumerate() {
            if is_join_setup(action, replay.turns.get(index + 1)) {
                continue;
            }
            let transition = adapter.advance(action)?;
            let state = transition.post_state();
            let slot = if action.kind_name() == "End"
                && state.turn.day == 15
                && state.turn.active_player == player_one
            {
                Some(0)
            } else if action.kind_name() == "End"
                && state.turn.day == 15
                && state.turn.active_player == player_two
            {
                Some(1)
            } else {
                None
            };
            if let Some(slot) = slot
                && positions[slot].is_none()
            {
                positions[slot] = Some(make_position(
                    if slot == 0 {
                        "day-15-player-1"
                    } else {
                        "day-15-player-2"
                    },
                    state,
                )?);
            }
        }

        let day_15_player_one = positions[0]
            .take()
            .ok_or_else(|| LateGameError::Invalid("player-one boundary is missing".into()))?;
        let day_15_player_two = positions[1]
            .take()
            .ok_or_else(|| LateGameError::Invalid("player-two boundary is missing".into()))?;
        validate_position(&manifest.day_15_player_one, &day_15_player_one)?;
        validate_position(&manifest.day_15_player_two, &day_15_player_two)?;
        Ok(Self {
            manifest,
            day_15_player_one,
            day_15_player_two,
        })
    }

    pub fn directory() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURE_DIRECTORY)
    }
}

#[derive(Clone, Copy, Debug)]
enum FixtureKind {
    Day15PlayerOne,
    Day15PlayerTwo,
}

impl FixtureKind {
    const fn name(self) -> &'static str {
        match self {
            Self::Day15PlayerOne => "day-15-player-one",
            Self::Day15PlayerTwo => "day-15-player-two",
        }
    }

    const fn position(self, fixture: &LateGameFixture) -> &FixturePosition {
        match self {
            Self::Day15PlayerOne => &fixture.day_15_player_one,
            Self::Day15PlayerTwo => &fixture.day_15_player_two,
        }
    }
}

struct LateGameCase {
    state: State,
    agent: Box<dyn Agent>,
    entropy: Rng,
    node_budget: NodeBudget,
}

fn setup(fixture_kind: FixtureKind) -> LateGameCase {
    let fixture = LateGameFixture::load().expect("late-game fixture loads");
    LateGameCase {
        state: fixture_kind.position(&fixture).state.clone(),
        agent: Box::new(production_agent(FIXED_SEED)),
        entropy: Rng::from_seed(FIXED_SEED),
        node_budget: BaselineConfig::PRODUCTION.node_budget,
    }
}

fn run(case: &mut LateGameCase) -> TurnResult {
    let result = run_agent_turn(
        case.state.clone(),
        case.agent.as_mut(),
        &mut case.entropy,
        case.node_budget,
    );
    assert!(result.completed, "late-game benchmark turn completes");
    assert_eq!(result.rejected_commands, 0, "all commands are accepted");
    assert_eq!(result.unrealizable_plays, 0, "all plays are realizable");
    result
}

pub mod criterion_benches {
    use super::*;
    use criterion::{BatchSize, Criterion};
    use std::hint::black_box;

    fn late_game(c: &mut Criterion) {
        let mut group = c.benchmark_group("ai-late-game-turn");
        for fixture in [FixtureKind::Day15PlayerOne, FixtureKind::Day15PlayerTwo] {
            group.bench_function(fixture.name(), |b| {
                b.iter_batched(
                    || setup(fixture),
                    |mut case| black_box(run(&mut case)),
                    BatchSize::LargeInput,
                );
            });
        }
        group.finish();
    }

    criterion::criterion_group!(late_game_benches, late_game);
}

#[cfg(not(target_family = "wasm"))]
pub mod gungraun_benches {
    use super::*;
    use gungraun::{library_benchmark, library_benchmark_group};

    #[library_benchmark(setup = setup)]
    #[bench::day_15_player_one(FixtureKind::Day15PlayerOne)]
    #[bench::day_15_player_two(FixtureKind::Day15PlayerTwo)]
    fn late_game_turn(mut case: LateGameCase) -> TurnResult {
        run(&mut case)
    }

    library_benchmark_group!(name = late_game_benches, benchmarks = [late_game_turn,]);
}

fn is_join_setup(action: &Action, next: Option<&Action>) -> bool {
    // AWBW records the joining unit's Move before its Join record. The move
    // cannot exist as an intermediate AWVM state because the target occupies
    // its destination, so the Join record is the authoritative transition.
    let Action::Move(movement) = action else {
        return false;
    };
    let Some(Action::Join { join_action, .. }) = next else {
        return false;
    };
    let moving_id = movement
        .unit
        .values()
        .find_map(|unit| unit.get_value())
        .map(|unit| unit.units_id.as_u32());
    let joining_id = join_action
        .join_id
        .values()
        .find_map(|unit| unit.get_value())
        .copied();
    moving_id == joining_id
}

pub fn stable_state_fingerprint(state: &State) -> Result<String, LateGameError> {
    let bytes = serde_json::to_vec(state)?;
    Ok(format!("{:016x}", stable_hash(&bytes)))
}

pub fn stable_hash(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x1000_0000_01b3;
    bytes.iter().fold(OFFSET, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(PRIME)
    })
}

fn make_position(identity: &str, state: &State) -> Result<FixturePosition, LateGameError> {
    state
        .validate()
        .map_err(|error| LateGameError::Invalid(format!("{identity}: {error}")))?;
    if state.board.width() != MAP_WIDTH || state.board.height() != MAP_HEIGHT {
        return Err(LateGameError::Invalid(format!(
            "{identity}: board is {}x{}",
            state.board.width(),
            state.board.height()
        )));
    }
    if state.settings.fog {
        return Err(LateGameError::Invalid(format!(
            "{identity}: fog is enabled"
        )));
    }
    let active_player = state.turn.active_player.to_string();
    let active_seat = state
        .players
        .seat(&state.turn.active_player)
        .ok_or_else(|| LateGameError::Invalid(format!("{identity}: active player has no seat")))?;
    let active_units = state
        .units
        .iter()
        .filter(|unit| unit.owner == active_seat)
        .count();
    let total_units = state.units.len();
    let enemy_units = total_units.saturating_sub(active_units);
    if let Some(unit) = state.units.iter().find(|unit| {
        !matches!(unit.location, Location::Board { .. }) || profile(unit.kind).transport.is_some()
    }) {
        return Err(LateGameError::Invalid(format!(
            "{identity}: cargo or transport unit is present: id={} owner={} kind={:?} location={:?}",
            unit.id,
            unit.owner.get(),
            unit.kind,
            unit.location
        )));
    }
    let active_funds = state
        .players
        .iter()
        .find(|player| player.id() == &state.turn.active_player)
        .ok_or_else(|| LateGameError::Invalid(format!("{identity}: active player is missing")))?
        .funds;
    let observation = observe(&AwbwVisibility, state, &state.turn.active_player)
        .map_err(|error| LateGameError::Invalid(format!("{identity}: observation: {error}")))?;
    Ok(FixturePosition {
        identity: identity.into(),
        state: state.clone(),
        observation,
        active_player,
        active_units,
        enemy_units,
        total_units,
        active_funds,
        state_fingerprint: stable_state_fingerprint(state)?,
    })
}

fn validate_manifest(
    manifest: &FixtureManifest,
    replay_bytes: &[u8],
    map_bytes: &[u8],
) -> Result<(), LateGameError> {
    if manifest.schema_version != 1
        || manifest.game_id != GAME_ID
        || manifest.map_id != MAP_ID
        || manifest.map_dimensions != [MAP_WIDTH, MAP_HEIGHT]
        || manifest.visibility != "standard"
        || !manifest.fog_disabled
    {
        return Err(LateGameError::Invalid(
            "manifest identity or visibility does not match the fixture".into(),
        ));
    }
    if manifest.replay_sha256 != sha256(replay_bytes) || manifest.map_sha256 != sha256(map_bytes) {
        return Err(LateGameError::Invalid(
            "fixture hash does not match the manifest".into(),
        ));
    }
    if manifest.day_boundary_identities.player_one != "day-15-player-1"
        || manifest.day_boundary_identities.player_two != "day-15-player-2"
    {
        return Err(LateGameError::Invalid(
            "day boundary identities changed".into(),
        ));
    }
    Ok(())
}

fn validate_position(
    expected: &PositionManifest,
    actual: &FixturePosition,
) -> Result<(), LateGameError> {
    if expected.active_player != actual.active_player
        || expected.active_units != actual.active_units
        || expected.enemy_units != actual.enemy_units
        || expected.total_units != actual.total_units
        || expected.active_funds != actual.active_funds
        || expected.state_fingerprint != actual.state_fingerprint
    {
        return Err(LateGameError::Invalid(format!(
            "{} does not match its manifest record: actual player={} active={} enemy={} total={} funds={} fingerprint={}",
            actual.identity,
            actual.active_player,
            actual.active_units,
            actual.enemy_units,
            actual.total_units,
            actual.active_funds,
            actual.state_fingerprint
        )));
    }
    Ok(())
}

fn sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::LateGameFixture;

    #[test]
    fn fixture_positions_load() {
        let fixture = LateGameFixture::load().expect("late-game fixture loads");
        assert_eq!(
            fixture.day_15_player_one.state_fingerprint,
            "58409208302f6630"
        );
        assert_eq!(
            fixture.day_15_player_two.state_fingerprint,
            "4d4bb5464ea27600"
        );
    }
}
