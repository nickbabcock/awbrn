use bench::benchmarks::{map, replay};
use criterion::criterion_main;

criterion_main!(
    map::criterion_benches::map_benches,
    replay::criterion_benches::replay_benches
);
