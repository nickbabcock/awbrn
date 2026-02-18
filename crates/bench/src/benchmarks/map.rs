use awbrn_map::{Position, TerrainCosts};

struct TestMovement;

impl TerrainCosts for TestMovement {
    fn cost(&self, _terrain: awbrn_core::MovementTerrain) -> Option<u8> {
        Some(1)
    }
}

fn sidewinder_fighter(
    pathfinder: &mut awbrn_map::PathFinder<impl awbrn_map::MovementMap>,
) -> usize {
    let reachable = pathfinder.reachable(Position::new(15, 15), 11, TestMovement);
    reachable.into_positions().count()
}

fn infantry(pathfinder: &mut awbrn_map::PathFinder<impl awbrn_map::MovementMap>) -> usize {
    let reachable = pathfinder.reachable(Position::new(15, 15), 3, TestMovement);
    reachable.into_positions().count()
}

pub mod criterion_benches {
    use super::*;
    use criterion::{BenchmarkId, Criterion};
    use std::hint::black_box;

    fn pathfinding(c: &mut Criterion) {
        let map = awbrn_map::AwbwMap::new(40, 40, awbrn_core::AwbwTerrain::Plain);
        let mut pathfinder = map.pathfinder();
        let mut group = c.benchmark_group("pathfinding");
        group.bench_function(BenchmarkId::from_parameter("sidewinder-fighter"), |b| {
            b.iter(|| black_box(sidewinder_fighter(&mut pathfinder)));
        });

        group.bench_function(BenchmarkId::from_parameter("infantry"), |b| {
            b.iter(|| black_box(infantry(&mut pathfinder)));
        });

        group.finish();
    }

    criterion::criterion_group!(map_benches, pathfinding);
}
