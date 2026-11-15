use bench::benchmarks::{
    adaptive::gungraun_benches::adaptive_benches, ai::gungraun_benches::ai_benches,
    ai::late_game::gungraun_benches::late_game_benches, awvm::gungraun_benches::awvm_benches,
    replay::gungraun_benches::replay_benches, server::gungraun_benches::server_benches,
    stratified::gungraun_benches::stratified_benches,
};
use gungraun::main;

main!(
    library_benchmark_groups = [
        adaptive_benches,
        ai_benches,
        awvm_benches,
        late_game_benches,
        replay_benches,
        server_benches,
        stratified_benches
    ]
);
