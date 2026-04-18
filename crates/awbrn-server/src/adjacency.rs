use awbrn_game::world::{BoardIndex, Faction};
use awbrn_map::Position;
use awbrn_types::PlayerFaction;
use bevy::prelude::*;

pub(crate) fn adjacent_self_owned_units(
    world: &World,
    position: Position,
    owner: PlayerFaction,
) -> impl Iterator<Item = Entity> + '_ {
    adjacent_positions(position)
        .into_iter()
        .flatten()
        .filter_map(|pos| {
            world
                .resource::<BoardIndex>()
                .unit_entity(pos)
                .ok()
                .flatten()
        })
        .filter(move |entity| {
            world
                .entity(*entity)
                .get::<Faction>()
                .is_some_and(|faction| faction.0 == owner)
        })
}

pub(crate) fn adjacent_positions(position: Position) -> [Option<Position>; 4] {
    [
        position
            .x
            .checked_sub(1)
            .map(|x| Position::new(x, position.y)),
        position
            .y
            .checked_sub(1)
            .map(|y| Position::new(position.x, y)),
        Some(Position::new(position.x + 1, position.y)),
        Some(Position::new(position.x, position.y + 1)),
    ]
}
