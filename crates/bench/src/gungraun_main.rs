#[cfg(not(test))]
use bench::benchmarks::map::gungraun_benches::map_benches;

#[cfg(not(test))]
gungraun::main!(library_benchmark_groups = map_benches);

#[cfg(test)]
fn main() {}
