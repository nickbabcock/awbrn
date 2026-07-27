#[cfg(not(test))]
use bench::benchmarks::{
    awvm::gungraun_benches::awvm_benches, map::gungraun_benches::map_benches,
    replay::gungraun_benches::replay_benches, server::gungraun_benches::server_benches,
};

#[cfg(not(test))]
gungraun::main!(
    library_benchmark_groups = [awvm_benches, map_benches, replay_benches, server_benches]
);

#[cfg(test)]
fn main() {}
