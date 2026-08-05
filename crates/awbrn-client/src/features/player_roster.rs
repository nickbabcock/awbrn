use crate::features::event_bus::{
    EventSink, PlayerRosterEntry, PlayerRosterSnapshot, PlayerRosterStats,
};
use crate::features::player_display::{PlayerDisplayFactionOverrides, display_faction_for_player};
use crate::loading::LiveMatchPlayer;
use awbrn_content::{CoPortraitMetadata, co_portrait_by_awbw_id, co_portraits};
use awbrn_game::replay::{AwbwUnitId, ReplayState};
use awbrn_game::world::{Faction, GraphicalHp, TerrainTile, Unit, ViewerVisibility};
use awbrn_types::{Faction as TerrainFaction, GraphicalTerrain, PlayerFaction, UnitExt};
use awbw_replay::AwbwReplay;
use awbw_replay::game_models::AwbwPlayer;
use awvm::semantic::{Observation, ObservedPlayer};
use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

#[derive(Resource, Clone)]
pub struct PlayerRosterConfig {
    pub match_id: u32,
    pub map_id: u32,
    pub funds_per_property: u32,
    pub players: Vec<PlayerRosterPlayer>,
}

#[derive(Clone)]
pub struct PlayerRosterPlayer {
    pub player_id: awbrn_types::AwbwGamePlayerId,
    pub user_id: awbrn_types::AwbwPlayerId,
    pub turn_order: u32,
    pub team: Option<String>,
    pub eliminated: bool,
    pub faction: PlayerFaction,
    pub faction_code: String,
    pub faction_name: String,
    pub co_key: Option<String>,
    pub co_name: Option<String>,
    pub tag_co_key: Option<String>,
    pub tag_co_name: Option<String>,
}

#[derive(Resource, Clone, Default)]
pub struct PlayerFunds(pub HashMap<awbrn_types::AwbwGamePlayerId, u32>);

impl PlayerFunds {
    pub fn set(&mut self, player_id: awbrn_types::AwbwGamePlayerId, funds: u32) {
        self.0.insert(player_id, funds);
    }

    pub fn subtract(&mut self, player_id: awbrn_types::AwbwGamePlayerId, amount: u32) {
        let entry = self.0.entry(player_id).or_default();
        *entry = entry.saturating_sub(amount);
    }

    pub fn get(&self, player_id: awbrn_types::AwbwGamePlayerId) -> u32 {
        self.0.get(&player_id).copied().unwrap_or_default()
    }
}

/// Public CO power meter values for the active commander of each player.
///
/// The current charge and use count are public AWVM facts. COP and SCOP costs
/// and the value of one star are calculated through AWVM's revisioned commander
/// query, keeping the UI payload self-contained without duplicating the scaling
/// formula.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerPowerMeter {
    pub charge: u32,
    pub cop_cost: Option<u32>,
    pub scop_cost: Option<u32>,
    /// Charge that one star is worth at this player's current use count.
    pub star_charge: Option<u32>,
    /// The power that this player has active.
    pub active_power: Option<awvm::commander::PowerLevel>,
}

#[derive(Resource, Clone, Default)]
pub struct PlayerPowerMeters(pub HashMap<awbrn_types::AwbwGamePlayerId, PlayerPowerMeter>);

impl PlayerPowerMeters {
    pub fn set(&mut self, player_id: awbrn_types::AwbwGamePlayerId, meter: PlayerPowerMeter) {
        self.0.insert(player_id, meter);
    }

    pub fn get(&self, player_id: awbrn_types::AwbwGamePlayerId) -> Option<PlayerPowerMeter> {
        self.0.get(&player_id).copied()
    }
}

#[derive(Resource, Clone, Default)]
pub struct PlayerUnitCosts(pub HashMap<AwbwUnitId, u32>);

impl PlayerUnitCosts {
    pub fn set(&mut self, unit_id: AwbwUnitId, cost: u32) {
        self.0.insert(unit_id, cost);
    }

    pub fn get(&self, unit_id: AwbwUnitId) -> Option<u32> {
        self.0.get(&unit_id).copied()
    }
}

pub fn player_roster_seed_from_replay(
    replay: &AwbwReplay,
) -> Option<(PlayerRosterConfig, PlayerFunds, PlayerUnitCosts)> {
    let first_game = replay.games.first()?;
    let mut players = first_game.players.clone();
    players.sort_by_key(|player| player.order);

    let config_players = players
        .iter()
        .map(|player| player_roster_player(player, first_game.team))
        .collect::<Vec<_>>();
    let funds = PlayerFunds(
        players
            .iter()
            .map(|player| (player.id, player.funds))
            .collect::<HashMap<_, _>>(),
    );
    let unit_costs = PlayerUnitCosts(
        first_game
            .units
            .iter()
            .map(|unit| (AwbwUnitId(unit.id), unit.cost))
            .collect::<HashMap<_, _>>(),
    );

    Some((
        PlayerRosterConfig {
            match_id: first_game.id.as_u32(),
            map_id: first_game.maps_id.as_u32(),
            funds_per_property: first_game.funds,
            players: config_players,
        },
        funds,
        unit_costs,
    ))
}

/// Seed the roster for a live match from its bootstrap observation.
///
/// A live match has no AWBW replay record to read, so identity comes from the
/// bootstrap player list and everything else from the observation. `match_id`
/// and `map_id` are not part of the live payload and stay zero.
pub fn player_roster_seed_from_live_match(
    players: &[LiveMatchPlayer],
    observation: &Observation,
) -> (
    PlayerRosterConfig,
    PlayerFunds,
    PlayerUnitCosts,
    PlayerPowerMeters,
) {
    let teams = observation
        .players
        .iter()
        .map(observed_player_team)
        .collect::<std::collections::HashSet<_>>();
    let team_game = teams.len() > 1 && teams.len() < observation.players.len();

    let mut config_players = Vec::with_capacity(players.len());
    let mut funds = PlayerFunds::default();
    let mut power_meters = PlayerPowerMeters::default();

    for (index, player) in players.iter().enumerate() {
        let Some(faction) = PlayerFaction::from_id(player.faction_id) else {
            continue;
        };
        let player_id = awbrn_types::AwbwGamePlayerId::new(player.player_id);
        let observed = observation.players.iter().find(|candidate| {
            observed_player_id(candidate).as_str() == player.player_id.to_string()
        });

        let commanders = observed.map(observed_player_commanders).unwrap_or_default();
        let co = commanders
            .first()
            .and_then(|kind| co_portrait_for_commander(*kind));
        let tag_co = commanders
            .get(1)
            .and_then(|kind| co_portrait_for_commander(*kind));

        if let Some(observed) = observed {
            if let ObservedPlayer::Private { funds: value, .. } = observed {
                funds.set(player_id, u32::try_from(*value).unwrap_or(u32::MAX));
            }
            if let Some(meter) = active_power_meter(observed) {
                power_meters.set(player_id, meter);
            }
        }

        config_players.push(PlayerRosterPlayer {
            player_id,
            // A live match carries no AWBW user account, and turn order follows
            // the bootstrap player list.
            user_id: awbrn_types::AwbwPlayerId::new(0),
            turn_order: index as u32,
            team: team_game
                .then(|| observed.map(|observed| observed_player_team(observed).to_string()))
                .flatten(),
            eliminated: observed.is_some_and(|observed| {
                observed_player_status(observed) != awvm::semantic::PlayerStatus::Active
            }),
            faction,
            faction_code: faction.country_code().to_string(),
            faction_name: faction.name().to_string(),
            co_key: co.map(|portrait| portrait.key().to_string()),
            co_name: co.map(|portrait| portrait.display_name().to_string()),
            tag_co_key: tag_co.map(|portrait| portrait.key().to_string()),
            tag_co_name: tag_co.map(|portrait| portrait.display_name().to_string()),
        });
    }

    (
        PlayerRosterConfig {
            match_id: 0,
            map_id: 0,
            funds_per_property: u32::try_from(observation.settings.income_per_property)
                .unwrap_or_default(),
            players: config_players,
        },
        funds,
        // Live builds have no recorded purchase price; `player_roster_snapshot`
        // falls back to the ruleset base cost for any unit missing here.
        PlayerUnitCosts::default(),
        power_meters,
    )
}

fn observed_player_id(player: &ObservedPlayer) -> &awvm::semantic::PlayerId {
    match player {
        ObservedPlayer::Private { id, .. } | ObservedPlayer::Public { id, .. } => id,
    }
}

fn observed_player_team(player: &ObservedPlayer) -> &str {
    match player {
        ObservedPlayer::Private { team, .. } | ObservedPlayer::Public { team, .. } => team.as_str(),
    }
}

fn observed_player_status(player: &ObservedPlayer) -> awvm::semantic::PlayerStatus {
    match player {
        ObservedPlayer::Private { status, .. } | ObservedPlayer::Public { status, .. } => *status,
    }
}

fn observed_player_commanders(player: &ObservedPlayer) -> Vec<awvm::semantic::CommanderId> {
    match player {
        ObservedPlayer::Private { commanders, .. } => {
            commanders.iter().map(|commander| commander.id).collect()
        }
        ObservedPlayer::Public { commanders, .. } => {
            commanders.iter().map(|commander| commander.id).collect()
        }
    }
}

/// Meter values for the commander currently leading, falling back to the first.
pub fn active_power_meter(player: &ObservedPlayer) -> Option<PlayerPowerMeter> {
    let (commander, charge, power_uses, power_state) = match player {
        ObservedPlayer::Private {
            commanders,
            power_state,
            ..
        } => commanders
            .iter()
            .find(|commander| commander.active)
            .or_else(|| commanders.first())
            .map(|commander| {
                (
                    commander.id,
                    commander.power_charge,
                    commander.power_uses,
                    power_state,
                )
            }),
        ObservedPlayer::Public {
            commanders,
            power_state,
            ..
        } => commanders
            .iter()
            .find(|commander| commander.active)
            .or_else(|| commanders.first())
            .map(|commander| {
                (
                    commander.id,
                    commander.power_charge,
                    commander.power_uses,
                    power_state,
                )
            }),
    }?;
    let cost = |level| {
        awvm::commander::power_activation_cost(commander, level, power_uses)
            .ok()
            .flatten()
            .and_then(|cost| u32::try_from(cost).ok())
    };
    Some(PlayerPowerMeter {
        charge: u32::try_from(charge).ok()?,
        cop_cost: cost(awvm::commander::PowerLevel::Cop),
        scop_cost: cost(awvm::commander::PowerLevel::Scop),
        star_charge: awvm::commander::power_star_charge(power_uses)
            .ok()
            .and_then(|charge| u32::try_from(charge).ok()),
        active_power: match power_state {
            awvm::semantic::PowerState::None => None,
            awvm::semantic::PowerState::Cop { .. } => Some(awvm::commander::PowerLevel::Cop),
            awvm::semantic::PowerState::Scop { .. } => Some(awvm::commander::PowerLevel::Scop),
        },
    })
}

/// Collect every public power meter from one canonical observation.
pub fn power_meters_from_observation(observation: &Observation) -> PlayerPowerMeters {
    PlayerPowerMeters(
        observation
            .players
            .iter()
            .filter_map(|player| {
                let player_id = observed_player_id(player).as_str().parse().ok()?;
                Some((
                    awbrn_types::AwbwGamePlayerId::new(player_id),
                    active_power_meter(player)?,
                ))
            })
            .collect(),
    )
}

/// AWVM names commanders by the same kebab key the portrait table uses, except
/// for the no-CO placeholder.
fn co_portrait_for_commander(kind: awvm::semantic::CommanderId) -> Option<CoPortraitMetadata> {
    let key = match kind {
        awvm::semantic::CommanderId::Neutral => "no-co",
        other => other.as_str(),
    };
    co_portraits()
        .iter()
        .copied()
        .find(|portrait| portrait.key() == key)
}

fn player_roster_player(player: &AwbwPlayer, team_game: bool) -> PlayerRosterPlayer {
    let co = co_portrait_by_awbw_id(player.co_id);
    let tag_co = player.tags_co_id.and_then(co_portrait_by_awbw_id);

    PlayerRosterPlayer {
        player_id: player.id,
        user_id: player.users_id,
        turn_order: player.order,
        team: team_game.then(|| player.team.clone()),
        eliminated: player.eliminated,
        faction: player.faction,
        faction_code: player.faction.country_code().to_string(),
        faction_name: player.faction.name().to_string(),
        co_key: co.map(|portrait| portrait.key().to_string()),
        co_name: co.map(|portrait| portrait.display_name().to_string()),
        tag_co_key: tag_co.map(|portrait| portrait.key().to_string()),
        tag_co_name: tag_co.map(|portrait| portrait.display_name().to_string()),
    }
}

pub fn player_roster_snapshot(world: &mut World) -> Option<PlayerRosterSnapshot> {
    let config = world.get_resource::<PlayerRosterConfig>()?.clone();
    let funds = world.get_resource::<PlayerFunds>()?.clone();
    let unit_costs = world.get_resource::<PlayerUnitCosts>()?.clone();
    let power_meters = world
        .get_resource::<PlayerPowerMeters>()
        .cloned()
        .unwrap_or_default();
    let replay_state = *world.get_resource::<ReplayState>()?;
    let display_faction_overrides = world
        .get_resource::<PlayerDisplayFactionOverrides>()
        .cloned();
    // Whose funds and roster are disclosed is the projection's own rule: a
    // teammate is reported privately and an opponent publicly.
    let disclosed = config
        .players
        .iter()
        .filter(|player| {
            world
                .get_resource::<ViewerVisibility>()
                .is_none_or(|visibility| visibility.player_disclosed(player.player_id))
        })
        .map(|player| player.player_id)
        .collect::<HashSet<_>>();
    let mut unit_counts = config
        .players
        .iter()
        .map(|player| (player.player_id, 0_u32))
        .collect::<HashMap<_, _>>();
    let mut unit_values = config
        .players
        .iter()
        .map(|player| (player.player_id, 0_u32))
        .collect::<HashMap<_, _>>();
    let mut income_counts = config
        .players
        .iter()
        .map(|player| (player.player_id, 0_u32))
        .collect::<HashMap<_, _>>();

    {
        let mut unit_query = world.query::<(&AwbwUnitId, &Faction, Option<&GraphicalHp>, &Unit)>();
        for (unit_id, faction, hp, unit) in unit_query.iter(world) {
            let Some(player_id) = player_id_for_faction(&config, faction.0) else {
                continue;
            };
            *unit_counts.entry(player_id).or_default() += 1;
            let hp_value = u32::from(hp.map(|value| value.value()).unwrap_or(10));
            let unit_cost = unit_costs
                .get(*unit_id)
                .unwrap_or_else(|| unit.0.base_cost());
            *unit_values.entry(player_id).or_default() += unit_cost.saturating_mul(hp_value) / 10;
        }
    }

    {
        let mut terrain_query = world.query::<&TerrainTile>();
        for terrain_tile in terrain_query.iter(world) {
            let GraphicalTerrain::Property(property) = terrain_tile.terrain else {
                continue;
            };
            let TerrainFaction::Player(faction) = property.faction() else {
                continue;
            };
            if let Some(player_id) = player_id_for_faction(&config, faction) {
                *income_counts.entry(player_id).or_default() += 1;
            }
        }
    }

    Some(PlayerRosterSnapshot {
        match_id: config.match_id,
        map_id: config.map_id,
        day: replay_state.day,
        active_player_id: replay_state.active_player_id.map(|id| id.as_u32()),
        players: config
            .players
            .iter()
            .map(|player| {
                let stats = if !disclosed.contains(&player.player_id) {
                    PlayerRosterStats {
                        funds: None,
                        unit_count: None,
                        unit_value: None,
                        income: None,
                    }
                } else {
                    PlayerRosterStats {
                        funds: Some(funds.get(player.player_id)),
                        unit_count: Some(
                            unit_counts
                                .get(&player.player_id)
                                .copied()
                                .unwrap_or_default(),
                        ),
                        unit_value: Some(
                            unit_values
                                .get(&player.player_id)
                                .copied()
                                .unwrap_or_default(),
                        ),
                        income: Some(
                            income_counts
                                .get(&player.player_id)
                                .copied()
                                .unwrap_or_default()
                                .saturating_mul(config.funds_per_property),
                        ),
                    }
                };
                let actual_faction_code = player.faction_code.clone();
                let actual_faction_name = player.faction_name.clone();
                let display_faction = display_faction_for_player(
                    display_faction_overrides.as_ref(),
                    player.player_id,
                    player.faction,
                );
                let display_faction_code = display_faction.country_code().to_string();
                let display_faction_name = display_faction.name().to_string();
                let power_meter = power_meters.get(player.player_id);
                PlayerRosterEntry {
                    player_id: player.player_id.as_u32(),
                    user_id: player.user_id.as_u32(),
                    turn_order: player.turn_order,
                    team: player.team.clone(),
                    eliminated: player.eliminated,
                    actual_faction_code,
                    actual_faction_name,
                    display_faction_code: display_faction_code.clone(),
                    display_faction_name: display_faction_name.clone(),
                    faction_code: display_faction_code,
                    faction_name: display_faction_name,
                    co_key: player.co_key.clone(),
                    co_name: player.co_name.clone(),
                    tag_co_key: player.tag_co_key.clone(),
                    tag_co_name: player.tag_co_name.clone(),
                    // Power meter values stay outside `stats` because AWVM
                    // projects their inputs to every player, fog or not.
                    power_charge: power_meter.map(|meter| meter.charge),
                    cop_cost: power_meter.and_then(|meter| meter.cop_cost),
                    scop_cost: power_meter.and_then(|meter| meter.scop_cost),
                    power_star_charge: power_meter.and_then(|meter| meter.star_charge),
                    active_power: power_meter.and_then(|meter| meter.active_power),
                    stats,
                }
            })
            .collect(),
    })
}

pub fn emit_player_roster_updated(world: &mut World) {
    let Some(snapshot) = player_roster_snapshot(world) else {
        return;
    };
    let Some(sink) = world.get_resource::<EventSink<PlayerRosterSnapshot>>() else {
        return;
    };
    sink.emit(snapshot);
}

pub fn player_ids_for_team(
    config: &PlayerRosterConfig,
    team: u8,
) -> impl Iterator<Item = awbrn_types::AwbwGamePlayerId> + '_ {
    // AWBW encodes teams as single ASCII letters ("A", "B", ...), so the
    // replay-side `team: u8` can be compared against the first team-name byte.
    config.players.iter().filter_map(move |player| {
        player
            .team
            .as_deref()
            .and_then(|team_name| team_name.as_bytes().first().copied())
            .filter(|team_id| *team_id == team)
            .map(|_| player.player_id)
    })
}

pub fn player_id_for_faction(
    config: &PlayerRosterConfig,
    faction: PlayerFaction,
) -> Option<awbrn_types::AwbwGamePlayerId> {
    config
        .players
        .iter()
        .find(|player| player.faction == faction)
        .map(|player| player.player_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use awbrn_game::GameWorldPlugin;
    use awbrn_game::world::{GraphicalHp, ViewerVisibility};
    use awbrn_types::{AwbwGamePlayerId, AwbwPlayerId};
    use bevy::app::App;
    use std::collections::HashMap;

    #[test]
    fn active_power_meter_reports_active_power() {
        let player: ObservedPlayer = serde_json::from_value(serde_json::json!({
            "id": "0",
            "team": "red-team",
            "relation": "self",
            "funds": 0,
            "status": "active",
            "commanders": [{
                "id": "andy",
                "active": true,
                "power_charge": 0,
                "power_uses": 1
            }],
            "power_state": { "type": "cop", "commander_slot": 0 }
        }))
        .unwrap();

        assert_eq!(
            active_power_meter(&player).unwrap().active_power,
            Some(awvm::commander::PowerLevel::Cop)
        );
    }

    #[test]
    fn fog_hides_opponent_roster_stats() {
        let mut app = App::new();
        app.add_plugins(GameWorldPlugin);
        app.world_mut().insert_resource(PlayerRosterConfig {
            match_id: 1,
            map_id: 2,
            funds_per_property: 1000,
            players: vec![
                PlayerRosterPlayer {
                    player_id: AwbwGamePlayerId::new(1),
                    user_id: AwbwPlayerId::new(10),
                    turn_order: 1,
                    team: None,
                    eliminated: false,
                    faction: PlayerFaction::OrangeStar,
                    faction_code: "os".to_string(),
                    faction_name: "Orange Star".to_string(),
                    co_key: None,
                    co_name: None,
                    tag_co_key: None,
                    tag_co_name: None,
                },
                PlayerRosterPlayer {
                    player_id: AwbwGamePlayerId::new(2),
                    user_id: AwbwPlayerId::new(20),
                    turn_order: 2,
                    team: None,
                    eliminated: false,
                    faction: PlayerFaction::BlueMoon,
                    faction_code: "bm".to_string(),
                    faction_name: "Blue Moon".to_string(),
                    co_key: None,
                    co_name: None,
                    tag_co_key: None,
                    tag_co_name: None,
                },
            ],
        });
        app.world_mut().insert_resource(PlayerFunds(HashMap::from([
            (AwbwGamePlayerId::new(1), 5000),
            (AwbwGamePlayerId::new(2), 6000),
        ])));
        app.world_mut()
            .insert_resource(PlayerUnitCosts(HashMap::from([
                (AwbwUnitId(awbrn_types::AwbwUnitId::new(1)), 7000),
                (AwbwUnitId(awbrn_types::AwbwUnitId::new(2)), 7000),
            ])));
        app.world_mut().insert_resource(ReplayState {
            next_action_index: 0,
            day: 1,
            active_player_id: Some(AwbwGamePlayerId::new(1)),
        });
        // Orange Star's own projection reports Orange Star privately and
        // Blue Moon publicly.
        {
            let mut visibility = app.world_mut().resource_mut::<ViewerVisibility>();
            visibility.reset(true, 1, 1);
            visibility.set_player_disclosed(AwbwGamePlayerId::new(1));
        }

        app.world_mut().spawn((
            Faction(PlayerFaction::OrangeStar),
            Unit(awbrn_types::Unit::Tank),
            AwbwUnitId(awbrn_types::AwbwUnitId::new(1)),
            GraphicalHp(10),
        ));
        app.world_mut().spawn((
            Faction(PlayerFaction::BlueMoon),
            Unit(awbrn_types::Unit::Tank),
            AwbwUnitId(awbrn_types::AwbwUnitId::new(2)),
            GraphicalHp(10),
        ));

        let snapshot = player_roster_snapshot(app.world_mut()).unwrap();
        assert_eq!(snapshot.players[0].stats.funds, Some(5000));
        assert_eq!(snapshot.players[0].stats.unit_count, Some(1));
        assert_eq!(snapshot.players[1].stats.funds, None);
        assert_eq!(snapshot.players[1].stats.unit_count, None);
        assert_eq!(snapshot.players[1].stats.unit_value, None);
        assert_eq!(snapshot.players[1].stats.income, None);
    }
}
