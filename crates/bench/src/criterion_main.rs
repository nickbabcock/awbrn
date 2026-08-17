use bench::benchmarks::{awvm, replay, server};
use criterion::criterion_main;

criterion_main!(
    awvm::criterion_benches::awvm_benches,
    replay::criterion_benches::replay_benches,
    server::criterion_benches::server_benches
);
