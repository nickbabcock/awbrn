use bench::benchmarks::{
    awvm::gungraun_benches::awvm_benches, map::gungraun_benches::map_benches,
    replay::gungraun_benches::replay_benches,
};
use gungraun::main;

main!(library_benchmark_groups = [awvm_benches, map_benches, replay_benches]);
