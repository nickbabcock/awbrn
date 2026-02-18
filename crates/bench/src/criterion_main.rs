use bench::benchmarks::map;
use criterion::criterion_main;

criterion_main!(map::criterion_benches::map_benches);
