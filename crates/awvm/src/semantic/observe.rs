//! The recipient projection of `spec/model/observation.md`.
//!
//! Two of the crate's three entry points are here — [`observe`] and
//! [`observe_events`], with [`observe_transition`] the one that computes both
//! halves at once — along with every `Observed*` value they return. Nothing in
//! this module is authoritative: it only decides what one recipient is
//! entitled to know about values that live in [`super`].

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::commander::AreaStrikePolicy;
use crate::event::{Event, ObservedReason, PublicEventKind};

use super::{
    BoardShapeError, Commander, CommanderId, Concealment, Location, Match, Outcome, PlayerId,
    PlayerStatus, Pos, PowerState, RareTileState, RulesetRef, Settings, Silo, State, Team, TeamId,
    TeleporterId, TerrainId, TileOwner, TraitId, Turn, Unit, UnitAction, UnitId, UnitKindId,
    Viewpoint, Visibility, Weather, owner_is_absent,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observation {
    pub ruleset: RulesetRef,
    pub recipient: PlayerId,
    pub settings: Settings,
    pub board: ObservedBoard,
    pub teams: Vec<Team>,
    pub players: Vec<ObservedPlayer>,
    pub turn: Turn,
    pub weather: Weather,
    pub units: Vec<ObservedUnit>,
    #[serde(rename = "match")]
    pub match_state: ObservedMatch,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ObservedUnitRef {
    Friendly { unit: UnitId },
    Enemy { position: Pos },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedUnit {
    #[serde(rename = "ref")]
    pub reference: ObservedUnitRef,
    pub kind: UnitKindId,
    pub owner: PlayerId,
    pub hp: u8,
    pub fuel: u64,
    pub ammo: u64,
    pub action: UnitAction,
    pub concealment: Concealment,
    pub location: Location,
}
/// The board as one recipient sees it — flat and row-major, like [`super::Board`].
///
/// The wire form is nested rows, and it stayed a `Vec<Vec<_>>` in memory long
/// after [`super::Board`] stopped being one. That left the projection as the last
/// place a coordinate was turned into a row and a column, and the last shape in
/// which rows could disagree with `width`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservedBoard {
    width: u8,
    height: u8,
    tiles: Vec<ObservedTile>,
}

impl ObservedBoard {
    /// Build a projected board from row-major tiles, on the same rectangle
    /// contract as [`super::Board::new`].
    pub fn new(width: u8, height: u8, tiles: Vec<ObservedTile>) -> Result<Self, BoardShapeError> {
        let expected = usize::from(width) * usize::from(height);
        if width == 0 || height == 0 || tiles.len() != expected {
            return Err(BoardShapeError {
                width,
                height,
                found: tiles.len(),
            });
        }
        Ok(Self {
            width,
            height,
            tiles,
        })
    }

    pub const fn width(&self) -> u8 {
        self.width
    }

    pub const fn height(&self) -> u8 {
        self.height
    }

    pub const fn contains(&self, position: Pos) -> bool {
        position.x < self.width && position.y < self.height
    }

    fn index(&self, position: Pos) -> Option<usize> {
        self.contains(position)
            .then(|| usize::from(position.y) * usize::from(self.width) + usize::from(position.x))
    }

    /// The projected tile at a coordinate, or `None` when it is off the board.
    pub fn get(&self, position: Pos) -> Option<&ObservedTile> {
        self.index(position).map(|index| &self.tiles[index])
    }

    /// The projected tile at a coordinate that has already been bounds-checked.
    pub fn tile(&self, position: Pos) -> &ObservedTile {
        self.get(position)
            .unwrap_or_else(|| panic!("{position} is off a {}x{} board", self.width, self.height))
    }

    /// Every coordinate on the board, row by row.
    pub fn positions(&self) -> impl Iterator<Item = Pos> + use<> {
        let (width, height) = (self.width, self.height);
        (0..height).flat_map(move |y| (0..width).map(move |x| Pos { x, y }))
    }

    /// Every projected tile with its coordinate, row by row.
    pub fn iter(&self) -> impl Iterator<Item = (Pos, &ObservedTile)> {
        self.positions().zip(self.tiles.iter())
    }

    pub fn tiles(&self) -> impl Iterator<Item = &ObservedTile> {
        self.tiles.iter()
    }
}

/// The wire shape: nested rows, one per `y`.
#[derive(Serialize, Deserialize)]
struct ObservedBoardRows {
    width: u8,
    height: u8,
    tiles: Vec<Vec<ObservedTile>>,
}

impl Serialize for ObservedBoard {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ObservedBoardRows {
            width: self.width,
            height: self.height,
            tiles: self
                .tiles
                .chunks(usize::from(self.width))
                .map(<[ObservedTile]>::to_vec)
                .collect(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ObservedBoard {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let rows = ObservedBoardRows::deserialize(deserializer)?;
        if rows.tiles.len() != usize::from(rows.height)
            || rows
                .tiles
                .iter()
                .any(|row| row.len() != usize::from(rows.width))
        {
            return Err(serde::de::Error::custom(BoardShapeError {
                width: rows.width,
                height: rows.height,
                found: rows.tiles.iter().map(Vec::len).sum(),
            }));
        }
        Self::new(
            rows.width,
            rows.height,
            rows.tiles.into_iter().flatten().collect(),
        )
        .map_err(serde::de::Error::custom)
    }
}

/// A tile as one recipient sees it.
///
/// Boxes its rare state for the same reason [`super::Tile`] does: an observation holds
/// one of these per square, and one is built per recipient per command. Its
/// `rare` is only ever built by the projection, which leaves it `None` when
/// there is nothing to hold — see `Tile::shrink` for why that matters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservedTile {
    pub terrain: TerrainId,
    pub visibility: TileVisibility,
    pub owner: TileOwner,
    pub capture_points: Option<u8>,
    pub silo: Option<Silo>,
    rare: Option<Box<RareTileState>>,
}

impl ObservedTile {
    pub fn destructible_hp(&self) -> Option<u64> {
        self.rare.as_ref().and_then(|rare| rare.destructible_hp)
    }

    pub fn teleporter(&self) -> Option<&TeleporterId> {
        self.rare.as_ref().and_then(|rare| rare.teleporter.as_ref())
    }

    pub fn trait_state(&self) -> Option<&BTreeMap<TraitId, serde_json::Value>> {
        self.rare
            .as_ref()
            .and_then(|rare| rare.trait_state.as_ref())
    }
}

#[derive(Serialize)]
struct ObservedTileFields<'a> {
    terrain: TerrainId,
    visibility: TileVisibility,
    #[serde(skip_serializing_if = "owner_is_absent")]
    owner: &'a TileOwner,
    #[serde(skip_serializing_if = "Option::is_none")]
    capture_points: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    silo: Option<Silo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    destructible_hp: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    teleporter: Option<&'a TeleporterId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trait_state: Option<&'a BTreeMap<TraitId, serde_json::Value>>,
}

/// The same object, owned, for reading.
#[derive(Deserialize)]
struct ObservedTileWire {
    terrain: TerrainId,
    visibility: TileVisibility,
    #[serde(default)]
    owner: TileOwner,
    capture_points: Option<u8>,
    silo: Option<Silo>,
    destructible_hp: Option<u64>,
    teleporter: Option<TeleporterId>,
    trait_state: Option<BTreeMap<TraitId, serde_json::Value>>,
}

impl Serialize for ObservedTile {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ObservedTileFields {
            terrain: self.terrain,
            visibility: self.visibility,
            owner: &self.owner,
            capture_points: self.capture_points,
            silo: self.silo,
            destructible_hp: self.destructible_hp(),
            teleporter: self.teleporter(),
            trait_state: self.trait_state(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ObservedTile {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = ObservedTileWire::deserialize(deserializer)?;
        let rare = RareTileState {
            destructible_hp: wire.destructible_hp,
            teleporter: wire.teleporter,
            trait_state: wire.trait_state,
        };
        Ok(Self {
            terrain: wire.terrain,
            visibility: wire.visibility,
            owner: wire.owner,
            capture_points: wire.capture_points,
            silo: wire.silo,
            rare: (!rare.is_empty()).then(|| Box::new(rare)),
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TileVisibility {
    Visible,
    Fogged,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ObservedPlayer {
    Private {
        id: PlayerId,
        team: TeamId,
        relation: Relation,
        funds: u64,
        status: PlayerStatus,
        commanders: Vec<Commander>,
        power_state: PowerState,
    },
    Public {
        id: PlayerId,
        team: TeamId,
        relation: Relation,
        status: PlayerStatus,
        commanders: Vec<PublicCommander>,
        power_state: PowerState,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Relation {
    #[serde(rename = "self")]
    Self_,
    Ally,
    Opponent,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicCommander {
    pub id: CommanderId,
    pub active: bool,
    pub power_charge: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum ObservedMatch {
    Active { own_team_offers: Vec<PlayerId> },
    Finished { outcome: Outcome },
}

/// One authoritative fact as a single recipient is entitled to see it.
///
/// Every variant is one `oneOf` branch of
/// `spec/schema/observed-event.schema.json`, so a projection the schema does not
/// license cannot be constructed — the same property [`Event`] gives the
/// authoritative side. A consumer translating these into a presentation model
/// matches exhaustively rather than reading a `type` string, and a new branch
/// stops its match compiling.
///
/// This is deliberately *not* [`Event`] with fields removed. A recipient sees
/// enemies by position rather than by id, learns of appearances and
/// disappearances that no authoritative event names, and receives the payload-free
/// [`PublicEventKind`] envelope in place of ten different public facts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ObservedEvent {
    /// `path` holds only the positions along the route the recipient could see
    /// the mover occupy, so a move through fog is reported with gaps.
    UnitMoved {
        unit: ObservedUnitRef,
        from: Pos,
        to: Pos,
        path: Vec<Pos>,
    },
    /// A unit the recipient could not see before is now visible.
    UnitAppeared { unit: ObservedUnit, position: Pos },
    /// A unit the recipient could see is no longer visible, without the
    /// recipient learning why.
    UnitDisappeared {
        unit: ObservedUnitRef,
        position: Pos,
    },
    /// The mover was interrupted. Reported without naming the blocker, which the
    /// recipient may not see.
    MovementStopped { unit: ObservedUnitRef },
    UnitChanged {
        unit: ObservedUnitRef,
        state: ObservedUnit,
        reason: ObservedReason,
    },
    UnitRemoved {
        unit: ObservedUnitRef,
        reason: ObservedReason,
    },
    TileChanged {
        position: Pos,
        tile: ObservedTile,
        reason: ObservedReason,
    },
    PlayerChanged {
        player: PlayerId,
        state: ObservedPlayer,
        reason: ObservedReason,
    },
    /// Public in full even when fog hides the units it hits
    /// (`spec/model/observation.md:318`).
    AreaStrikeResolved {
        strike: usize,
        policy: AreaStrikePolicy,
        center: Pos,
        radius: usize,
        damage: u8,
    },
    /// A public fact changed. Carries no payload by design; the recipient reads
    /// every new value from the post-observation
    /// (`spec/model/observation.md:329`), which is why [`observe_transition`]
    /// hands that back alongside these.
    PublicEvent { kind: PublicEventKind },
}

/// What one command looked like to one recipient.
///
/// The post-observation is part of this contract because `public-event` carries
/// no payload: `spec/model/observation.md:329` makes `post` the authority for
/// every public value those events only signal. Callers that need only event
/// payloads should use [`observe_events`], which avoids constructing the full
/// post-observation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedTransition {
    pub post: Observation,
    pub events: Vec<ObservedEvent>,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ObserveError {
    #[error("UnknownRecipient({0:?})")]
    UnknownRecipient(PlayerId),
    #[error("UnknownUnitOwner({0:?})")]
    UnknownUnitOwner(PlayerId),
    /// An event names a unit that is in neither the prior nor the next state.
    /// The three inputs are supplied independently over the protocol, so they
    /// can disagree; a typed event cannot fail to decode, but it can still
    /// reference a unit the caller did not send.
    #[error("UnknownUnit({0:?})")]
    UnknownUnit(UnitId),
}

/// Project a state as one recipient is entitled to see it.
pub fn observe(
    rules: &impl Visibility,
    state: &State,
    recipient: &PlayerId,
) -> Result<Observation, ObserveError> {
    let team = recipient_team(state, recipient)?;
    project_state(&rules.view(state, team), state, recipient, team)
}

/// Project one command for one recipient: the post-state as that recipient sees
/// it, and the events it is entitled to.
///
/// Both halves are needed together. A `public-event` carries no payload, so
/// `spec/model/observation.md:329` makes the post-observation the authority for
/// every value those events merely signal. This function constructs that full
/// observation and reuses its tile and player snapshots while projecting
/// events; [`observe_events`] instead constructs only the snapshots named by
/// events.
pub fn observe_transition(
    rules: &impl Visibility,
    state: &State,
    next_state: &State,
    events: &[Event],
    recipient: &PlayerId,
) -> Result<ObservedTransition, ObserveError> {
    let mut projection = transition_projection(rules, state, next_state, recipient, true)?;
    projection.project(events)?;
    Ok(ObservedTransition {
        post: projection
            .post
            .expect("observe_transition always projects the post-state"),
        events: projection.output,
    })
}

/// Project authoritative transition events for one recipient.
///
/// A caller that needs the recipient-safe post-state referenced by a
/// `public-event` should use [`observe_transition`]. Event-only callers use
/// this path so tile and player changes project just their snapshots, without
/// allocating an otherwise-unused full board observation.
pub fn observe_events(
    rules: &impl Visibility,
    state: &State,
    next_state: &State,
    events: &[Event],
    recipient: &PlayerId,
) -> Result<Vec<ObservedEvent>, ObserveError> {
    let mut projection = transition_projection(rules, state, next_state, recipient, false)?;
    projection.project(events)?;
    Ok(projection.output)
}

fn transition_projection<'a, R: Visibility>(
    rules: &'a R,
    state: &'a State,
    next_state: &'a State,
    recipient: &'a PlayerId,
    include_post: bool,
) -> Result<Projection<'a, R::View<'a>>, ObserveError> {
    let team = recipient_team(state, recipient)?;
    // The inputs are supplied independently over the protocol. Validate both
    // stores even on the event-only path so it has the same error contract as
    // a full post-observation.
    for candidate in [state, next_state] {
        for unit in &candidate.units {
            if candidate.find_player(&unit.owner).is_none() {
                return Err(ObserveError::UnknownUnitOwner(unit.owner.clone()));
            }
        }
    }
    // The recipient's team is taken from `state` throughout. A recipient who
    // changed teams is not a transition the specification admits; this still
    // rejects one who is absent from `next_state` altogether.
    recipient_team(next_state, recipient)?;

    let pre_view = rules.view(state, team);
    let post_view = rules.view(next_state, team);
    let post = include_post
        .then(|| project_state(&post_view, next_state, recipient, team))
        .transpose()?;
    let teammates: Vec<&PlayerId> = state
        .players
        .iter()
        .filter(|player| player.team == team)
        .map(|player| &player.id)
        .collect();
    Ok(Projection {
        pre_view,
        post_view,
        state,
        next_state,
        post,
        recipient,
        team,
        teammates,
        appeared: HashSet::new(),
        disappeared: HashSet::new(),
        output: Vec::new(),
    })
}

fn recipient_team<'a>(state: &'a State, recipient: &PlayerId) -> Result<&'a TeamId, ObserveError> {
    state
        .find_player(recipient)
        .map(|player| &player.team)
        .ok_or_else(|| ObserveError::UnknownRecipient(recipient.clone()))
}

fn project_state(
    view: &impl Viewpoint,
    state: &State,
    recipient: &PlayerId,
    team: &TeamId,
) -> Result<Observation, ObserveError> {
    let owners: HashMap<&PlayerId, &TeamId> =
        state.players.iter().map(|p| (&p.id, &p.team)).collect();
    let tiles = state
        .board
        .rows()
        .flatten()
        .map(|(position, t)| projected_tile(view, position, t))
        .collect();
    let players = state
        .players
        .iter()
        .map(|player| projected_player(player, recipient, team))
        .collect();
    let mut units = Vec::new();
    for u in &state.units {
        let owner_team = *owners
            .get(&u.owner)
            .ok_or_else(|| ObserveError::UnknownUnitOwner(u.owner.clone()))?;
        // A viewpoint already reports a teammate's unit as visible wherever it
        // is, including inside a transport, and an opponent's cargo as hidden.
        if view.unit(u) {
            units.push(observed_unit_snapshot(u, owner_team == team));
        }
    }
    units.sort_by_key(|unit| unit.reference);
    let match_state = match &state.match_state {
        Match::Active { draw_offers } => {
            let mut offers: Vec<_> = draw_offers
                .iter()
                .filter(|id| owners.get(id).is_some_and(|t| *t == team))
                .cloned()
                .collect();
            offers.sort();
            ObservedMatch::Active {
                own_team_offers: offers,
            }
        }
        Match::Finished { outcome } => ObservedMatch::Finished {
            outcome: outcome.clone(),
        },
    };
    Ok(Observation {
        ruleset: state.ruleset.clone(),
        recipient: recipient.clone(),
        settings: state.settings.clone(),
        board: ObservedBoard::new(state.board.width(), state.board.height(), tiles)
            .expect("the projection keeps the authoritative board's rectangle"),
        teams: state.teams.clone(),
        players,
        turn: state.turn.clone(),
        weather: state.weather.clone(),
        units,
        match_state,
    })
}

fn projected_tile(view: &impl Viewpoint, position: Pos, tile: &super::Tile) -> ObservedTile {
    let visible = view.position(position);
    // A teleporter pairing is disclosed even through fog; the rest of a
    // tile's rare state is not.
    let rare = RareTileState {
        destructible_hp: visible.then_some(tile.destructible_hp()).flatten(),
        teleporter: tile.teleporter().cloned(),
        trait_state: visible.then(|| tile.trait_state().cloned()).flatten(),
    };
    ObservedTile {
        terrain: tile.terrain,
        visibility: if visible {
            TileVisibility::Visible
        } else {
            TileVisibility::Fogged
        },
        owner: if visible {
            tile.owner.clone()
        } else {
            TileOwner::NotOwnable
        },
        capture_points: visible.then_some(tile.capture_points).flatten(),
        silo: visible.then_some(tile.silo).flatten(),
        rare: (!rare.is_empty()).then(|| Box::new(rare)),
    }
}

fn projected_player(player: &super::Player, recipient: &PlayerId, team: &TeamId) -> ObservedPlayer {
    if player.team == *team {
        ObservedPlayer::Private {
            id: player.id.clone(),
            team: player.team.clone(),
            relation: if player.id == *recipient {
                Relation::Self_
            } else {
                Relation::Ally
            },
            funds: player.funds,
            status: player.status,
            commanders: player.commanders.clone(),
            power_state: player.power_state.clone(),
        }
    } else {
        ObservedPlayer::Public {
            id: player.id.clone(),
            team: player.team.clone(),
            relation: Relation::Opponent,
            status: player.status,
            commanders: player
                .commanders
                .iter()
                .map(|commander| PublicCommander {
                    id: commander.id,
                    active: commander.active,
                    power_charge: commander.power_charge,
                })
                .collect(),
            power_state: player.power_state.clone(),
        }
    }
}

/// The context every event's projection shares.
///
/// These were eleven parameters threaded through free functions, which is why
/// two of them carried `#[allow(clippy::too_many_arguments)]`. Nothing here is
/// derivable from an event alone: whether a unit was visible before and after is
/// what decides between reporting a change, an appearance, and a disappearance.
struct Projection<'a, V: Viewpoint> {
    pre_view: V,
    post_view: V,
    state: &'a State,
    next_state: &'a State,
    /// The complete post-state when the caller requested an
    /// [`ObservedTransition`]. Event-only projection leaves this absent and
    /// constructs `tile-changed` and `player-changed` snapshots directly.
    post: Option<Observation>,
    recipient: &'a PlayerId,
    team: &'a TeamId,
    /// The recipient's own team. Short enough that a scan beats hashing.
    teammates: Vec<&'a PlayerId>,
    /// Appearances and disappearances already announced, so that several events
    /// about one unit produce at most one of each.
    appeared: HashSet<UnitId>,
    disappeared: HashSet<UnitId>,
    output: Vec<ObservedEvent>,
}

impl<V: Viewpoint> Projection<'_, V> {
    fn owns(&self, player: &PlayerId) -> bool {
        self.teammates.contains(&player)
    }

    fn visible_pre(&self, id: UnitId) -> bool {
        self.state
            .units
            .get(id)
            .is_some_and(|unit| self.pre_view.unit(unit))
    }

    fn visible_post(&self, id: UnitId) -> bool {
        self.next_state
            .units
            .get(id)
            .is_some_and(|unit| self.post_view.unit(unit))
    }

    fn project(&mut self, events: &[Event]) -> Result<(), ObserveError> {
        for event in events {
            let reason = event.reason();
            match event {
                Event::UnitActionChanged { unit, .. }
                | Event::UnitDamaged { unit, .. }
                | Event::UnitRepaired { unit, .. }
                | Event::UnitResourced { unit, .. }
                | Event::ConcealmentChanged { unit, .. }
                | Event::AutomaticRepair { unit, .. } => self.unit_fact(*unit, reason),
                Event::AutomaticSupply { units, .. } => {
                    for unit in units {
                        self.unit_fact(*unit, reason.clone());
                    }
                }
                Event::UnitMoved {
                    unit: id,
                    from,
                    to,
                    path,
                    ..
                } => self.movement(*id, *from, *to, path)?,
                Event::MovementTrapped { unit: id, .. } => {
                    let id = *id;
                    if self
                        .state
                        .units
                        .get(id)
                        .is_some_and(|unit| self.owns(&unit.owner))
                    {
                        self.output.push(ObservedEvent::MovementStopped {
                            unit: ObservedUnitRef::Friendly { unit: id },
                        });
                    }
                }
                Event::UnitCreated {
                    unit: id, position, ..
                } => {
                    let id = *id;
                    if self.visible_post(id)
                        && let Some(unit) = self.next_state.units.get(id)
                    {
                        let friendly = self.owns(&unit.owner);
                        self.push_appeared(unit, *position, friendly);
                    }
                }
                Event::UnitRemoved { unit, .. } => self.removal(*unit, reason)?,
                Event::UnitsJoined { source, target } => {
                    self.removal(*source, reason.clone())?;
                    self.unit_fact(*target, reason);
                }
                Event::UnitLoaded { unit: id, .. } => {
                    let id = *id;
                    let unit = self.state.units.get(id);
                    if unit.is_some_and(|unit| self.owns(&unit.owner)) {
                        self.unit_fact(id, reason);
                    } else if self.visible_pre(id)
                        && let Some(position) = unit.and_then(board_position)
                    {
                        self.push_disappeared(id, position);
                    }
                }
                Event::UnitUnloaded {
                    unit: id, position, ..
                } => {
                    let id = *id;
                    let unit = self.next_state.units.get(id);
                    if unit.is_some_and(|unit| self.owns(&unit.owner)) {
                        self.unit_fact(id, reason);
                    } else if self.visible_post(id)
                        && let Some(unit) = unit
                    {
                        self.push_appeared(unit, *position, false);
                    }
                }
                Event::TileOwnerChanged { position, .. }
                | Event::TileTerrainChanged { position, .. }
                | Event::CaptureChanged { position, .. }
                | Event::SiloChanged { position, .. }
                | Event::DestructibleDamaged { position, .. } => {
                    let position = *position;
                    if self.pre_view.position(position) || self.post_view.position(position) {
                        let tile = self.post.as_ref().map_or_else(
                            || {
                                projected_tile(
                                    &self.post_view,
                                    position,
                                    self.next_state.board.tile(position),
                                )
                            },
                            |post| post.board.tile(position).clone(),
                        );
                        self.output.push(ObservedEvent::TileChanged {
                            position,
                            tile,
                            reason,
                        });
                    }
                }
                // A player only learns their own funds changed; a power charge is
                // public, so it is projected to everyone.
                Event::FundsChanged { player, .. } if !self.owns(player) => {}
                Event::FundsChanged { player, .. } | Event::PowerChargeChanged { player, .. } => {
                    if let Some(player_state) = self.next_state.find_player(player) {
                        let snapshot = self
                            .post
                            .as_ref()
                            .and_then(|post| {
                                post.players.iter().find(|candidate| match candidate {
                                    ObservedPlayer::Private { id, .. }
                                    | ObservedPlayer::Public { id, .. } => id == player,
                                })
                            })
                            .cloned()
                            .unwrap_or_else(|| {
                                projected_player(player_state, self.recipient, self.team)
                            });
                        self.output.push(ObservedEvent::PlayerChanged {
                            player: player.clone(),
                            state: snapshot,
                            reason,
                        });
                    }
                }
                Event::DrawOfferChanged { player, .. } => {
                    if self.owns(player) {
                        self.public(event);
                    }
                }
                Event::PhaseChanged { .. }
                | Event::TurnSelected { .. }
                | Event::DayAdvanced { .. }
                | Event::WeatherChanged { .. }
                | Event::PowerActivated { .. }
                | Event::PowerEnded { .. }
                | Event::CommanderSwapped { .. }
                | Event::PlayerStatusChanged { .. }
                | Event::TeamEliminated { .. }
                | Event::MatchCompleted { .. } => self.public(event),
                Event::AreaStrikeResolved {
                    strike,
                    policy,
                    center,
                    radius,
                    damage,
                } => self.output.push(ObservedEvent::AreaStrikeResolved {
                    strike: *strike,
                    policy: *policy,
                    center: *center,
                    radius: *radius,
                    damage: *damage,
                }),
                // Deliberately unprojected. `spec/model/observation.md:337` withholds
                // both: `attack-resolved` would disclose the weapon and target of an
                // attack a recipient may not see, and `random-outcome` would leak the
                // tape a recipient is not entitled to read. The damage and state
                // changes they cause reach recipients through their own events.
                Event::AttackResolved { .. } | Event::RandomOutcome { .. } => {}
            }
        }
        Ok(())
    }

    /// Emit the payload-free envelope for a public fact.
    ///
    /// [`EventKind::public`] is exhaustive over every kind, so an event that
    /// reaches here without a public spelling is one this match should not have
    /// routed here — and `only_the_documented_kinds_are_public` pins which are
    /// which.
    fn public(&mut self, event: &Event) {
        self.output.extend(
            event
                .kind()
                .public()
                .map(|kind| ObservedEvent::PublicEvent { kind }),
        );
    }

    /// Project an ordinary fact about one unit: a change if the recipient could
    /// see it throughout, otherwise the appearance or disappearance that
    /// crossing the visibility boundary amounts to.
    fn unit_fact(&mut self, id: UnitId, reason: ObservedReason) {
        let Some(unit) = self.next_state.units.get(id) else {
            return;
        };
        match (self.visible_pre(id), self.visible_post(id)) {
            (true, true) => {
                let snapshot = observed_unit_snapshot(unit, self.owns(&unit.owner));
                self.output.push(ObservedEvent::UnitChanged {
                    unit: snapshot.reference,
                    state: snapshot,
                    reason,
                });
            }
            (false, true) => {
                if let Some(position) = board_position(unit) {
                    let friendly = self.owns(&unit.owner);
                    self.push_appeared(unit, position, friendly);
                }
            }
            (true, false) => {
                if let Some(position) = self.state.units.get(id).and_then(board_position) {
                    self.push_disappeared(id, position);
                }
            }
            (false, false) => {}
        }
    }

    /// Project a move. The recipient's own move is reported in full; an
    /// opponent's is reported only along the stretch the recipient could watch.
    fn movement(
        &mut self,
        id: UnitId,
        from: Pos,
        to: Pos,
        path: &[Pos],
    ) -> Result<(), ObserveError> {
        let unit = self
            .state
            .units
            .get(id)
            .ok_or(ObserveError::UnknownUnit(id))?;
        if self.owns(&unit.owner) {
            self.output.push(ObservedEvent::UnitMoved {
                unit: ObservedUnitRef::Friendly { unit: id },
                from,
                to,
                path: path.to_vec(),
            });
            return Ok(());
        }
        match (self.visible_pre(id), self.visible_post(id)) {
            (true, false) => self.push_disappeared(id, from),
            (false, true) => {
                if let Some(snapshot) = self.next_state.units.get(id) {
                    self.push_appeared(snapshot, to, false);
                }
            }
            (true, true) => {
                let post_unit = self
                    .next_state
                    .units
                    .get(id)
                    .ok_or(ObserveError::UnknownUnit(id))?;
                let observed_path = path
                    .iter()
                    .copied()
                    .filter(|position| {
                        self.pre_view.unit_at(post_unit, *position)
                            || self.post_view.unit_at(post_unit, *position)
                    })
                    .collect();
                self.output.push(ObservedEvent::UnitMoved {
                    unit: enemy_unit_ref(to),
                    from,
                    to,
                    path: observed_path,
                });
            }
            (false, false) => {}
        }
        Ok(())
    }

    /// Project a removal. An opponent's removal is only disclosed where the
    /// recipient can see the tile it happened on; otherwise the unit merely
    /// disappears.
    fn removal(&mut self, id: UnitId, reason: ObservedReason) -> Result<(), ObserveError> {
        let unit = self
            .state
            .units
            .get(id)
            .ok_or(ObserveError::UnknownUnit(id))?;
        if self.owns(&unit.owner) {
            self.output.push(ObservedEvent::UnitRemoved {
                unit: ObservedUnitRef::Friendly { unit: id },
                reason,
            });
        } else if self.visible_pre(id) {
            // A removed enemy that was visible must have been on the board:
            // visibility is only ever computed for board positions.
            let position = board_position(unit).ok_or(ObserveError::UnknownUnit(id))?;
            if self.post_view.position(position) {
                self.output.push(ObservedEvent::UnitRemoved {
                    unit: enemy_unit_ref(position),
                    reason,
                });
            } else {
                self.push_disappeared(id, position);
            }
        }
        Ok(())
    }

    fn push_appeared(&mut self, unit: &Unit, position: Pos, friendly: bool) {
        if self.appeared.insert(unit.id) {
            self.output.push(ObservedEvent::UnitAppeared {
                unit: observed_unit_snapshot(unit, friendly),
                position,
            });
        }
    }

    fn push_disappeared(&mut self, id: UnitId, position: Pos) {
        if self.disappeared.insert(id) {
            self.output.push(ObservedEvent::UnitDisappeared {
                unit: enemy_unit_ref(position),
                position,
            });
        }
    }
}

fn observed_unit_snapshot(unit: &Unit, friendly: bool) -> ObservedUnit {
    ObservedUnit {
        reference: if friendly {
            ObservedUnitRef::Friendly { unit: unit.id }
        } else {
            enemy_unit_ref(
                board_position(unit).expect("an observed enemy unit must be on the board"),
            )
        },
        kind: unit.kind,
        owner: unit.owner.clone(),
        hp: unit.hp,
        fuel: unit.fuel,
        ammo: unit.ammo,
        action: unit.action,
        concealment: unit.concealment,
        location: unit.location.clone(),
    }
}

fn enemy_unit_ref(position: Pos) -> ObservedUnitRef {
    ObservedUnitRef::Enemy { position }
}

fn board_position(unit: &Unit) -> Option<Pos> {
    match unit.location {
        Location::Board { position } => Some(position),
        Location::Cargo { .. } => None,
    }
}
/// What the projection *does* is asserted by `spec/fixtures/fog/**` through the
/// conformance corpus and the goldens, against the real visibility operators.
/// What is left for Rust is the wire shape of the values it returns, which no
/// fixture pins independently of the projection that produced it.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::{KnownReason, TileVisibility};
    use serde_json::json;

    /// Every branch of `spec/schema/observed-event.schema.json`, in the shape
    /// the schema names it. The goldens cover whichever branches the corpus
    /// happens to produce; this covers all ten.
    #[test]
    fn observed_events_serialize_as_their_schema_branches() {
        let enemy = ObservedUnitRef::Enemy {
            position: Pos::new(1, 2),
        };
        let friendly = ObservedUnitRef::Friendly {
            unit: UnitId::new(7),
        };
        let unit = ObservedUnit {
            reference: friendly,
            kind: UnitKindId::Infantry,
            owner: PlayerId::from("p1"),
            hp: 100,
            fuel: 99,
            ammo: 0,
            action: UnitAction::Ready,
            concealment: Concealment::Exposed,
            location: Location::Board {
                position: Pos::new(1, 2),
            },
        };
        let cases = [
            (
                ObservedEvent::UnitMoved {
                    unit: enemy,
                    from: Pos::new(0, 0),
                    to: Pos::new(1, 2),
                    path: vec![Pos::new(0, 0), Pos::new(1, 2)],
                },
                json!({"type":"unit-moved","unit":{"type":"enemy","position":[1,2]},
                       "from":[0,0],"to":[1,2],"path":[[0,0],[1,2]]}),
            ),
            (
                ObservedEvent::UnitAppeared {
                    unit: unit.clone(),
                    position: Pos::new(1, 2),
                },
                json!({"type":"unit-appeared","unit":serde_json::to_value(&unit).unwrap(),
                       "position":[1,2]}),
            ),
            (
                ObservedEvent::UnitDisappeared {
                    unit: enemy,
                    position: Pos::new(1, 2),
                },
                json!({"type":"unit-disappeared","unit":{"type":"enemy","position":[1,2]},
                       "position":[1,2]}),
            ),
            (
                ObservedEvent::MovementStopped { unit: friendly },
                json!({"type":"movement-stopped","unit":{"type":"friendly","unit":7}}),
            ),
            (
                ObservedEvent::UnitChanged {
                    unit: friendly,
                    state: unit.clone(),
                    reason: ObservedReason::Declared(KnownReason::Combat.into()),
                },
                json!({"type":"unit-changed","unit":{"type":"friendly","unit":7},
                       "state":serde_json::to_value(&unit).unwrap(),"reason":"combat"}),
            ),
            (
                ObservedEvent::UnitRemoved {
                    unit: friendly,
                    reason: ObservedReason::Kind(crate::event::EventKind::UnitsJoined),
                },
                json!({"type":"unit-removed","unit":{"type":"friendly","unit":7},
                       "reason":"units-joined"}),
            ),
            (
                ObservedEvent::AreaStrikeResolved {
                    strike: 0,
                    policy: AreaStrikePolicy::UnitValue,
                    center: Pos::new(3, 4),
                    radius: 2,
                    damage: 30,
                },
                json!({"type":"area-strike-resolved","strike":0,"policy":"unit-value",
                       "center":[3,4],"radius":2,"damage":30}),
            ),
            (
                ObservedEvent::PublicEvent {
                    kind: PublicEventKind::DayAdvanced,
                },
                json!({"type":"public-event","kind":"day-advanced"}),
            ),
        ];
        for (event, expected) in cases {
            assert_eq!(serde_json::to_value(&event).unwrap(), expected);
        }

        let tile = ObservedTile {
            terrain: TerrainId::Plain,
            visibility: TileVisibility::Visible,
            owner: TileOwner::NotOwnable,
            capture_points: None,
            silo: None,
            rare: None,
        };
        assert_eq!(
            serde_json::to_value(ObservedEvent::TileChanged {
                position: Pos::new(1, 2),
                tile,
                reason: ObservedReason::Kind(crate::event::EventKind::CaptureChanged),
            })
            .unwrap(),
            json!({"type":"tile-changed","position":[1,2],
                   "tile":{"terrain":"plain","visibility":"visible"},
                   "reason":"capture-changed"})
        );

        let player = ObservedPlayer::Public {
            id: PlayerId::from("p2"),
            team: TeamId::from("t2"),
            relation: Relation::Opponent,
            status: PlayerStatus::Active,
            commanders: vec![],
            power_state: PowerState::None,
        };
        assert_eq!(
            serde_json::to_value(ObservedEvent::PlayerChanged {
                player: PlayerId::from("p2"),
                state: player.clone(),
                reason: ObservedReason::Declared(KnownReason::Combat.into()),
            })
            .unwrap(),
            json!({"type":"player-changed","player":"p2",
                   "state":serde_json::to_value(&player).unwrap(),"reason":"combat"})
        );
    }

    /// `self` is a Rust keyword and the schema's value for the recipient's own
    /// seat, so the rename is the only thing keeping the two spellings apart.
    #[test]
    fn relation_self_serializes_as_schema_value() {
        assert_eq!(serde_json::to_value(Relation::Self_).unwrap(), "self");
    }
}
