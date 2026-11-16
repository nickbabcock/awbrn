//! What one scrubber seek costs: rebuilding a replay timeline at a boundary.
//!
//! A dragged scrubber asks for a new boundary every frame it can, so a seek
//! has to fit in a frame. The cost is the actions replayed from the nearest
//! checkpoint, so each archive is measured twice: from a timeline holding no
//! checkpoints, and from one holding them, at the boundary furthest from one.

use awbrn_client::replay_archive::{
    AwbrnReplayFile, CHECKPOINT_INTERVAL, ReplayArchive, ReplayTimeline,
};
use awbrn_game::{Authority, GameSetup};
use awbrn_map::AwbwMapData;
use awbrn_server::{GameServer, play_ai_turn};
use awvm_awbw::RecordedAdapter;

/// The longest AWBW fixture, by actions.
const AWBW_REPLAY: &str = "1468032_landfall_2025-12-22.zip";
const AWBW_MAP: &str = "108806.json";
/// The AWBRN fixture's setup. Its log is one resignation, so the bench plays
/// a log of its own on that setup.
const AWBRN_REPLAY: &str = "awbrn-replay-3zceh5a97b31o.json";
/// About the length of a played game.
const AWBRN_DAYS: u64 = 30;

fn asset(directory: &str, file_name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets")
        .join(directory)
        .join(file_name);
    std::fs::read(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

#[derive(Debug)]
pub struct SeekCase {
    pub archive: ReplayArchive,
    opening: Opening,
}

/// What a timeline standing at the opening is made from.
#[derive(Debug)]
enum Opening {
    Awbw(Box<RecordedAdapter>),
    Awbrn(Box<GameSetup>),
}

impl SeekCase {
    pub fn awbw() -> Self {
        let archive = ReplayArchive::parse(&asset("replays", AWBW_REPLAY)).unwrap();
        let map: AwbwMapData = serde_json::from_slice(&asset("maps", AWBW_MAP)).unwrap();
        let adapter = RecordedAdapter::new(archive.awbw().unwrap(), &map).unwrap();
        Self {
            archive,
            opening: Opening::Awbw(Box::new(adapter)),
        }
    }

    /// The AWBRN fixture's setup, played out by the server's own AI seats.
    pub fn awbrn() -> Self {
        let mut file = AwbrnReplayFile::parse(&asset("replays", AWBRN_REPLAY)).unwrap();
        let setup = file.game_setup().unwrap();
        let mut server = GameServer::new(setup.clone()).unwrap();
        let profile = awbrn_server::profile_for_tier(awbrn_server::AiTier::Easy);
        let mut events = Vec::new();
        'days: for day in 0..AWBRN_DAYS {
            for slot in 0..setup.players.len() {
                let Some(player) = server.active_player() else {
                    break 'days;
                };
                let seed = profile.turn_seed(setup.rng_seed, slot, day);
                events.extend(play_ai_turn(&mut server, player, profile, seed));
            }
        }
        file.actions = events;
        Self {
            archive: ReplayArchive::Awbrn(Box::new(file)),
            opening: Opening::Awbrn(Box::new(setup)),
        }
    }

    /// A timeline standing at the opening, holding no checkpoints.
    pub fn cold(&self) -> ReplayTimeline {
        match &self.opening {
            Opening::Awbw(adapter) => ReplayTimeline::from((**adapter).clone()),
            Opening::Awbrn(setup) => ReplayTimeline::Awbrn {
                setup: (**setup).clone(),
                current: Box::new(Authority::new(setup).unwrap()),
                checkpoints: Default::default(),
            },
        }
    }

    /// A timeline as a loaded archive holds it: outlined, so it holds every
    /// checkpoint it keeps.
    pub fn warm(&self) -> ReplayTimeline {
        let mut timeline = self.cold();
        timeline.outline(&self.archive).unwrap();
        timeline
    }

    /// The boundary furthest past a checkpoint.
    pub fn furthest_from_checkpoint(&self) -> usize {
        let len = self.archive.len();
        if len < CHECKPOINT_INTERVAL {
            return len;
        }
        (len + 1) / CHECKPOINT_INTERVAL * CHECKPOINT_INTERVAL - 1
    }
}

pub fn seek(case: &SeekCase, timeline: &mut ReplayTimeline, target: usize) -> usize {
    timeline.rebuild(&case.archive, target).unwrap().len()
}

pub mod criterion_benches {
    use super::*;
    use criterion::{BatchSize, Criterion};
    use std::hint::black_box;

    fn replay_seek(c: &mut Criterion) {
        let mut group = c.benchmark_group("replay_seek");
        group.sample_size(10);
        for (name, case) in [("awbw", SeekCase::awbw()), ("awbrn", SeekCase::awbrn())] {
            let end = case.archive.len();
            group.bench_function(format!("{name}/cold_to_end_{end}"), |b| {
                b.iter_batched(
                    || case.cold(),
                    |mut timeline| black_box(seek(&case, &mut timeline, end)),
                    BatchSize::LargeInput,
                );
            });
            let target = case.furthest_from_checkpoint();
            let mut warm = case.warm();
            group.bench_function(format!("{name}/warm_to_{target}"), |b| {
                b.iter(|| black_box(seek(&case, &mut warm, target)));
            });
        }
        group.finish();
    }

    criterion::criterion_group!(seek_benches, replay_seek);
}
