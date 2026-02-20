#[cfg(not(test))]
use bench::benchmarks::{
    map::gungraun_benches::map_benches, replay::gungraun_benches::replay_benches,
};

#[cfg(not(test))]
gungraun::main!(library_benchmark_groups = [map_benches, replay_benches]);

#[cfg(test)]
fn main() {}
