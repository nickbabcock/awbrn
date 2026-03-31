use crate::features::event_bus::{
    ExternalGameEvent, GameEvent, PlayerRosterEntry, PlayerRosterSnapshot, PlayerRosterStats,
};
use awbrn_content::co_portrait_by_awbw_id;
use awbrn_game::replay::{AwbwUnitId, ReplayState};
use awbrn_game::world::{Faction, FogActive, FriendlyFactions, GraphicalHp, TerrainTile, Unit};
use awbrn_types::{Faction as TerrainFaction, GraphicalTerrain, PlayerFaction};
use awbw_replay::AwbwReplay;
use awbw_replay::game_models::AwbwPlayer;
use bevy::prelude::*;
use std::collections::HashMap;

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
    let replay_state = *world.get_resource::<ReplayState>()?;
    let fog_active = world.get_resource::<FogActive>().is_some_and(|fog| fog.0);
    let friendly_factions = world
        .get_resource::<FriendlyFactions>()
        .map(|factions| factions.0.clone())
        .unwrap_or_default();
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
                let stats = if fog_active && !friendly_factions.contains(&player.faction) {
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
                PlayerRosterEntry {
                    player_id: player.player_id.as_u32(),
                    user_id: player.user_id.as_u32(),
                    turn_order: player.turn_order,
                    team: player.team.clone(),
                    eliminated: player.eliminated,
                    faction_code: player.faction_code.clone(),
                    faction_name: player.faction_name.clone(),
                    co_key: player.co_key.clone(),
                    co_name: player.co_name.clone(),
                    tag_co_key: player.tag_co_key.clone(),
                    tag_co_name: player.tag_co_name.clone(),
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

    world.write_message(ExternalGameEvent {
        payload: GameEvent::PlayerRosterUpdated(snapshot),
    });
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
    use awbrn_game::world::{FogActive, FriendlyFactions, GraphicalHp};
    use awbrn_types::{AwbwGamePlayerId, AwbwPlayerId};
    use bevy::app::App;
    use std::collections::{HashMap, HashSet};

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
        app.world_mut().insert_resource(FogActive(true));
        app.world_mut()
            .insert_resource(FriendlyFactions(HashSet::from([PlayerFaction::OrangeStar])));

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
