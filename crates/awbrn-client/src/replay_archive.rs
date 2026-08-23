use std::collections::BTreeMap;
use std::num::NonZeroU8;

use awbrn_game::{Authority, GameSetup, PlayerSetup, StoredActionEvent};
use awbrn_map::{AwbrnMap, AwbwMap, AwbwMapData};
use awbrn_types::{AwbwCoId, Co, CoExt, PlayerFaction};
use awvm::semantic::{Observation, ObservedTransition};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AwbrnReplayPlayer {
    pub user_id: String,
    pub faction_id: u8,
    pub team: Option<NonZeroU8>,
    pub starting_funds: u32,
    pub co_id: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AwbrnReplaySetup {
    pub match_id: String,
    pub map_id: u32,
    pub map: AwbwMapData,
    pub players: Vec<AwbrnReplayPlayer>,
    pub fog_enabled: bool,
    pub starting_funds: u32,
    #[serde(default)]
    pub rng_seed: Option<u64>,
}

#[derive(Deserialize)]
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
        let map = AwbwMap::try_from(&self.setup.map).map_err(|error| error.to_string())?;
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
        Ok(GameSetup {
            map: AwbrnMap::from_map(&map),
            players,
            fog_enabled: self.setup.fog_enabled,
            rng_seed: self.setup.rng_seed.unwrap_or(0),
        })
    }
}

pub enum ReplayArchive {
    Awbw(awbw_replay::AwbwReplay),
    Awbrn(AwbrnReplayFile),
}

impl ReplayArchive {
    pub fn parse(data: &[u8]) -> Result<Self, String> {
        if data
            .iter()
            .copied()
            .find(|byte| !byte.is_ascii_whitespace())
            == Some(b'{')
        {
            return AwbrnReplayFile::parse(data).map(Self::Awbrn);
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

pub enum ReplayTimeline {
    Awbw {
        initial: Box<awvm_awbw::RecordedAdapter>,
        current: Box<awvm_awbw::RecordedAdapter>,
        checkpoints: BTreeMap<usize, awvm_awbw::RecordedAdapter>,
    },
    Awbrn {
        setup: GameSetup,
        current: Box<Authority>,
    },
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

impl ReplayTimeline {
    pub fn initial_observations(&self) -> Result<Vec<Observation>, String> {
        match self {
            Self::Awbw { initial, .. } => observe_awbw_state(initial),
            Self::Awbrn { setup, .. } => {
                let initial = Authority::new(setup).map_err(|error| error.to_string())?;
                initial
                    .players()
                    .map(|player| {
                        awvm::semantic::observe(
                            &awvm::semantic::AwbwVisibility,
                            initial.state(),
                            &initial.player(player),
                        )
                        .map_err(|error| error.to_string())
                    })
                    .collect()
            }
        }
    }

    pub fn advance(
        &mut self,
        archive: &ReplayArchive,
        index: usize,
    ) -> Result<Vec<ObservedTransition>, String> {
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
                let players = transition.post_state().players.clone();
                let observations = players
                    .iter()
                    .map(|player| {
                        transition
                            .observe(player.id())
                            .map_err(|error| error.to_string())
                    })
                    .collect();
                let completed = index + 1;
                if completed.is_multiple_of(64) {
                    checkpoints
                        .entry(completed)
                        .or_insert_with(|| (**current).clone());
                }
                observations
            }
            (Self::Awbrn { current, .. }, ReplayArchive::Awbrn(replay)) => {
                let event = replay
                    .actions
                    .get(index)
                    .ok_or_else(|| format!("Missing AWBRN action {index}"))?;
                let transition = current
                    .execute_recorded(event.player, &event.command, &event.random)
                    .map_err(|error| format!("Could not replay AWBRN action {index}: {error}"))?;
                current
                    .players()
                    .map(|player| {
                        transition
                            .observe(current, &current.player(player))
                            .map_err(|error| error.to_string())
                    })
                    .collect()
            }
            _ => Err("Replay archive and timeline formats do not match".to_string()),
        }
    }

    pub fn rebuild(
        &mut self,
        archive: &ReplayArchive,
        target: usize,
    ) -> Result<Vec<ObservedTransition>, String> {
        match self {
            Self::Awbw {
                initial,
                current,
                checkpoints,
            } => {
                let (checkpoint_index, checkpoint) = checkpoints
                    .range(..=target)
                    .next_back()
                    .map(|(index, adapter)| (*index, adapter.clone()))
                    .unwrap_or_else(|| (0, (**initial).clone()));
                **current = checkpoint;
                let mut observations = observe_awbw_state(current)?
                    .into_iter()
                    .map(|post| ObservedTransition {
                        post,
                        events: Vec::new(),
                    })
                    .collect();
                for index in checkpoint_index..target {
                    observations = self.advance(archive, index)?;
                }
                return Ok(observations);
            }
            Self::Awbrn { setup, current } => {
                **current = Authority::new(setup).map_err(|error| error.to_string())?
            }
        }
        let mut observations = self
            .initial_observations()?
            .into_iter()
            .map(|post| ObservedTransition {
                post,
                events: Vec::new(),
            })
            .collect();
        for index in 0..target {
            observations = self.advance(archive, index)?;
        }
        Ok(observations)
    }
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
    use serde_json::json;

    fn replay_json(version: u32) -> Vec<u8> {
        let map: serde_json::Value = serde_json::from_slice(
            &std::fs::read(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../assets/maps/162795.json"),
            )
            .unwrap(),
        )
        .unwrap();
        serde_json::to_vec(&json!({
            "version": version,
            "setup": {
                "matchId": "0123456789abcdef",
                "mapId": 162795,
                "map": map,
                "players": [
                    { "userId": "one", "factionId": 1, "team": null, "startingFunds": 0, "coId": 1 },
                    { "userId": "two", "factionId": 2, "team": null, "startingFunds": 0, "coId": 2 }
                ],
                "fogEnabled": false,
                "startingFunds": 0,
                "creatorUserId": "one"
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
        assert_eq!(replay.setup.map_id, 162795);
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
}
