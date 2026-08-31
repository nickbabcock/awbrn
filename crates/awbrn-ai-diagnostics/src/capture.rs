//! Deterministic visual capture for authoritative match states.

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use awbrn_ai_diagnostic_types::{CapturePolicy, FramePolicy, SeatOrderVariant};
use awbrn_image::{Tilesets, encode_png, render_state};
use awbrn_map::AwbrnMap;
use awbrn_types::PlayerFaction;
use awvm::semantic::{Match, PlayerId, State};
use awvm::transition::Command;
use serde::Serialize;

/// The current visual capture schema.
pub const SCHEMA_VERSION: u16 = 1;

/// Identity copied into every frame sidecar row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisualCaptureIdentity {
    pub map_id: u32,
    pub run_seed: u64,
    pub pair_index: u64,
    pub attempt: u32,
    pub seat_order: SeatOrderVariant,
    pub match_seed: u64,
}

#[derive(Serialize)]
struct FrameRecord<'a> {
    schema_version: u16,
    frame: u32,
    image: String,
    map_id: u32,
    run_seed: u64,
    pair_index: u64,
    attempt: u32,
    seat_order: SeatOrderVariant,
    match_seed: u64,
    day: u64,
    active_player: &'a PlayerId,
    terminal: bool,
    turn_end: bool,
    commands: &'a [Command],
    state: &'a State,
}

#[derive(Clone, Debug)]
struct PendingFrame {
    state: State,
    turn_end: bool,
    commands: Vec<Command>,
}

/// A fallible capture sink for authoritative states.
#[derive(Debug)]
pub struct VisualCapture<'a> {
    directory: PathBuf,
    map: AwbrnMap,
    factions: [PlayerFaction; 2],
    tilesets: &'a Tilesets,
    identity: VisualCaptureIdentity,
    policy: CapturePolicy,
    frames: BufWriter<File>,
    frame: u32,
    commands: Vec<Command>,
    pending: Vec<PendingFrame>,
    finished: bool,
}

/// Load the production sprite atlases.
///
/// A caller loads these once and lends them to every capture it makes. A
/// caller that does not check how the frames look lends synthetic atlases
/// instead, so it does not need the generated `assets/textures` files.
pub fn load_tilesets() -> Result<Tilesets> {
    let assets = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the diagnostics crate is inside the workspace")
        .join("assets/textures");
    Tilesets::load_from_dir(&assets)
        .with_context(|| format!("loading capture sprites from {}", assets.display()))
}

impl<'a> VisualCapture<'a> {
    /// Create a capture sink that renders with the given sprite atlases.
    pub fn new(
        map: AwbrnMap,
        factions: [PlayerFaction; 2],
        directory: impl AsRef<Path>,
        identity: VisualCaptureIdentity,
        policy: CapturePolicy,
        tilesets: &'a Tilesets,
    ) -> Result<Self> {
        let directory = directory.as_ref().to_owned();
        fs::create_dir_all(&directory)
            .with_context(|| format!("creating capture directory {}", directory.display()))?;
        let _ = fs::remove_file(directory.join("complete"));
        let frames = File::create(directory.join("frames.jsonl"))
            .with_context(|| format!("creating {}/frames.jsonl", directory.display()))?;
        Ok(Self {
            directory,
            map,
            factions,
            tilesets,
            identity,
            policy,
            frames: BufWriter::new(frames),
            frame: 0,
            commands: Vec::new(),
            pending: Vec::new(),
            finished: false,
        })
    }

    /// Record one accepted command or the initial state.
    pub fn observe(&mut self, state: &State, command: Option<&Command>) -> Result<()> {
        if self.finished {
            anyhow::bail!("visual capture received an observation after finish")
        }
        let terminal = matches!(state.match_state, Match::Finished { .. });
        let turn_end = command.is_some_and(|command| matches!(command, Command::EndTurn { .. }));
        if let Some(command) = command {
            self.commands.push(command.clone());
        }
        if !self.is_selected() {
            if turn_end {
                self.commands.clear();
            }
            return Ok(());
        }
        if terminal && matches!(self.policy.frame_policy, FramePolicy::TerminalWindow { .. }) {
            self.flush_terminal_window(state.turn.day)?;
            self.write_frame(state, turn_end)?;
        } else if matches!(self.policy.frame_policy, FramePolicy::TerminalWindow { .. }) && turn_end
        {
            self.pending.push(PendingFrame {
                state: state.clone(),
                turn_end,
                commands: std::mem::take(&mut self.commands),
            });
        } else if self.should_capture(state, command, turn_end, terminal) {
            self.write_frame(state, turn_end)?;
        } else if turn_end {
            // Do not combine commands from two turns in a later selected
            // frame.
            self.commands.clear();
        }
        Ok(())
    }

    /// Capture the current state for an anomaly-triggered policy.
    pub fn capture_now(&mut self, state: &State) -> Result<()> {
        if !matches!(self.policy.frame_policy, FramePolicy::AnomalyTriggered) {
            anyhow::bail!("capture_now requires an anomaly-triggered policy")
        }
        if !self.is_selected() {
            return Ok(());
        }
        self.write_frame(state, false)
    }

    fn should_capture(
        &self,
        state: &State,
        command: Option<&Command>,
        turn_end: bool,
        terminal: bool,
    ) -> bool {
        if matches!(self.policy.frame_policy, FramePolicy::Disabled) {
            return false;
        }
        if command.is_none() || terminal {
            return true;
        }
        match &self.policy.frame_policy {
            FramePolicy::Disabled => false,
            FramePolicy::EveryTurn => turn_end,
            FramePolicy::SelectedDays { days } => turn_end && days.contains(&state.turn.day),
            FramePolicy::TerminalWindow { .. } => false,
            FramePolicy::AnomalyTriggered => false,
        }
    }

    fn is_selected(&self) -> bool {
        self.policy.selects(
            self.identity.map_id,
            self.identity.run_seed,
            self.identity.pair_index,
            self.identity.seat_order,
        )
    }

    fn flush_terminal_window(&mut self, terminal_day: u64) -> Result<()> {
        let FramePolicy::TerminalWindow { before, after } = &self.policy.frame_policy else {
            return Ok(());
        };
        let first_day = terminal_day.saturating_sub(u64::from(*before));
        let last_day = terminal_day.saturating_add(u64::from(*after));
        let pending = std::mem::take(&mut self.pending);
        for frame in pending {
            if (first_day..=last_day).contains(&frame.state.turn.day) {
                self.write_frame_with_commands(&frame.state, frame.turn_end, &frame.commands)?;
            }
        }
        Ok(())
    }

    fn write_frame(&mut self, state: &State, turn_end: bool) -> Result<()> {
        let commands = std::mem::take(&mut self.commands);
        self.write_frame_with_commands(state, turn_end, &commands)
    }

    fn write_frame_with_commands(
        &mut self,
        state: &State,
        turn_end: bool,
        commands: &[Command],
    ) -> Result<()> {
        let terminal = matches!(state.match_state, Match::Finished { .. });
        let image = if self.frame == 0 {
            "frame-0000-start.png".to_owned()
        } else if terminal {
            format!("frame-{:04}-final.png", self.frame)
        } else {
            format!("frame-{:04}-turn.png", self.frame)
        };
        let rendered = render_state(&self.map, state, &self.factions, self.tilesets)
            .with_context(|| format!("rendering visual capture frame {image}"))?;
        fs::write(self.directory.join(&image), encode_png(&rendered)?)
            .with_context(|| format!("writing visual capture frame {image}"))?;
        let record = FrameRecord {
            schema_version: SCHEMA_VERSION,
            frame: self.frame,
            image,
            map_id: self.identity.map_id,
            run_seed: self.identity.run_seed,
            pair_index: self.identity.pair_index,
            attempt: self.identity.attempt,
            seat_order: self.identity.seat_order,
            match_seed: self.identity.match_seed,
            day: state.turn.day,
            active_player: &state.turn.active_player,
            terminal,
            turn_end,
            commands,
            state,
        };
        serde_json::to_writer(&mut self.frames, &record).context("serializing visual frame")?;
        self.frames.write_all(b"\n")?;
        self.frames.flush().context("flushing visual frame")?;
        self.frame = self.frame.saturating_add(1);
        Ok(())
    }

    /// Flush the sidecar and complete the capture.
    pub fn finish(&mut self) -> Result<()> {
        if matches!(self.policy.frame_policy, FramePolicy::Disabled) {
            return self.mark_complete();
        }
        if !self.pending.is_empty() {
            let terminal_day = self.pending.last().map_or(0, |frame| frame.state.turn.day);
            self.flush_terminal_window(terminal_day)?;
        }
        if self.frame == 0 {
            if !self.is_selected() {
                return self.mark_complete();
            }
            anyhow::bail!("visual capture has no initial frame")
        }
        self.mark_complete()
    }

    /// Flush the sidecar and mark the capture directory complete.
    fn mark_complete(&mut self) -> Result<()> {
        self.frames.flush().context("flushing visual capture")?;
        fs::write(self.directory.join("complete"), b"visual-capture-v1\n")
            .with_context(|| format!("marking capture complete in {}", self.directory.display()))?;
        self.finished = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use awbrn_ai::board::{amber_valley_map, arena};
    use awbrn_ai_diagnostic_types::PairSelection;
    use awvm::semantic::{DrawReason, Outcome};
    use std::io::{BufRead, BufReader};
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temporary_directory() -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "awbrn-ai-diagnostics-capture-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("the capture directory is unused");
        path
    }

    fn capture(path: &Path) -> VisualCapture<'static> {
        capture_with_policy(
            path,
            CapturePolicy {
                selection: awbrn_ai_diagnostic_types::CaptureSelection::All,
                frame_policy: FramePolicy::EveryTurn,
            },
        )
    }

    /// Synthetic atlases, made once and lent to every capture here.
    ///
    /// These tests read `frames.jsonl` and compare renders with each other,
    /// so they do not need the generated `assets/textures` files and run from
    /// a fresh clone.
    fn test_tilesets() -> &'static Tilesets {
        static TILESETS: OnceLock<Tilesets> = OnceLock::new();
        TILESETS.get_or_init(awbrn_image::fixtures::tilesets)
    }

    fn capture_with_policy(path: &Path, policy: CapturePolicy) -> VisualCapture<'static> {
        VisualCapture::new(
            amber_valley_map(),
            [PlayerFaction::PinkCosmos, PlayerFaction::TealGalaxy],
            path,
            VisualCaptureIdentity {
                map_id: 61748,
                run_seed: 7,
                pair_index: 9,
                attempt: 0,
                seat_order: SeatOrderVariant::AgentFirst,
                match_seed: 7,
            },
            policy,
            test_tilesets(),
        )
        .expect("the capture directory opens")
    }

    #[test]
    fn captures_initial_turn_end_and_terminal_frames() {
        let path = temporary_directory();
        let mut capture = capture(&path);
        let state = arena(false, 1);
        capture.observe(&state, None).unwrap();
        let end_turn = Command::EndTurn {
            player: state.turn.active_player.clone(),
        };
        capture.observe(&state, Some(&end_turn)).unwrap();
        let mut terminal = state.clone();
        terminal.match_state = Match::Finished {
            outcome: Outcome::Draw {
                teams: Vec::new(),
                reason: DrawReason::DayLimit,
            },
        };
        let resign = Command::Resign {
            player: state.turn.active_player.clone(),
        };
        capture.observe(&terminal, Some(&resign)).unwrap();
        capture.finish().unwrap();

        let lines = BufReader::new(File::open(path.join("frames.jsonl")).unwrap())
            .lines()
            .collect::<std::io::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(lines.len(), 3);
        let records = lines
            .iter()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(records[0]["image"], "frame-0000-start.png");
        assert_eq!(records[1]["commands"].as_array().unwrap().len(), 1);
        assert!(records[1]["turn_end"].as_bool().unwrap());
        assert!(records[2]["terminal"].as_bool().unwrap());
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn accepted_commands_are_grouped_by_turn() {
        let path = temporary_directory();
        let mut capture = capture(&path);
        let state = arena(false, 1);
        capture.observe(&state, None).unwrap();
        let tag = Command::Tag {
            player: state.turn.active_player.clone(),
        };
        let end_turn = Command::EndTurn {
            player: state.turn.active_player.clone(),
        };
        capture.observe(&state, Some(&tag)).unwrap();
        capture.observe(&state, Some(&end_turn)).unwrap();
        capture.finish().unwrap();
        let line = BufReader::new(File::open(path.join("frames.jsonl")).unwrap())
            .lines()
            .nth(1)
            .unwrap()
            .unwrap();
        let value: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["commands"].as_array().unwrap().len(), 2);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn repeated_capture_has_identical_png_and_jsonl_bytes() {
        let first = temporary_directory();
        let second = temporary_directory();
        for path in [&first, &second] {
            let mut capture = capture(path);
            let state = arena(false, 1);
            capture.observe(&state, None).unwrap();
            let end_turn = Command::EndTurn {
                player: state.turn.active_player.clone(),
            };
            capture.observe(&state, Some(&end_turn)).unwrap();
            capture.finish().unwrap();
        }
        for name in [
            "frames.jsonl",
            "frame-0000-start.png",
            "frame-0001-turn.png",
        ] {
            assert_eq!(
                fs::read(first.join(name)).unwrap(),
                fs::read(second.join(name)).unwrap(),
                "capture file {name} differs"
            );
        }
        fs::remove_dir_all(first).unwrap();
        fs::remove_dir_all(second).unwrap();
    }

    #[test]
    fn terminal_window_uses_the_terminal_day() {
        let path = temporary_directory();
        let mut capture = capture_with_policy(
            &path,
            CapturePolicy {
                selection: awbrn_ai_diagnostic_types::CaptureSelection::All,
                frame_policy: FramePolicy::TerminalWindow {
                    before: 0,
                    after: 0,
                },
            },
        );
        let state = arena(false, 1);
        capture.observe(&state, None).unwrap();
        let end_turn = Command::EndTurn {
            player: state.turn.active_player.clone(),
        };
        capture.observe(&state, Some(&end_turn)).unwrap();
        let mut terminal = state;
        terminal.turn.day = 2;
        terminal.match_state = Match::Finished {
            outcome: Outcome::Draw {
                teams: Vec::new(),
                reason: DrawReason::DayLimit,
            },
        };
        capture.observe(&terminal, None).unwrap();
        capture.finish().unwrap();
        let lines = BufReader::new(File::open(path.join("frames.jsonl")).unwrap())
            .lines()
            .collect::<std::io::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(lines.len(), 2);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn explicit_pair_selection_skips_other_matches() {
        let path = temporary_directory();
        let mut capture = capture_with_policy(
            &path,
            CapturePolicy {
                selection: awbrn_ai_diagnostic_types::CaptureSelection::ExplicitPairs {
                    pairs: vec![PairSelection {
                        map_id: 999,
                        run_seed: 7,
                        pair_index: 9,
                        seat_order: SeatOrderVariant::AgentFirst,
                    }],
                },
                frame_policy: FramePolicy::EveryTurn,
            },
        );
        let state = arena(false, 1);
        capture.observe(&state, None).unwrap();
        capture.finish().unwrap();
        let lines = BufReader::new(File::open(path.join("frames.jsonl")).unwrap())
            .lines()
            .collect::<std::io::Result<Vec<_>>>()
            .unwrap();
        assert!(lines.is_empty());
        assert!(path.join("complete").is_file());
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn explicit_pair_selection_keeps_the_selected_frame_policy() {
        let path = temporary_directory();
        let mut capture = capture_with_policy(
            &path,
            CapturePolicy {
                selection: awbrn_ai_diagnostic_types::CaptureSelection::ExplicitPairs {
                    pairs: vec![PairSelection {
                        map_id: 61748,
                        run_seed: 7,
                        pair_index: 9,
                        seat_order: SeatOrderVariant::AgentFirst,
                    }],
                },
                frame_policy: FramePolicy::EveryTurn,
            },
        );
        let state = arena(false, 1);
        capture.observe(&state, None).unwrap();
        let end_turn = Command::EndTurn {
            player: state.turn.active_player.clone(),
        };
        capture.observe(&state, Some(&end_turn)).unwrap();
        capture.finish().unwrap();
        let lines = BufReader::new(File::open(path.join("frames.jsonl")).unwrap())
            .lines()
            .collect::<std::io::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(lines.len(), 2);
        let turn = serde_json::from_str::<serde_json::Value>(&lines[1]).unwrap();
        assert!(turn["turn_end"].as_bool().unwrap());
        fs::remove_dir_all(path).unwrap();
    }
}
