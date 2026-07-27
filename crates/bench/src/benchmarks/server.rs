//! `awbrn-server`'s command and view paths, at the scale a real match runs at.
//!
//! These exist because of phase 5.1: `validate.rs`, `apply.rs` and `damage.rs`
//! are about to be replaced by `awvm`, and until now nothing in this crate
//! measured them. The awvm side of that swap has been benchmarked since phase
//! 0f; the incumbent had not been, which is the wrong way round for a
//! no-regression gate.
//!
//! Two things drive the shape of the cases.
//!
//! **Scale.** Fixtures are two to four units, and both halves of this swap have
//! costs that only appear at real size: `submit_command` recomputes fog once
//! per player before the command and again after (`view::snapshot_pre_fog` and
//! `view::build_command_result`), and `awvm::transition::execute` clones the
//! whole state where `apply::apply_command` mutates a `World` in place. AWBW
//! maps run to roughly 20x20 with 20-50 units, so that is what these use.
//!
//! **Fog on and off.** With fog off `snapshot_pre_fog` returns immediately, so
//! the fog rebuilds — the part most likely to move — are invisible. Every
//! command case is measured both ways so the difference is attributable.
//!
//! `submit_command` mutates the server, so a command case cannot be iterated
//! against one world: the unit has moved and is no longer active by the second
//! iteration, and what would be measured is the rejection path. Every command
//! case therefore rebuilds its server in setup, which both harnesses exclude
//! from the measurement.

use awbrn_map::{AwbrnMap, AwbwMap, Position};
use awbrn_server::{GameCommand, GameServer, GameSetup, PlayerId, PlayerSetup, PostMoveAction};
use awbrn_types::{AwbwTerrain, Co, Faction, PlayerFaction, Property, Unit};

/// The board every case runs on. AWBW's own maps cluster around this size.
const WIDTH: usize = 20;
const HEIGHT: usize = 20;

/// Factions in seating order. Cases take a prefix of this.
///
/// Player count is a parameter rather than a constant because it is the thing
/// fog cost is predicted to scale on: `snapshot_pre_fog` loops the roster
/// calling `compute_fog_for_factions`, and `build_command_result` does it
/// again, so a six-player match should pay three times what a two-player one
/// does. That prediction is worth measuring rather than asserting.
const FACTIONS: [PlayerFaction; 6] = [
    PlayerFaction::OrangeStar,
    PlayerFaction::BlueMoon,
    PlayerFaction::GreenEarth,
    PlayerFaction::YellowComet,
    PlayerFaction::BlackHole,
    PlayerFaction::RedFire,
];

/// The two rosters cases use: the common duel, and the six-player match that
/// makes the per-player fog work visible.
pub const DUEL: usize = 2;
pub const SIX_PLAYER: usize = 6;

/// Total units on the board, held fixed across player counts so the fog
/// comparison isolates the roster loop rather than also changing the number of
/// sighting units.
const UNITS: usize = 40;

/// A plains board with an HQ, bases and cities for each player.
///
/// Properties matter beyond decoration: they are what `apply_end_turn` collects
/// income over, and they are the tiles a fog rebuild treats differently from
/// open ground.
fn map(players: usize) -> AwbrnMap {
    let mut awbw = AwbwMap::new(WIDTH, HEIGHT, AwbwTerrain::Plain);

    let mut put = |x: usize, y: usize, terrain: AwbwTerrain| {
        if let Some(tile) = awbw.terrain_at_mut(Position::new(x, y)) {
            *tile = terrain;
        }
    };

    for (seat, faction) in FACTIONS.iter().take(players).enumerate() {
        // Seats alternate between the top and bottom edges, working inward, so
        // six of them fit without overlapping.
        let band = seat / 2;
        let (row, inner) = if seat % 2 == 0 {
            (band * 2, band * 2 + 1)
        } else {
            (HEIGHT - 1 - band * 2, HEIGHT - 2 - band * 2)
        };

        put(
            WIDTH / 2,
            row,
            AwbwTerrain::Property(Property::HQ(*faction)),
        );
        for i in 0..3 {
            put(
                2 + i * 5,
                row,
                AwbwTerrain::Property(Property::Base(Faction::Player(*faction))),
            );
            put(
                3 + i * 5,
                inner,
                AwbwTerrain::Property(Property::City(Faction::Player(*faction))),
            );
        }
    }

    // Neutral cities across the middle, the ones a real match is fought over.
    for i in 0..6 {
        put(
            3 + i * 3,
            HEIGHT / 2,
            AwbwTerrain::Property(Property::City(Faction::Neutral)),
        );
    }

    AwbrnMap::from_map(&awbw)
}

/// A mix wide enough that the damage tables are actually consulted rather than
/// one row of them being reused.
const ROSTER: [Unit; 5] = [
    Unit::Infantry,
    Unit::Mech,
    Unit::Tank,
    Unit::Artillery,
    Unit::APC,
];

/// The tile the mover starts on, and the two it walks to. Kept clear of every
/// other unit so the move case is never blocked.
const MOVER_START: (usize, usize) = (0, 3);
const MOVER_PATH: [(usize, usize); 3] = [(0, 3), (0, 4), (0, 5)];
/// Adjacent to the mover's destination, so the attack case measures an attack
/// rather than a fourteen-row walk to find someone to shoot.
const ATTACK_TARGET: (usize, usize) = (1, 5);

/// Where the rest of the roster stands: rows 6 upward, clear of both edges'
/// properties and of the mover's lane.
fn deployment(players: usize) -> impl Iterator<Item = (Position, Unit, PlayerFaction)> {
    (0..UNITS - 1).map(move |i| {
        let position = Position::new(i % WIDTH, 6 + i / WIDTH);
        // Round-robin so every seat owns units — a seat with none sees nothing
        // and its fog rebuild would be unrepresentatively cheap.
        let faction = FACTIONS[(i + 1) % players];
        (position, ROSTER[i % ROSTER.len()], faction)
    })
}

/// A fresh server at game scale.
///
/// Rebuilt per iteration for the command cases, so it is setup rather than
/// measured work — see the module comment.
pub fn server(players: usize, fog: bool) -> GameServer {
    let setup = GameSetup {
        map: map(players),
        players: FACTIONS
            .iter()
            .take(players)
            .map(|faction| PlayerSetup {
                faction: *faction,
                team: None,
                starting_funds: 10_000,
                co: Co::Andy,
            })
            .collect(),
        fog_enabled: fog,
        rng_seed: 0x5eed,
    };

    let mut server = GameServer::new(setup).expect("game setup is valid");
    // The mover goes first so it is always unit 1, whatever the roster does.
    server.spawn_unit(
        Position::new(MOVER_START.0, MOVER_START.1),
        Unit::Tank,
        FACTIONS[0],
    );
    for (position, unit, faction) in deployment(players) {
        server.spawn_unit(position, unit, faction);
    }
    server
}

/// A server and the command to submit against it.
pub struct Submission {
    pub server: GameServer,
    pub player: PlayerId,
    pub command: GameCommand,
}

/// Which of the three command shapes a case measures.
#[derive(Clone, Copy)]
pub enum Kind {
    /// The cheapest accepted command: validate, move, rebuild fog twice.
    Move,
    /// The same plus combat, which is what `damage.rs` is on the hook for.
    Attack,
    /// Income, resupply, repair and reactivation over the whole board — the
    /// one command whose cost scales with the roster rather than one unit.
    EndTurn,
}

pub fn submission(kind: Kind, players: usize, fog: bool) -> Submission {
    let mut server = server(players, fog);
    let player = PlayerId(0);
    let path: Vec<Position> = MOVER_PATH
        .iter()
        .map(|(x, y)| Position::new(*x, *y))
        .collect();

    let command = match kind {
        Kind::Move => GameCommand::MoveUnit {
            unit_id: awbrn_server::ServerUnitId(1),
            path,
            action: Some(PostMoveAction::Wait),
        },
        Kind::Attack => {
            let target = Position::new(ATTACK_TARGET.0, ATTACK_TARGET.1);
            server.spawn_unit(target, Unit::Infantry, FACTIONS[1]);
            GameCommand::MoveUnit {
                unit_id: awbrn_server::ServerUnitId(1),
                path,
                action: Some(PostMoveAction::Attack { target }),
            }
        }
        Kind::EndTurn => GameCommand::EndTurn,
    };

    Submission {
        server,
        player,
        command,
    }
}

/// Submit the command, returning how many per-player updates came back so the
/// result cannot be optimized away.
pub fn run_submit(submission: &mut Submission) -> usize {
    let command = submission.command.clone();
    submission
        .server
        .submit_command(submission.player, command)
        .expect("benchmark command is accepted")
        .updates
        .len()
}

pub fn run_player_view(server: &mut GameServer) -> usize {
    server.player_view(PlayerId(0)).units.len()
}

pub fn run_spectator_view(server: &mut GameServer) -> usize {
    server.spectator_view().units.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every command case has to be *accepted*, or the benchmark measures the
    /// rejection path — which is cheap, plausible-looking, and wrong. The
    /// deployment is hand-placed, so this is the thing most likely to rot.
    #[test]
    fn every_command_case_is_accepted() {
        for kind in [Kind::Move, Kind::Attack, Kind::EndTurn] {
            for players in [DUEL, SIX_PLAYER] {
                for fog in [false, true] {
                    let mut submission = submission(kind, players, fog);
                    let command = submission.command.clone();
                    submission
                        .server
                        .submit_command(submission.player, command)
                        .expect("benchmark command is accepted");
                }
            }
        }
    }

    /// The roster has to be the size the module claims, or the "game scale"
    /// framing is wrong and the numbers do not compare to phase 4.6e's.
    #[test]
    fn the_board_carries_a_full_roster() {
        for players in [DUEL, SIX_PLAYER] {
            let mut server = server(players, false);
            assert_eq!(
                run_spectator_view(&mut server),
                UNITS,
                "roster is not {UNITS} units at {players} players"
            );
        }
    }

    /// The attack case has to actually fight. A `MoveUnit` with an `Attack`
    /// action that resolves to no combat would still be accepted and would
    /// quietly measure a plain move.
    #[test]
    fn the_attack_case_resolves_combat() {
        let mut submission = submission(Kind::Attack, DUEL, false);
        let command = submission.command.clone();
        let result = submission
            .server
            .submit_command(submission.player, command)
            .expect("attack is accepted");
        assert!(
            result.combat_outcome.is_some(),
            "the attack case did not resolve combat"
        );
    }

    /// Both views have to see something. An empty roster would make them
    /// trivially fast and the numbers meaningless.
    #[test]
    fn the_views_are_populated() {
        for fog in [false, true] {
            let mut server = server(DUEL, fog);
            assert!(
                run_spectator_view(&mut server) > 0,
                "spectator sees nothing"
            );
            assert!(run_player_view(&mut server) > 0, "player sees nothing");
        }
    }
}

pub mod criterion_benches {
    use super::*;
    use criterion::{BatchSize, BenchmarkId, Criterion};
    use std::hint::black_box;

    const KINDS: [(&str, Kind); 3] = [
        ("move", Kind::Move),
        ("attack", Kind::Attack),
        ("end-turn", Kind::EndTurn),
    ];

    fn submit(c: &mut Criterion) {
        let mut group = c.benchmark_group("server-submit-command");
        for (name, kind) in KINDS {
            for players in [DUEL, SIX_PLAYER] {
                for fog in [false, true] {
                    let id = format!("{name}-{players}p-fog-{}", if fog { "on" } else { "off" });
                    group.bench_function(BenchmarkId::from_parameter(id), |b| {
                        // Setup is excluded from the measurement, which is the
                        // only way to benchmark a mutating call: see the module
                        // comment.
                        b.iter_batched_ref(
                            || submission(kind, players, fog),
                            |s| black_box(run_submit(s)),
                            BatchSize::PerIteration,
                        );
                    });
                }
            }
        }
        group.finish();
    }

    fn views(c: &mut Criterion) {
        let mut group = c.benchmark_group("server-view");
        for players in [DUEL, SIX_PLAYER] {
            for fog in [false, true] {
                let suffix = if fog { "on" } else { "off" };
                // Views do not mutate game state, so one server serves every
                // iteration.
                let mut built = server(players, fog);
                group.bench_function(
                    BenchmarkId::from_parameter(format!("player-{players}p-fog-{suffix}")),
                    |b| {
                        b.iter(|| black_box(run_player_view(&mut built)));
                    },
                );
                group.bench_function(
                    BenchmarkId::from_parameter(format!("spectator-{players}p-fog-{suffix}")),
                    |b| {
                        b.iter(|| black_box(run_spectator_view(&mut built)));
                    },
                );
            }
        }
        group.finish();
    }

    criterion::criterion_group!(server_benches, submit, views);
}

#[cfg(not(target_family = "wasm"))]
pub mod gungraun_benches {
    use super::*;
    use gungraun::{library_benchmark, library_benchmark_group};

    fn built((kind, players, fog): (Kind, usize, bool)) -> Submission {
        submission(kind, players, fog)
    }

    fn built_server((players, fog): (usize, bool)) -> GameServer {
        server(players, fog)
    }

    #[library_benchmark(setup = built)]
    #[bench::move_duel_fog_off((Kind::Move, DUEL, false))]
    #[bench::move_duel_fog_on((Kind::Move, DUEL, true))]
    #[bench::move_six_fog_on((Kind::Move, SIX_PLAYER, true))]
    #[bench::attack_duel_fog_off((Kind::Attack, DUEL, false))]
    #[bench::attack_duel_fog_on((Kind::Attack, DUEL, true))]
    #[bench::attack_six_fog_on((Kind::Attack, SIX_PLAYER, true))]
    #[bench::end_turn_duel_fog_off((Kind::EndTurn, DUEL, false))]
    #[bench::end_turn_duel_fog_on((Kind::EndTurn, DUEL, true))]
    #[bench::end_turn_six_fog_on((Kind::EndTurn, SIX_PLAYER, true))]
    fn submit(mut submission: Submission) -> usize {
        run_submit(&mut submission)
    }

    #[library_benchmark(setup = built_server)]
    #[bench::duel_fog_off((DUEL, false))]
    #[bench::duel_fog_on((DUEL, true))]
    #[bench::six_fog_on((SIX_PLAYER, true))]
    fn player_view(mut server: GameServer) -> usize {
        run_player_view(&mut server)
    }

    #[library_benchmark(setup = built_server)]
    #[bench::duel_fog_off((DUEL, false))]
    #[bench::duel_fog_on((DUEL, true))]
    #[bench::six_fog_on((SIX_PLAYER, true))]
    fn spectator_view(mut server: GameServer) -> usize {
        run_spectator_view(&mut server)
    }

    library_benchmark_group!(
        name = server_benches,
        benchmarks = [submit, player_view, spectator_view,]
    );
}
