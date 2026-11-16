//! The roster an AWBW archive opens with.

use std::collections::HashMap;

use awbrn_bevy::world::UnitId;
use awbrn_content::co_portrait_by_awbw_id;
use awbw_replay::AwbwReplay;
use awbw_replay::game_models::AwbwPlayer;

use crate::features::player_roster::{
    PlayerFunds, PlayerRosterConfig, PlayerRosterPlayer, PlayerUnitCosts,
};

pub(crate) fn player_roster_seed_from_replay(
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
            .map(|player| (awvm_awbw::player_id(player.id), player.funds))
            .collect::<HashMap<_, _>>(),
    );
    let unit_costs = PlayerUnitCosts(
        first_game
            .units
            .iter()
            .map(|unit| (UnitId(awvm_awbw::unit_id(unit.id)), unit.cost))
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
        player_id: awvm_awbw::player_id(player.id),
        user_id: Some(player.users_id),
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
