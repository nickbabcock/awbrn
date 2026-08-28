use bench::benchmarks::{adaptive, ai, awvm, replay, server, stratified};
use criterion::criterion_main;

criterion_main!(
    ai::criterion_benches::ai_benches,
    adaptive::criterion_benches::adaptive_benches,
    awvm::criterion_benches::awvm_benches,
    replay::criterion_benches::replay_benches,
    server::criterion_benches::server_benches,
    stratified::criterion_benches::stratified_benches
);
