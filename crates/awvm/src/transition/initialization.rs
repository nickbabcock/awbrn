use crate::random::{RandomError, RandomToken};
use crate::ruleset;
use crate::semantic::{
    Commander, Concealment, Location, Match, Phase, Player, PlayerStatus, PowerState, Roster,
    State, Team, TeamStatus, Turn, Unit, UnitAction, UnitId, UnitStore, Weather, WeatherKind,
};
use crate::setup::MatchSetup;

use super::{ExecuteError, Execution, InvalidStateError, begin_match};

/// A fault that prevented a setup from becoming a match.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum InitializeError {
    #[error("only awbw/2026-07-10 is implemented")]
    UnsupportedRuleset,
    #[error("invalid match setup: {0}")]
    InvalidSetup(InvalidStateError),
    #[error("invalid random input: {0}")]
    InvalidRandom(#[from] RandomError),
}

/// Initialize a match exactly once from its declarative setup.
///
/// This consumes the setup, assigns all lifecycle state, and runs the first
/// player's turn-start hooks. The result is actionable or finished; only
/// commands can advance it afterwards.
pub fn initialize_match(
    setup: MatchSetup,
    random: &[RandomToken],
) -> Result<Execution, InitializeError> {
    let MatchSetup {
        ruleset: setup_ruleset,
        settings,
        board,
        players: setup_players,
        deployments,
    } = setup;
    let players = Roster::new(
        setup_players
            .iter()
            .map(|player| {
                Player::new(player.id.clone(), player.team.clone())
                    .with_funds(player.starting_funds)
                    .with_status(PlayerStatus::Active)
                    .with_commanders(
                        player
                            .commanders
                            .iter()
                            .enumerate()
                            .map(|(index, id)| Commander {
                                id: *id,
                                active: index == 0,
                                power_charge: 0,
                                power_uses: 0,
                            })
                            .collect(),
                    )
                    .with_power_state(PowerState::None)
            })
            .collect(),
    )
    .map_err(|error| InitializeError::InvalidSetup(error.to_string().into()))?;
    let teams = setup_players.iter().fold(Vec::new(), |mut teams, player| {
        if !teams.iter().any(|team: &Team| team.id == player.team) {
            teams.push(Team {
                id: player.team.clone(),
                status: TeamStatus::Active,
            });
        }
        teams
    });
    let mut units = Vec::with_capacity(deployments.len());
    for (index, deployment) in deployments.iter().enumerate() {
        let owner = players.seat(&deployment.owner).ok_or_else(|| {
            InitializeError::InvalidSetup(
                format!("deployment owner {} is not on the roster", deployment.owner).into(),
            )
        })?;
        let id = u32::try_from(index + 1)
            .map(UnitId::new)
            .map_err(|_| InitializeError::InvalidSetup("too many deployed units".into()))?;
        let profile = ruleset::profile(deployment.kind);
        units.push(Unit {
            id,
            kind: deployment.kind,
            owner,
            hp: deployment.hp,
            fuel: profile.max_fuel,
            ammo: profile.max_ammo,
            action: UnitAction::Spent,
            concealment: Concealment::Exposed,
            location: Location::Board {
                position: deployment.position,
            },
        });
    }
    let next_unit_id = u32::try_from(units.len() + 1)
        .map_err(|_| InitializeError::InvalidSetup("too many deployed units".into()))?;
    let active_player = players[0].id().clone();
    let state = State {
        ruleset: setup_ruleset,
        settings,
        board,
        teams,
        players,
        turn: Turn {
            day: 1,
            active_player,
            phase: Phase::TurnStart,
            order: setup_players
                .iter()
                .map(|player| player.id.clone())
                .collect(),
            position: 0,
        },
        weather: Weather {
            kind: WeatherKind::Clear,
            remaining_turns: 0,
        },
        units: UnitStore::new(units)
            .map_err(|error| InitializeError::InvalidSetup(error.to_string().into()))?,
        next_unit_id: Some(next_unit_id),
        match_state: Match::Active {
            draw_offers: Vec::new(),
        },
    };
    begin_match(&state, random).map_err(|error| match error {
        ExecuteError::UnsupportedRuleset => InitializeError::UnsupportedRuleset,
        ExecuteError::InvalidState(error) => InitializeError::InvalidSetup(error),
        ExecuteError::InvalidRandom(error) => InitializeError::InvalidRandom(error),
        ExecuteError::UnsupportedCommand => {
            InitializeError::InvalidSetup("the ruleset cannot run its match-opening hooks".into())
        }
    })
}
