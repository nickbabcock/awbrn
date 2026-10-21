use awvm::semantic::{
    Board, Location, Match, Roster, RulesetRef, Settings, State, Team, Turn, Weather,
};

use crate::agent::Play;

#[derive(PartialEq)]
struct Context<'a> {
    ruleset: &'a RulesetRef,
    settings: &'a Settings,
    board: &'a Board,
    teams: &'a [Team],
    players: &'a Roster,
    turn: &'a Turn,
    weather: &'a Weather,
    can_produce: bool,
    match_state: &'a Match,
}

fn context(state: &State) -> Context<'_> {
    let State {
        ruleset,
        settings,
        board,
        teams,
        players,
        turn,
        weather,
        units: _units,
        next_unit_id,
        match_state,
    } = state;
    Context {
        ruleset,
        settings,
        board,
        teams,
        players,
        turn,
        weather,
        can_produce: next_unit_id.is_some(),
        match_state,
    }
}

fn can_rename(state: &State) -> bool {
    if state.settings.fog
        || state.players.len() != 2
        || state
            .units
            .iter()
            .any(|unit| unit.owner.get() >= 2 || matches!(unit.location, Location::Cargo { .. }))
    {
        return false;
    }
    let mut highest = 0;
    for (owner, _) in state.players.seats() {
        let mut previous = None;
        for unit in state.units.iter().filter(|unit| unit.owner == owner) {
            if previous.is_some_and(|id| id >= unit.id.get()) {
                return false;
            }
            previous = Some(unit.id.get());
            highest = highest.max(unit.id.get());
        }
    }
    // A power can create up to one unit per board cell.
    !state
        .next_unit_id
        .is_some_and(|next| next <= highest || next > u32::MAX - 65_025)
}

/// Keep each owner's unit order for target and turn processing.
/// Unit names and storage order between owners can change after projection.
pub(super) fn matches(expected: &State, current: &State) -> bool {
    if expected == current {
        return true;
    }
    if !can_rename(expected)
        || !can_rename(current)
        || context(expected) != context(current)
        || expected.units.len() != current.units.len()
    {
        return false;
    }
    for (owner, _) in expected.players.seats() {
        let mut current = current.units.iter().filter(|unit| unit.owner == owner);
        for unit in expected.units.iter().filter(|unit| unit.owner == owner) {
            let Some(other) = current.next() else {
                return false;
            };
            let mut renamed = *unit;
            renamed.id = other.id;
            if renamed != *other {
                return false;
            }
        }
        if current.next().is_some() {
            return false;
        }
    }
    true
}

/// Translate a play between positions that `matches` accepted.
pub(super) fn remap(expected: &State, current: &State, play: Play) -> Option<Play> {
    if expected == current || play.unit().is_none() {
        return Some(play);
    }
    let id = play.unit()?;
    let owner = expected.units.get(id)?.owner;
    let ordinal = expected
        .units
        .iter()
        .filter(|unit| unit.owner == owner)
        .position(|unit| unit.id == id)?;
    let unit = current
        .units
        .iter()
        .filter(|unit| unit.owner == owner)
        .nth(ordinal)?;
    Some(Play::new(unit.id, play.destination(), play.kind()))
}
