//! Append-only event logs and deterministic offline reanalysis.

use std::collections::BTreeMap;
use std::fmt::Write as FmtWrite;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use awbrn_ai_diagnostic_types::{
    EVENT_LOG_SCHEMA_VERSION, Invalidation, MatchIdentity, MatchObservation, PairKey, Reduction,
    ReductionPlan, RunManifest, SeatOrderVariant, fingerprint_bytes,
};
use awvm::semantic::{Match, PlayerId, State};
use awvm::transition::Command;
use serde::{Deserialize, Serialize};

/// The event kind written to the raw JSONL log.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EventKind {
    Initial,
    Command,
    TurnEnd,
    Terminal,
    AttemptInvalidated,
}

/// One authoritative event-log row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EventRow {
    pub schema_version: u16,
    pub sequence: u64,
    pub match_id: String,
    #[serde(default)]
    pub attempt: u32,
    pub pair: PairKey,
    pub match_seed: u64,
    pub seat_order: SeatOrderVariant,
    pub map_fingerprint: String,
    pub configuration_fingerprint: String,
    pub event_kind: EventKind,
    pub day: u64,
    pub active_player: PlayerId,
    pub turn_index: u32,
    pub command_index: u32,
    pub command: Option<Command>,
    pub command_fingerprint: u64,
    #[serde(default)]
    pub invalidation: Option<Invalidation>,
    pub state: State,
}

/// Compatibility name for callers that use the older event terminology.
pub type MatchEventRow = EventRow;

/// Errors from raw event logging and reanalysis.
#[derive(Debug, thiserror::Error)]
pub enum EventLogError {
    #[error("event log I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("event log JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("event log schema error: {0}")]
    Schema(String),
    #[error("event log validation error: {0}")]
    Validation(String),
}

/// What the log already holds for one match.
#[derive(Clone, Debug)]
struct MatchProgress {
    /// The highest attempt number the log holds.
    attempt: u32,
    /// Whether any attempt reached a terminal row.
    terminal: bool,
    /// The last row of the highest attempt.
    last: EventRow,
}

/// An append-only JSONL writer.
#[derive(Debug)]
pub struct EventLogWriter {
    path: PathBuf,
    writer: BufWriter<File>,
    sequence: u64,
    /// Per-match progress, so resume decisions do not reread the log.
    matches: BTreeMap<String, MatchProgress>,
}

impl EventLogWriter {
    /// Open a log without truncating existing rows.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, EventLogError> {
        let path = path.as_ref().to_owned();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        // An interrupted append can leave a partial last line. Drop it before
        // the next append, so a complete row never follows half of one.
        let rows = if path.exists() {
            truncate_partial_line(&path)?;
            read_event_log(&path)?
        } else {
            Vec::new()
        };
        let sequence = rows.last().map_or(0, |row| row.sequence + 1);
        let mut matches = BTreeMap::new();
        for row in rows {
            record_progress(&mut matches, row);
        }
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Self {
            path,
            writer: BufWriter::new(file),
            sequence,
            matches,
        })
    }

    /// Return the log path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Return whether a terminal row exists for `match_id`.
    pub fn has_terminal_match(&mut self, match_id: &str) -> Result<bool, EventLogError> {
        Ok(self
            .matches
            .get(match_id)
            .is_some_and(|progress| progress.terminal))
    }

    /// Start the next attempt for a match and record an interrupted attempt.
    pub fn begin_attempt(&mut self, match_id: &str) -> Result<u32, EventLogError> {
        let Some(progress) = self.matches.get(match_id) else {
            return Ok(0);
        };
        let attempt = progress.attempt;
        let last = progress.last.clone();
        if last.event_kind != EventKind::AttemptInvalidated
            && last.event_kind != EventKind::Terminal
        {
            self.append(row_for_invalidated_attempt(&last, Invalidation::Abandoned))?;
            self.flush()?;
        }
        Ok(attempt + 1)
    }

    /// Append one row and assign its sequence number.
    pub fn append(&mut self, mut row: EventRow) -> Result<(), EventLogError> {
        if row.schema_version != EVENT_LOG_SCHEMA_VERSION {
            return Err(EventLogError::Schema(format!(
                "unsupported event log schema {}",
                row.schema_version
            )));
        }
        row.sequence = self.sequence;
        self.sequence += 1;
        serde_json::to_writer(&mut self.writer, &row)?;
        self.writer.write_all(b"\n")?;
        record_progress(&mut self.matches, row);
        Ok(())
    }

    /// Flush all rows to the operating system.
    pub fn flush(&mut self) -> Result<(), EventLogError> {
        self.writer.flush()?;
        Ok(())
    }
}

/// Cut a log back to the end of its last complete line.
///
/// The tail is scanned backwards, so a large log is not read again to find
/// the one line that a stopped process could have left unfinished.
fn truncate_partial_line(path: &Path) -> Result<(), EventLogError> {
    const CHUNK: u64 = 8 * 1024;

    let mut file = OpenOptions::new().read(true).write(true).open(path)?;
    let length = file.metadata()?.len();
    if length == 0 {
        return Ok(());
    }
    let mut end = length;
    let mut buffer = vec![0_u8; CHUNK as usize];
    while end > 0 {
        let start = end.saturating_sub(CHUNK);
        let read = usize::try_from(end - start).unwrap_or(buffer.len());
        file.seek(SeekFrom::Start(start))?;
        file.read_exact(&mut buffer[..read])?;
        if let Some(index) = buffer[..read].iter().rposition(|byte| *byte == b'\n') {
            let complete = start + index as u64 + 1;
            if complete != length {
                file.set_len(complete)?;
            }
            return Ok(());
        }
        end = start;
    }
    // The whole file is one unfinished line.
    file.set_len(0)?;
    Ok(())
}

/// Fold one row into the per-match progress the writer keeps.
fn record_progress(matches: &mut BTreeMap<String, MatchProgress>, row: EventRow) {
    let terminal = row.event_kind == EventKind::Terminal;
    match matches.get_mut(&row.match_id) {
        Some(progress) => {
            progress.terminal |= terminal;
            if row.attempt >= progress.attempt {
                progress.attempt = row.attempt;
                progress.last = row;
            }
        }
        None => {
            matches.insert(
                row.match_id.clone(),
                MatchProgress {
                    attempt: row.attempt,
                    terminal,
                    last: row,
                },
            );
        }
    }
}

/// Read and validate an event log in source order.
pub fn read_event_log(path: impl AsRef<Path>) -> Result<Vec<EventRow>, EventLogError> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut rows = Vec::new();
    // An interrupted append can leave a partial last line. Every earlier line
    // was written completely, so only the last one may be malformed.
    let lines = reader.lines().collect::<Result<Vec<_>, _>>()?;
    let line_count = lines.len();
    for (line_number, line) in lines.into_iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let row: EventRow = match serde_json::from_str(&line) {
            Ok(row) => row,
            Err(_) if line_number + 1 == line_count => break,
            Err(error) => return Err(error.into()),
        };
        if row.schema_version != EVENT_LOG_SCHEMA_VERSION {
            return Err(EventLogError::Schema(format!(
                "row {} uses unsupported schema {}",
                line_number + 1,
                row.schema_version
            )));
        }
        if row.sequence != rows.len() as u64 {
            return Err(EventLogError::Validation(format!(
                "row {} has sequence {}, expected {}",
                line_number + 1,
                row.sequence,
                rows.len()
            )));
        }
        rows.push(row);
    }
    Ok(rows)
}

/// Write stable derived tables from the raw event log.
pub fn reanalyse_event_log(
    events: impl AsRef<Path>,
    output: impl AsRef<Path>,
) -> Result<ReanalysisSummary, EventLogError> {
    let rows = read_event_log(events)?;
    let output = output.as_ref();
    fs::create_dir_all(output)?;
    write_event_tables(output, &rows)
}

/// Verify the fingerprints declared by a run manifest.
pub fn verify_expected_fingerprints(
    manifest: &RunManifest,
    event_log: impl AsRef<Path>,
    derived_root: impl AsRef<Path>,
) -> Result<(), EventLogError> {
    let event_log = event_log.as_ref();
    let derived_root = derived_root.as_ref();
    let expected = &manifest.expected;
    if expected.event_log.is_none()
        && expected.command.is_empty()
        && expected.derived_tables.is_empty()
    {
        return Ok(());
    }
    let event_bytes = fs::read(event_log)?;
    if let Some(expected_fingerprint) = &expected.event_log {
        let actual = fingerprint_bytes(&event_bytes);
        if &actual != expected_fingerprint {
            return Err(EventLogError::Validation(format!(
                "event log fingerprint differs: expected {expected_fingerprint}, got {actual}"
            )));
        }
    }
    if !expected.command.is_empty() {
        let rows = latest_attempt_rows(&read_event_log(event_log)?);
        let mut actual = BTreeMap::new();
        for row in rows
            .iter()
            .filter(|row| row.event_kind == EventKind::Terminal)
        {
            if actual
                .insert(
                    row.match_id.clone(),
                    format!("{:016x}", row.command_fingerprint),
                )
                .is_some()
            {
                return Err(EventLogError::Validation(format!(
                    "match {} has more than one terminal event",
                    row.match_id
                )));
            }
        }
        if actual != expected.command {
            return Err(EventLogError::Validation(format!(
                "command fingerprints differ: expected {:?}, got {:?}",
                expected.command, actual
            )));
        }
    }
    for specification in &expected.derived_tables {
        let (name, expected_fingerprint) = specification.split_once('=').ok_or_else(|| {
            EventLogError::Validation(format!(
                "derived table expectation {specification:?} must use name=fingerprint"
            ))
        })?;
        let mut components = Path::new(name).components();
        let safe_name = matches!(components.next(), Some(std::path::Component::Normal(_)))
            && components.next().is_none();
        if name.is_empty() || expected_fingerprint.is_empty() || !safe_name {
            return Err(EventLogError::Validation(format!(
                "derived table expectation {specification:?} has an invalid path"
            )));
        }
        let path = derived_root.join(name);
        let actual = fingerprint_bytes(&fs::read(&path)?);
        if actual != expected_fingerprint {
            return Err(EventLogError::Validation(format!(
                "derived table {name} fingerprint differs: expected {expected_fingerprint}, got {actual}"
            )));
        }
    }
    Ok(())
}

/// Rebuild all match-derived outputs from the raw event log.
pub fn reanalyse_event_log_with_manifest(
    events: impl AsRef<Path>,
    output: impl AsRef<Path>,
    manifest: &RunManifest,
) -> Result<ReanalysisSummary, EventLogError> {
    manifest.validate().map_err(EventLogError::Schema)?;
    let events = events.as_ref().to_owned();
    let rows = read_event_log(&events)?;
    let output = output.as_ref();
    fs::create_dir_all(output)?;
    let observations = observations_from_event_log(&rows, manifest);
    write_derived_outputs(output, &observations, manifest)?;
    write_event_tables(output, &rows)?;
    verify_expected_fingerprints(manifest, &events, output)?;
    Ok(ReanalysisSummary {
        schema_version: EVENT_LOG_SCHEMA_VERSION,
        event_rows: rows.len(),
        matches: observations.len(),
        command_rows: rows.iter().filter(|row| row.command.is_some()).count(),
    })
}

/// Counts produced by offline reanalysis.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReanalysisSummary {
    pub schema_version: u16,
    pub event_rows: usize,
    pub matches: usize,
    pub command_rows: usize,
}

fn csv(value: &str) -> String {
    if value
        .bytes()
        .any(|byte| matches!(byte, b',' | b'"' | b'\n' | b'\r'))
    {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

pub(crate) fn write_event_tables(
    output: &Path,
    rows: &[EventRow],
) -> Result<ReanalysisSummary, EventLogError> {
    let (commands, states, summary) = render_event_tables(rows)?;
    fs::write(output.join("commands.csv"), commands)?;
    fs::write(output.join("states.jsonl"), states)?;
    fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    Ok(summary)
}

pub(crate) fn render_event_tables(
    rows: &[EventRow],
) -> Result<(Vec<u8>, Vec<u8>, ReanalysisSummary), EventLogError> {
    let mut commands = String::from(
        "sequence,match_id,attempt,map_id,run_seed,pair_index,match_seed,seat_order,event_kind,day,turn_index,command_index,command_fingerprint,command\n",
    );
    let mut states = Vec::new();
    let mut match_count = 0;
    let mut seen_matches = std::collections::BTreeSet::new();
    for row in rows {
        let command = row
            .command
            .as_ref()
            .map(|command| serde_json::to_string(command).expect("commands serialize"))
            .unwrap_or_default();
        writeln!(
            commands,
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            row.sequence,
            csv(&row.match_id),
            row.attempt,
            row.pair.map_id,
            row.pair.run_seed,
            row.pair.pair_index,
            row.match_seed,
            row.seat_order.as_str(),
            event_name(row.event_kind),
            row.day,
            row.turn_index,
            row.command_index,
            row.command_fingerprint,
            csv(&command),
        )
        .map_err(|error| EventLogError::Validation(error.to_string()))?;
        serde_json::to_writer(&mut states, row)?;
        states.write_all(b"\n")?;
        if row.event_kind == EventKind::Terminal && seen_matches.insert(row.match_id.clone()) {
            match_count += 1;
        }
    }
    let summary = ReanalysisSummary {
        schema_version: EVENT_LOG_SCHEMA_VERSION,
        event_rows: rows.len(),
        matches: match_count,
        command_rows: rows.iter().filter(|row| row.command.is_some()).count(),
    };
    Ok((commands.into_bytes(), states, summary))
}

/// Reconstruct match observations from terminal authoritative states.
pub fn observations_from_event_log(
    rows: &[EventRow],
    manifest: &RunManifest,
) -> Vec<MatchObservation> {
    let rows = latest_attempt_rows(rows);
    let mut grouped = std::collections::BTreeMap::<String, Vec<&EventRow>>::new();
    for row in &rows {
        grouped.entry(row.match_id.clone()).or_default().push(row);
    }
    let mut observations = grouped
        .into_values()
        .map(|rows| {
            let Some(terminal) = rows
                .iter()
                .rev()
                .find(|row| row.event_kind == EventKind::Terminal)
            else {
                let row = rows[0];
                return MatchObservation::invalid(
                    MatchIdentity {
                        pair: row.pair.clone(),
                        match_seed: row.match_seed,
                        seat_order: row.seat_order,
                        configuration_fingerprint: row.configuration_fingerprint.clone(),
                        map_fingerprint: row.map_fingerprint.clone(),
                    },
                    rows.iter()
                        .rev()
                        .find_map(|row| row.invalidation.clone())
                        .unwrap_or(Invalidation::MissingOutcome),
                );
            };
            let identity = MatchIdentity {
                pair: terminal.pair.clone(),
                match_seed: terminal.match_seed,
                seat_order: terminal.seat_order,
                configuration_fingerprint: terminal.configuration_fingerprint.clone(),
                map_fingerprint: terminal.map_fingerprint.clone(),
            };
            match match_points(&terminal.state, terminal.seat_order) {
                Some(value) => MatchObservation::valid(
                    identity,
                    value,
                    Some(u32::try_from(terminal.day).unwrap_or(u32::MAX)),
                    Some(if terminal.day >= u64::from(manifest.limits.day_limit) {
                        "day-limit".into()
                    } else {
                        "terminal".into()
                    }),
                ),
                None => MatchObservation::invalid(identity, Invalidation::MissingOutcome),
            }
        })
        .collect::<Vec<_>>();
    observations.sort_by(|left, right| {
        left.identity
            .pair
            .cmp(&right.identity.pair)
            .then(left.identity.seat_order.cmp(&right.identity.seat_order))
    });
    observations
}

/// Return only the newest attempt for each logical match.
pub(crate) fn latest_attempt_rows(rows: &[EventRow]) -> Vec<EventRow> {
    let mut latest = BTreeMap::<String, u32>::new();
    for row in rows {
        latest
            .entry(row.match_id.clone())
            .and_modify(|attempt| *attempt = (*attempt).max(row.attempt))
            .or_insert(row.attempt);
    }
    rows.iter()
        .filter(|row| latest.get(&row.match_id) == Some(&row.attempt))
        .cloned()
        .collect()
}

/// Write match rows and reducer outputs in stable order.
pub fn write_derived_outputs(
    output: &Path,
    observations: &[MatchObservation],
    manifest: &RunManifest,
) -> Result<Reduction, EventLogError> {
    let rendered = render_derived_outputs(observations, manifest)?;
    fs::write(output.join("matches.jsonl"), rendered.matches)?;
    fs::write(output.join("reduction.json"), rendered.reduction_json)?;
    fs::write(output.join("reduction.csv"), rendered.reduction_csv)?;
    Ok(rendered.reduction)
}

pub(crate) fn render_derived_outputs(
    observations: &[MatchObservation],
    manifest: &RunManifest,
) -> Result<RenderedDerivedOutputs, EventLogError> {
    let mut matches = Vec::new();
    for observation in observations {
        serde_json::to_writer(&mut matches, observation)?;
        matches.write_all(b"\n")?;
    }
    let plan = ReductionPlan::from_pairs(manifest.expected_pairs());
    let reduction = plan.reduce(observations.iter().cloned());
    let mut csv = String::from("map_id,run_seed,pair_index,differential,non_day_limit\n");
    for observation in &reduction.observations {
        csv.push_str(&format!(
            "{},{},{},{},{}\n",
            observation.key.map_id,
            observation.key.run_seed,
            observation.key.pair_index,
            observation.differential,
            observation.non_day_limit,
        ));
    }
    Ok(RenderedDerivedOutputs {
        matches,
        reduction_json: serde_json::to_vec_pretty(&reduction)?,
        reduction_csv: csv.into_bytes(),
        reduction,
    })
}

pub(crate) struct RenderedDerivedOutputs {
    pub matches: Vec<u8>,
    pub reduction_json: Vec<u8>,
    pub reduction_csv: Vec<u8>,
    pub reduction: Reduction,
}

/// The points the candidate agent takes from a finished match.
///
/// Scoring lives here because the event log is the authority: a match scored
/// while it runs and the same match scored again from its rows must agree.
pub(crate) fn match_points(state: &State, seat_order: SeatOrderVariant) -> Option<f64> {
    let Match::Finished { outcome } = &state.match_state else {
        return None;
    };
    let agent_seat = match seat_order {
        SeatOrderVariant::AgentFirst => 0,
        SeatOrderVariant::BaselineFirst => 1,
    };
    let team = state
        .players
        .seats()
        .find(|(seat, _)| seat.get() == agent_seat)
        .map(|(_, player)| &player.team)?;
    match outcome {
        awvm::semantic::Outcome::Victory { winners, .. } => {
            Some(if winners.contains(team) { 1.0 } else { 0.0 })
        }
        awvm::semantic::Outcome::Draw { .. } => Some(0.5),
        awvm::semantic::Outcome::Cancelled { .. } => None,
    }
}

fn event_name(kind: EventKind) -> &'static str {
    match kind {
        EventKind::Initial => "initial",
        EventKind::Command => "command",
        EventKind::TurnEnd => "turn-end",
        EventKind::Terminal => "terminal",
        EventKind::AttemptInvalidated => "attempt-invalidated",
    }
}

/// Build an event row from an authoritative callback.
pub fn row_for_state(
    metadata: &EventMetadata,
    sequence: u64,
    state: &State,
    command: Option<Command>,
    command_fingerprint: u64,
    turn_index: u32,
    command_index: u32,
) -> EventRow {
    let event_kind = match command.as_ref() {
        None => EventKind::Initial,
        Some(Command::EndTurn { .. }) if matches!(state.match_state, Match::Finished { .. }) => {
            EventKind::Terminal
        }
        Some(Command::EndTurn { .. }) => EventKind::TurnEnd,
        Some(_) if matches!(state.match_state, Match::Finished { .. }) => EventKind::Terminal,
        Some(_) => EventKind::Command,
    };
    EventRow {
        schema_version: EVENT_LOG_SCHEMA_VERSION,
        sequence,
        match_id: metadata.match_id.clone(),
        attempt: metadata.attempt,
        pair: metadata.pair.clone(),
        match_seed: metadata.match_seed,
        seat_order: metadata.seat_order,
        map_fingerprint: metadata.map_fingerprint.clone(),
        configuration_fingerprint: metadata.configuration_fingerprint.clone(),
        event_kind,
        day: state.turn.day,
        active_player: state.turn.active_player.clone(),
        turn_index,
        command_index,
        command,
        command_fingerprint,
        invalidation: None,
        state: state.clone(),
    }
}

fn row_for_invalidated_attempt(row: &EventRow, invalidation: Invalidation) -> EventRow {
    EventRow {
        schema_version: EVENT_LOG_SCHEMA_VERSION,
        sequence: row.sequence,
        match_id: row.match_id.clone(),
        attempt: row.attempt,
        pair: row.pair.clone(),
        match_seed: row.match_seed,
        seat_order: row.seat_order,
        map_fingerprint: row.map_fingerprint.clone(),
        configuration_fingerprint: row.configuration_fingerprint.clone(),
        event_kind: EventKind::AttemptInvalidated,
        day: row.day,
        active_player: row.active_player.clone(),
        turn_index: row.turn_index,
        command_index: row.command_index,
        command: None,
        command_fingerprint: row.command_fingerprint,
        invalidation: Some(invalidation),
        state: row.state.clone(),
    }
}

/// Identity attached to every event row of one match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventMetadata {
    pub match_id: String,
    pub attempt: u32,
    pub pair: PairKey,
    pub match_seed: u64,
    pub seat_order: SeatOrderVariant,
    pub map_fingerprint: String,
    pub configuration_fingerprint: String,
}
