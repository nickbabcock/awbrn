//! Compact game traces of SPRT runs (`ai-diagnostics sprt --trace <folder>`).
//!
//! Each game writes one JSON Lines file in the folder. The writer keeps only
//! a small buffer and the units and properties after the last command, so
//! memory does not grow with the length of the game. The lines are, in this
//! order:
//!
//! - `{"game": ...}`: the map, the pair, the seat order and the seat of the
//!   candidate.
//! - `{"terrain": ...}`: the terrain of the board at the start, row by row.
//!   The trace does not record later changes of a tile that is not a
//!   property (a destroyed pipe seam, a used missile silo), so it is not a
//!   full record of the board.
//! - `{"turn": ...}`: a snapshot at the start of each turn: the day, the
//!   active seat, and for each seat the funds, the power state, the power
//!   meter and the commander, and all units and properties.
//! - `{"command": ...}`: each accepted command, with the units whose health
//!   or position it changed and the properties that it changed. Fuel, ammo
//!   and cargo are not recorded.
//! - `{"end": ...}`: the final snapshot, the match outcome, the candidate
//!   points and the number of days.
//!
//! A unit is `[id, seat, kind, hp, [x, y]]`, and its position is `null` in
//! a transport. The owner of a property is a seat, or `null` when the
//! property is neutral. A unit change is `[id, seat, kind, hp before, hp after,
//! position before, position after]`, with `null` for a unit that does not
//! exist before or after. A property is `[x, y, owner, capture points]`, and
//! a property change is `[x, y, owner before, owner after, points before,
//! points after]`. The trace does not change the commands of a game, but
//! it adds time to each turn. A file that already exists stops the run.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use awvm::semantic::{Location, Match, State, TileOwner};
use awvm::transition::Command;
use serde_json::{Value, json};

/// A unit: seat, kind, health and position.
type Unit = (usize, Value, u8, Option<[u8; 2]>);
/// A property: the seat of the owner (`None` for neutral) and the capture
/// points.
type Property = (Option<usize>, u8);
/// The units by ID and the properties by position.
type Board = (BTreeMap<u32, Unit>, BTreeMap<[u8; 2], Property>);

fn units_of(state: &State) -> BTreeMap<u32, Unit> {
    state
        .units
        .iter()
        .map(|unit| {
            let position = match unit.location {
                Location::Board { position } => Some([position.x, position.y]),
                _ => None,
            };
            (
                unit.id.get(),
                (unit.owner.get(), json!(unit.kind), unit.hp, position),
            )
        })
        .collect()
}

fn owner(owner: TileOwner) -> Option<usize> {
    match owner {
        TileOwner::Owned(seat) => Some(seat.get()),
        _ => None,
    }
}

fn properties_of(state: &State) -> BTreeMap<[u8; 2], Property> {
    state
        .board
        .iter()
        .filter_map(|(position, tile)| {
            let points = tile.capture_points?;
            Some(([position.x, position.y], (owner(tile.owner), points)))
        })
        .collect()
}

fn snapshot(
    state: &State,
    units: &BTreeMap<u32, Unit>,
    properties: &BTreeMap<[u8; 2], Property>,
) -> Value {
    let players = state
        .players
        .seats()
        .map(|(seat, player)| {
            let commander = player.commanders.iter().find(|co| co.active);
            json!({
                "seat": seat.get(),
                "funds": player.funds,
                "power": player.power_state,
                "charge": commander.map(|co| co.power_charge),
                "commander": commander.map(|co| co.id),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "day": state.turn.day,
        "active": state.players.seat(&state.turn.active_player).map(|seat| seat.get()),
        "players": players,
        "units": units
            .iter()
            .map(|(id, (seat, kind, hp, position))| json!([id, seat, kind, hp, position]))
            .collect::<Vec<_>>(),
        "properties": properties
            .iter()
            .map(|(at, (owner, points))| json!([at[0], at[1], owner, points]))
            .collect::<Vec<_>>(),
    })
}

/// The trace of one game, written while the game is played.
pub(crate) struct GameTrace {
    writer: BufWriter<File>,
    previous: Option<Board>,
}

impl GameTrace {
    /// Create the file of one game in `folder` and write its first line.
    pub(crate) fn create(
        folder: &Path,
        map_id: u32,
        pair_index: usize,
        seat_order: &str,
        candidate_seat: usize,
    ) -> std::io::Result<Self> {
        std::fs::create_dir_all(folder)?;
        let name = format!("map-{map_id}-pair-{pair_index}-{seat_order}.jsonl");
        let mut trace = Self {
            // A file of an earlier run is an error, not a file to replace.
            writer: BufWriter::new(File::create_new(folder.join(name))?),
            previous: None,
        };
        trace.line(&json!({"game": {
            "map": map_id,
            "pair": pair_index,
            "seat_order": seat_order,
            "candidate_seat": candidate_seat,
        }}))?;
        Ok(trace)
    }

    fn line(&mut self, value: &Value) -> std::io::Result<()> {
        serde_json::to_writer(&mut self.writer, value)?;
        self.writer.write_all(b"\n")
    }

    /// Record the initial state (`command` is `None`) or one accepted
    /// command and the state after it.
    pub(crate) fn observe(
        &mut self,
        state: &State,
        command: Option<&Command>,
    ) -> std::io::Result<()> {
        let units = units_of(state);
        let properties = properties_of(state);
        match (command, self.previous.take()) {
            (Some(command), Some((units_before, properties_before))) => {
                let ids = units_before.keys().chain(units.keys());
                let unit_changes = ids
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .filter_map(|id| match (units_before.get(id), units.get(id)) {
                        (Some(a), Some(b)) if a.2 == b.2 && a.3 == b.3 => None,
                        (a, b) => {
                            let (seat, kind) = a.or(b).map(|unit| (unit.0, &unit.1))?;
                            Some(json!([
                                id,
                                seat,
                                kind,
                                a.map(|unit| unit.2),
                                b.map(|unit| unit.2),
                                a.and_then(|unit| unit.3),
                                b.and_then(|unit| unit.3),
                            ]))
                        }
                    })
                    .collect::<Vec<_>>();
                let property_changes = properties
                    .iter()
                    .filter_map(|(at, now)| {
                        let before = properties_before.get(at)?;
                        (before != now)
                            .then(|| json!([at[0], at[1], before.0, now.0, before.1, now.1]))
                    })
                    .collect::<Vec<_>>();
                self.line(&json!({"command": {
                    "command": command,
                    "units": unit_changes,
                    "properties": property_changes,
                }}))?;
                if matches!(command, Command::EndTurn { .. })
                    && matches!(state.match_state, Match::Active { .. })
                {
                    self.line(&json!({"turn": snapshot(state, &units, &properties)}))?;
                }
            }
            _ => {
                let mut rows = BTreeMap::<u8, Vec<Value>>::new();
                for (position, tile) in state.board.iter() {
                    rows.entry(position.y)
                        .or_default()
                        .push(json!(tile.terrain));
                }
                self.line(&json!({"terrain": rows.into_values().collect::<Vec<_>>()}))?;
                self.line(&json!({"turn": snapshot(state, &units, &properties)}))?;
            }
        }
        self.previous = Some((units, properties));
        Ok(())
    }

    /// Write the final line and flush the file.
    pub(crate) fn finish(
        mut self,
        state: &State,
        points: f64,
        days: u32,
        reason: Option<&str>,
    ) -> std::io::Result<()> {
        let mut end = snapshot(state, &units_of(state), &properties_of(state));
        end["match"] = json!(state.match_state);
        end["points"] = json!(points);
        end["days"] = json!(days);
        end["reason"] = json!(reason);
        self.line(&json!({ "end": end }))?;
        self.writer.flush()
    }
}
