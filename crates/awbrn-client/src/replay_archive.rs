use std::collections::BTreeMap;
use std::num::NonZeroU8;

use awbrn_game::{Authority, GameSetup, PlayerSetup, StoredActionEvent};
use awbrn_map::{AwbrnMap, ValidatedMapDocument};
use awbrn_types::{AwbwCoId, Co, CoExt, PlayerFaction};
use awvm::semantic::{Observation, ObservedTransition};
use serde::Deserialize;

/// One seat, as the server archived it.
///
/// A seat is held by a person or by the server, so `userId` and `aiProfileId`
/// are both optional and the archive can name either one.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AwbrnReplayPlayer {
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub ai_profile_id: Option<String>,
    pub faction_id: u8,
    pub team: Option<NonZeroU8>,
    pub starting_funds: u32,
    pub co_id: u32,
}

/// The setup a match started from, as the server archived it.
///
/// The fields the archive holds beyond these ones - who made the match, the
/// pool it counted for, the clock it ran on - say how the match was played
/// rather than what board it was played on, so playback does not read them.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AwbrnReplaySetup {
    pub match_id: String,
    /// The AWBRN map identifier, which is a string and not an AWBW number.
    pub map_id: String,
    pub revision: u32,
    /// The map itself, so playback needs no map from the catalog.
    pub map: ValidatedMapDocument,
    pub players: Vec<AwbrnReplayPlayer>,
    pub fog_enabled: bool,
    pub starting_funds: u32,
    #[serde(default)]
    pub rng_seed: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct AwbrnReplayFile {
    pub version: u32,
    pub setup: AwbrnReplaySetup,
    pub actions: Vec<StoredActionEvent>,
}

impl AwbrnReplayFile {
    pub fn parse(data: &[u8]) -> Result<Self, String> {
        let replay: Self = serde_json::from_slice(data)
            .map_err(|error| format!("Could not parse AWBRN replay JSON: {error}"))?;
        if replay.version != 1 {
            return Err(format!(
                "Unsupported AWBRN replay version {}",
                replay.version
            ));
        }
        replay.game_setup()?;
        Ok(replay)
    }

    pub fn game_setup(&self) -> Result<GameSetup, String> {
        let players = self
            .setup
            .players
            .iter()
            .map(|player| {
                let faction = PlayerFaction::from_awbw_id(player.faction_id)
                    .ok_or_else(|| format!("Unknown AWBW faction ID {}", player.faction_id))?;
                let co_id = AwbwCoId::new(player.co_id);
                let co = Co::from_awbw_id(co_id)
                    .ok_or_else(|| format!("Unknown AWBW CO ID {}", player.co_id))?;
                Ok(PlayerSetup {
                    faction,
                    team: player.team,
                    starting_funds: player.starting_funds,
                    co,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let armies: Vec<_> = players.iter().map(|player| player.faction).collect();
        Ok(GameSetup {
            // The map carries its own starting units.
            map: AwbrnMap::from_map(&self.setup.map.map().with_seat_armies(&armies)),
            players,
            fog_enabled: self.setup.fog_enabled,
            rng_seed: self.setup.rng_seed.unwrap_or(0),
        })
    }
}

pub enum ReplayArchive {
    Awbw(awbw_replay::AwbwReplay),
    /// Boxed: the file holds the map it was played on, so it dwarfs the other
    /// variant.
    Awbrn(Box<AwbrnReplayFile>),
}

impl std::fmt::Debug for ReplayArchive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReplayArchive").finish_non_exhaustive()
    }
}

impl ReplayArchive {
    pub fn parse(data: &[u8]) -> Result<Self, String> {
        if data
            .iter()
            .copied()
            .find(|byte| !byte.is_ascii_whitespace())
            == Some(b'{')
        {
            return AwbrnReplayFile::parse(data).map(Box::new).map(Self::Awbrn);
        }
        awbw_replay::ReplayParser::new()
            .parse(data)
            .map(Self::Awbw)
            .map_err(|error| format!("Could not parse AWBW replay archive: {error:?}"))
    }

    pub fn len(&self) -> usize {
        match self {
            Self::Awbw(replay) => replay.turns.len(),
            Self::Awbrn(replay) => replay.actions.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn awbw(&self) -> Option<&awbw_replay::AwbwReplay> {
        match self {
            Self::Awbw(replay) => Some(replay),
            Self::Awbrn(_) => None,
        }
    }
}

/// How many actions separate the positions a seek may start from.
///
/// A seek replays at most this many actions, which is what keeps a dragged
/// scrubber inside a frame.
pub const CHECKPOINT_INTERVAL: usize = 64;

pub enum ReplayTimeline {
    Awbw {
        initial: Box<awvm_awbw::RecordedAdapter>,
        current: Box<awvm_awbw::RecordedAdapter>,
        checkpoints: BTreeMap<usize, awvm_awbw::RecordedAdapter>,
    },
    Awbrn {
        setup: GameSetup,
        current: Box<Authority>,
        checkpoints: BTreeMap<usize, Authority>,
    },
}

impl std::fmt::Debug for ReplayTimeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReplayTimeline").finish_non_exhaustive()
    }
}

impl From<awvm_awbw::RecordedAdapter> for ReplayTimeline {
    fn from(adapter: awvm_awbw::RecordedAdapter) -> Self {
        Self::Awbw {
            initial: Box::new(adapter.clone()),
            current: Box::new(adapter),
            checkpoints: BTreeMap::new(),
        }
    }
}

/// Where an archive stood at one boundary.
///
/// The board is not here. This is what the controls read to label a moment
/// and to find where a turn begins, so a whole archive's worth of it stays
/// smaller than a single board would be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayBoundary {
    pub day: u32,
    /// The seat holding the turn here, or `None` once the game is over.
    pub active_player: Option<u32>,
}

impl ReplayTimeline {
    /// Replay the whole archive once, recording the turn each boundary falls
    /// in.
    ///
    /// The pass runs on a timeline of its own, so the position the viewer is
    /// reading never moves. Days and seats come from the projections AWVM
    /// produces rather than from reading the archived actions, because what
    /// counts as a turn is a rule and the archive does not hold the rules.
    ///
    /// The pass keeps the checkpoints it passes, so a seek anywhere in the
    /// archive replays at most one checkpoint interval.
    pub fn outline(&mut self, archive: &ReplayArchive) -> Result<Vec<ReplayBoundary>, String> {
        let mut scratch = self.rewound()?;
        let opening = scratch
            .initial_observations()?
            .first()
            .map(|observation| boundary(&observation.turn))
            .unwrap_or(ReplayBoundary {
                day: 1,
                active_player: None,
            });

        let mut boundaries = Vec::with_capacity(archive.len() + 1);
        boundaries.push(opening);
        for index in 0..archive.len() {
            let observed = scratch.advance(archive, index)?;
            let boundary = observed
                .first()
                .map(|transition| boundary(&transition.post.turn))
                .unwrap_or(opening);
            boundaries.push(boundary);
        }
        match (self, scratch) {
            (
                Self::Awbw { checkpoints, .. },
                Self::Awbw {
                    checkpoints: kept, ..
                },
            ) => *checkpoints = kept,
            (
                Self::Awbrn { checkpoints, .. },
                Self::Awbrn {
                    checkpoints: kept, ..
                },
            ) => *checkpoints = kept,
            _ => unreachable!("a rewound timeline keeps its format"),
        }
        Ok(boundaries)
    }

    /// A copy of this timeline standing before its first action.
    fn rewound(&self) -> Result<Self, String> {
        match self {
            Self::Awbw { initial, .. } => Ok(Self::Awbw {
                initial: initial.clone(),
                current: initial.clone(),
                checkpoints: BTreeMap::new(),
            }),
            Self::Awbrn { setup, .. } => Ok(Self::Awbrn {
                setup: setup.clone(),
                current: Box::new(Authority::new(setup).map_err(|error| error.to_string())?),
                checkpoints: BTreeMap::new(),
            }),
        }
    }

    /// The authoritative state before the first action of an AWBW archive.
    ///
    /// An AWBRN archive starts from its setup, so it has no stored state.
    pub fn initial_state(&self) -> Option<&awvm::semantic::State> {
        match self {
            Self::Awbw { initial, .. } => Some(initial.state()),
            Self::Awbrn { .. } => None,
        }
    }

    pub fn initial_observations(&self) -> Result<Vec<Observation>, String> {
        match self {
            Self::Awbw { initial, .. } => observe_awbw_state(initial),
            Self::Awbrn { setup, .. } => {
                observe_authority(&Authority::new(setup).map_err(|error| error.to_string())?)
            }
        }
    }

    pub fn advance(
        &mut self,
        archive: &ReplayArchive,
        index: usize,
    ) -> Result<Vec<ObservedTransition>, String> {
        self.apply(archive, index, true)
    }

    /// Apply one action, projecting it for every player only when `observe`
    /// is set. A seek passes through actions nobody is shown, and projecting
    /// them is most of what replaying them costs.
    fn apply(
        &mut self,
        archive: &ReplayArchive,
        index: usize,
        observe: bool,
    ) -> Result<Vec<ObservedTransition>, String> {
        let completed = index + 1;
        match (self, archive) {
            (
                Self::Awbw {
                    current,
                    checkpoints,
                    ..
                },
                ReplayArchive::Awbw(replay),
            ) => {
                let action = replay
                    .turns
                    .get(index)
                    .ok_or_else(|| format!("Missing AWBW action {index}"))?;
                let transition = current.advance(action).map_err(|error| error.to_string())?;
                let observations = if observe {
                    transition
                        .post_state()
                        .players
                        .iter()
                        .map(|player| {
                            transition
                                .observe(player.id())
                                .map_err(|error| error.to_string())
                        })
                        .collect::<Result<_, _>>()?
                } else {
                    Vec::new()
                };
                if completed.is_multiple_of(CHECKPOINT_INTERVAL) {
                    checkpoints
                        .entry(completed)
                        .or_insert_with(|| (**current).clone());
                }
                Ok(observations)
            }
            (
                Self::Awbrn {
                    current,
                    checkpoints,
                    ..
                },
                ReplayArchive::Awbrn(replay),
            ) => {
                let event = replay
                    .actions
                    .get(index)
                    .ok_or_else(|| format!("Missing AWBRN action {index}"))?;
                let transition = current
                    .execute_recorded(event.player, &event.command, &event.random)
                    .map_err(|error| format!("Could not replay AWBRN action {index}: {error}"))?;
                let observations = if observe {
                    current
                        .players()
                        .map(|player| {
                            transition
                                .observe(current, &current.player(player))
                                .map_err(|error| error.to_string())
                        })
                        .collect::<Result<_, _>>()?
                } else {
                    Vec::new()
                };
                if completed.is_multiple_of(CHECKPOINT_INTERVAL) {
                    checkpoints
                        .entry(completed)
                        .or_insert_with(|| (**current).clone());
                }
                Ok(observations)
            }
            _ => Err("Replay archive and timeline formats do not match".to_string()),
        }
    }

    /// Stand at `target`, replaying from the nearest checkpoint before it.
    ///
    /// The projections are of the position arrived at, with no events: a
    /// seek shows where it landed rather than how it got there.
    pub fn rebuild(
        &mut self,
        archive: &ReplayArchive,
        target: usize,
    ) -> Result<Vec<ObservedTransition>, String> {
        let start = match self {
            Self::Awbw {
                initial,
                current,
                checkpoints,
            } => {
                let (start, checkpoint) = checkpoints
                    .range(..=target)
                    .next_back()
                    .map(|(index, adapter)| (*index, adapter))
                    .unwrap_or((0, initial));
                **current = checkpoint.clone();
                start
            }
            Self::Awbrn {
                setup,
                current,
                checkpoints,
            } => match checkpoints.range(..=target).next_back() {
                Some((index, authority)) => {
                    **current = authority.clone();
                    *index
                }
                None => {
                    **current = Authority::new(setup).map_err(|error| error.to_string())?;
                    0
                }
            },
        };
        for index in start..target {
            self.apply(archive, index, false)?;
        }
        let observations = match self {
            Self::Awbw { current, .. } => observe_awbw_state(current)?,
            Self::Awbrn { current, .. } => observe_authority(current)?,
        };
        Ok(observations
            .into_iter()
            .map(|post| ObservedTransition {
                post,
                events: Vec::new(),
            })
            .collect())
    }
}

fn boundary(turn: &awvm::semantic::Turn) -> ReplayBoundary {
    ReplayBoundary {
        day: u32::try_from(turn.day).unwrap_or(u32::MAX),
        active_player: turn.active_player.as_str().parse().ok(),
    }
}

fn observe_authority(authority: &Authority) -> Result<Vec<Observation>, String> {
    authority
        .players()
        .map(|player| {
            awvm::semantic::observe(
                &awvm::semantic::AwbwVisibility,
                authority.state(),
                &authority.player(player),
            )
            .map_err(|error| error.to_string())
        })
        .collect()
}

fn observe_awbw_state(adapter: &awvm_awbw::RecordedAdapter) -> Result<Vec<Observation>, String> {
    adapter
        .state()
        .players
        .iter()
        .map(|player| {
            awvm::semantic::observe(
                &awvm::semantic::AwbwVisibility,
                adapter.state(),
                player.id(),
            )
            .map_err(|error| error.to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use awbrn_map::{AwbrnMapDocument, AwbrnMapMetadata, AwbwMap, AwbwMapData};
    use serde_json::json;

    /// The archive as the server writes it: a stored AWBRN map document, and
    /// a string map identifier rather than an AWBW number.
    fn replay_json(version: u32) -> Vec<u8> {
        let awbw: AwbwMapData = serde_json::from_slice(
            &std::fs::read(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../assets/maps/162795.json"),
            )
            .unwrap(),
        )
        .unwrap();
        let map = AwbwMap::try_from(&awbw).unwrap();
        let document = AwbrnMapDocument::from_awbw_map(
            &map,
            AwbrnMapMetadata {
                name: awbw.name.clone(),
                author: awbw.author.clone(),
                player_count: awbw.player_count,
            },
        );
        serde_json::to_vec(&json!({
            "version": version,
            "setup": {
                "matchId": "x9lw2qab0jlf",
                "mapId": "000000000001",
                "revision": 1,
                "map": document,
                "players": [
                    {
                        "userId": "one",
                        "aiProfileId": null,
                        "factionId": 1,
                        "team": null,
                        "startingFunds": 0,
                        "coId": 1
                    },
                    {
                        "userId": null,
                        "aiProfileId": "scout",
                        "factionId": 2,
                        "team": null,
                        "startingFunds": 0,
                        "coId": 2
                    }
                ],
                "fogEnabled": false,
                "startingFunds": 0,
                "creatorUserId": "one",
                "pool": null,
                "season": null,
                "clock": { "initialMs": 604800000, "incrementMs": 172800000, "maxBankMs": 604800000 }
            },
            "actions": []
        }))
        .unwrap()
    }

    #[test]
    fn detects_awbrn_json_and_uses_the_embedded_map() {
        let replay = ReplayArchive::parse(&replay_json(1)).unwrap();
        let ReplayArchive::Awbrn(replay) = replay else {
            panic!("JSON replay should use the AWBRN adapter");
        };
        assert_eq!(replay.setup.match_id, "x9lw2qab0jlf");
        assert_eq!(replay.setup.map_id, "000000000001");
        assert_eq!(replay.actions.len(), 0);
        assert_eq!(replay.game_setup().unwrap().players.len(), 2);
    }

    #[test]
    fn rejects_unknown_awbrn_versions() {
        let error = match ReplayArchive::parse(&replay_json(2)) {
            Ok(_) => panic!("unknown version should fail"),
            Err(error) => error,
        };
        assert!(error.contains("Unsupported AWBRN replay version 2"));
    }

    /// Land on boundaries through the checkpoints an outline keeps, and check
    /// each against the board reached by stepping every action from the
    /// opening.
    fn assert_seeks_match_stepping(
        archive: &ReplayArchive,
        mut outlined: ReplayTimeline,
        mut stepped: ReplayTimeline,
    ) {
        outlined.outline(archive).unwrap();
        let kept = match &outlined {
            ReplayTimeline::Awbw { checkpoints, .. } => checkpoints.len(),
            ReplayTimeline::Awbrn { checkpoints, .. } => checkpoints.len(),
        };
        assert_eq!(kept, archive.len() / CHECKPOINT_INTERVAL);
        assert!(kept >= 2, "the archive has to cross two checkpoints");

        let mut expected = stepped.initial_observations().unwrap();
        for target in 0..=archive.len() {
            if target > 0 {
                expected = stepped
                    .advance(archive, target - 1)
                    .unwrap()
                    .into_iter()
                    .map(|transition| transition.post)
                    .collect();
            }
            if target > 3 * CHECKPOINT_INTERVAL && target < archive.len() {
                continue;
            }
            let landed: Vec<_> = outlined
                .rebuild(archive, target)
                .unwrap()
                .into_iter()
                .map(|transition| transition.post)
                .collect();
            assert_eq!(landed, expected, "boundary {target}");
        }
    }

    #[test]
    fn an_awbw_seek_from_a_kept_checkpoint_lands_where_stepping_does() {
        let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let archive =
            ReplayArchive::parse(&std::fs::read(assets.join("replays/1362397.zip")).unwrap())
                .unwrap();
        let map: AwbwMapData =
            serde_json::from_slice(&std::fs::read(assets.join("maps/162795.json")).unwrap())
                .unwrap();
        let adapter = awvm_awbw::RecordedAdapter::new(archive.awbw().unwrap(), &map).unwrap();

        assert_seeks_match_stepping(
            &archive,
            ReplayTimeline::from(adapter.clone()),
            ReplayTimeline::from(adapter),
        );
    }

    #[test]
    fn an_awbrn_seek_from_a_kept_checkpoint_lands_where_stepping_does() {
        let archive = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../assets/replays/awbrn-replay-3zceh5a97b31o.json"),
        )
        .unwrap();
        let ReplayArchive::Awbrn(mut replay) = ReplayArchive::parse(&archive).unwrap() else {
            panic!("JSON replay should use the AWBRN adapter");
        };
        let setup = replay.game_setup().unwrap();
        let mut authority = Authority::new(&setup).unwrap();
        replay.actions = (0..150)
            .map(|turn| {
                let player = awbrn_game::PlayerId(turn % 2);
                let command = awbrn_game::GameCommand::EndTurn;
                authority.execute(player, &command).unwrap();
                StoredActionEvent {
                    player,
                    command,
                    random: authority.last_random_tokens().to_vec(),
                }
            })
            .collect();
        let timeline = || ReplayTimeline::Awbrn {
            setup: setup.clone(),
            current: Box::new(Authority::new(&setup).unwrap()),
            checkpoints: BTreeMap::new(),
        };

        assert_seeks_match_stepping(&ReplayArchive::Awbrn(replay), timeline(), timeline());
    }
}
