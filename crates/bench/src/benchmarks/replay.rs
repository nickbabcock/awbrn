const REPLAY_FIXTURES: [&str; 6] = [
    "1362397.zip",
    "1391406.zip",
    "1403019.zip",
    "1419680.zip",
    "1468032_landfall_2025-12-22.zip",
    "1563018.zip",
];

fn replay_fixture_path(file_name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/replays")
        .join(file_name)
}

fn read_fixture(file_name: &str) -> Vec<u8> {
    let path = replay_fixture_path(file_name);
    std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "failed to read replay fixture {file_name} ({path}): {error}",
            path = path.display()
        )
    })
}

fn parse_replay(parser: &awbw_replay::ReplayParser, fixture_name: &str, data: &[u8]) -> usize {
    let replay = parser
        .parse(data)
        .unwrap_or_else(|error| panic!("failed to parse replay fixture {fixture_name}: {error}"));
    replay.games.len() + replay.turns.len()
}

pub mod criterion_benches {
    use super::*;
    use criterion::{BenchmarkId, Criterion};
    use std::hint::black_box;

    fn replay_parser(c: &mut Criterion) {
        let fixtures: Vec<_> = REPLAY_FIXTURES
            .iter()
            .copied()
            .map(|fixture| (fixture, read_fixture(fixture)))
            .collect();
        let parser = awbw_replay::ReplayParser::new();

        let mut group = c.benchmark_group("replay_parser");
        for (fixture_name, data) in &fixtures {
            group.bench_function(BenchmarkId::from_parameter(*fixture_name), |b| {
                b.iter(|| black_box(parse_replay(&parser, fixture_name, data)));
            });
        }
        group.finish();
    }

    criterion::criterion_group!(replay_benches, replay_parser);
}

#[cfg(not(target_family = "wasm"))]
pub mod gungraun_benches {
    use super::*;
    use gungraun::{library_benchmark, library_benchmark_group};

    fn setup(file_name: &str) -> (String, awbw_replay::ReplayParser, Vec<u8>) {
        (
            file_name.to_owned(),
            awbw_replay::ReplayParser::new(),
            read_fixture(file_name),
        )
    }

    #[library_benchmark(setup = setup)]
    #[bench::replay_1362397("1362397.zip")]
    #[bench::replay_1391406("1391406.zip")]
    #[bench::replay_1403019("1403019.zip")]
    #[bench::replay_1419680("1419680.zip")]
    #[bench::replay_1468032("1468032_landfall_2025-12-22.zip")]
    #[bench::replay_1563018("1563018.zip")]
    fn replay_parser(
        (fixture_name, parser, data): (String, awbw_replay::ReplayParser, Vec<u8>),
    ) -> usize {
        parse_replay(&parser, &fixture_name, &data)
    }

    library_benchmark_group!(name = replay_benches, benchmarks = [replay_parser,]);
}
