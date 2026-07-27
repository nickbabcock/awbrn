use bench::benchmarks::{awvm, map, replay, server};
use criterion::criterion_main;

criterion_main!(
    awvm::criterion_benches::awvm_benches,
    map::criterion_benches::map_benches,
    replay::criterion_benches::replay_benches,
    server::criterion_benches::server_benches
);
