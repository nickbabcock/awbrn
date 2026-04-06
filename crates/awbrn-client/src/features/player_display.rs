use crate::features::player_roster::{
    PlayerRosterConfig, emit_player_roster_updated, player_id_for_faction,
};
use awbrn_types::{AwbwGamePlayerId, PlayerFaction};
use bevy::prelude::*;
use std::collections::HashMap;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetPlayerDisplayFaction {
    pub player_id: AwbwGamePlayerId,
    pub faction: Option<PlayerFaction>,
}

#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct PlayerDisplayFactionOverrides(pub HashMap<AwbwGamePlayerId, PlayerFaction>);

impl PlayerDisplayFactionOverrides {
    pub fn set(&mut self, player_id: AwbwGamePlayerId, faction: Option<PlayerFaction>) -> bool {
        match faction {
            Some(faction) => self.0.insert(player_id, faction) != Some(faction),
            None => self.0.remove(&player_id).is_some(),
        }
    }

    pub fn display_faction_for_player(
        &self,
        player_id: AwbwGamePlayerId,
        actual_faction: PlayerFaction,
    ) -> PlayerFaction {
        self.0.get(&player_id).copied().unwrap_or(actual_faction)
    }
}

pub fn display_faction_for_player(
    overrides: Option<&PlayerDisplayFactionOverrides>,
    player_id: AwbwGamePlayerId,
    actual_faction: PlayerFaction,
) -> PlayerFaction {
    overrides
        .map(|overrides| overrides.display_faction_for_player(player_id, actual_faction))
        .unwrap_or(actual_faction)
}

pub fn display_faction_for_actual_faction(
    config: Option<&PlayerRosterConfig>,
    overrides: Option<&PlayerDisplayFactionOverrides>,
    actual_faction: PlayerFaction,
) -> PlayerFaction {
    let Some(config) = config else {
        return actual_faction;
    };

    let Some(player_id) = player_id_for_faction(config, actual_faction) else {
        return actual_faction;
    };

    display_faction_for_player(overrides, player_id, actual_faction)
}

fn apply_player_display_faction_commands(
    mut messages: MessageReader<SetPlayerDisplayFaction>,
    mut overrides: ResMut<PlayerDisplayFactionOverrides>,
) {
    for command in messages.read().copied() {
        overrides.set(command.player_id, command.faction);
    }
}

pub struct PlayerDisplayPlugin;

impl Plugin for PlayerDisplayPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SetPlayerDisplayFaction>()
            .init_resource::<PlayerDisplayFactionOverrides>()
            .add_systems(
                Update,
                (
                    apply_player_display_faction_commands,
                    emit_player_roster_updated
                        .after(apply_player_display_faction_commands)
                        .run_if(
                            resource_exists::<PlayerRosterConfig>
                                .and(resource_changed::<PlayerDisplayFactionOverrides>),
                        ),
                ),
            );
    }
}
