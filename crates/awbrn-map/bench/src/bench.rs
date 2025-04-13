use awbrn_map::{Position, TerrainCosts};
use criterion::{black_box, BenchmarkId, Criterion};

struct TestMovement;

impl TerrainCosts for TestMovement {
    fn cost(&self, _terrain: awbrn_core::MovementTerrain) -> Option<u8> {
        Some(1)
    }
}

fn parser(c: &mut Criterion) {
    let map = awbrn_map::AwbwMap::new(40, 40, awbrn_core::Terrain::Plain);
    let mut pathfinder = map.pathfinder();
    let mut group = c.benchmark_group("pathfinding");
    group.bench_function(BenchmarkId::from_parameter("sidewinder-fighter"), |b| {
        b.iter(|| {
            let reachable = pathfinder.reachable(Position::new(15, 15), 11, TestMovement);
            let count = reachable.into_positions().count();
            black_box(count);
        });
    });

    group.bench_function(BenchmarkId::from_parameter("infantry"), |b| {
        b.iter(|| {
            let reachable = pathfinder.reachable(Position::new(15, 15), 3, TestMovement);
            let count = reachable.into_positions().count();
            black_box(count);        });
    });

    group.finish();
}

criterion::criterion_group!(benches, parser);
criterion::criterion_main!(benches);
