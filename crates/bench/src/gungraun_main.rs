use bench::benchmarks::{
    ai::gungraun_benches::ai_benches, awvm::gungraun_benches::awvm_benches,
    replay::gungraun_benches::replay_benches, server::gungraun_benches::server_benches,
};

mod instrumented {
    use super::{ai_benches, awvm_benches, replay_benches, server_benches};
    use gungraun::main;

    main!(library_benchmark_groups = [ai_benches, awvm_benches, replay_benches, server_benches]);

    pub fn run() {
        main();
    }
}

fn run_once() {
    macro_rules! run_once {
        ($($group:ident),+ $(,)?) => {
            $(
                $group::__run_setup(true);
                for (_, _, benches) in $group::__BENCHES {
                    for benchmark in *benches {
                        match benchmark.func {
                            gungraun::__internal::InternalLibFunctionKind::Iter(function) => {
                                for index in 0..function(None) {
                                    function(Some(index));
                                }
                            }
                            gungraun::__internal::InternalLibFunctionKind::Default(function) => {
                                function();
                            }
                        }
                    }
                }
                $group::__run_teardown(true);
            )+
        };
    }

    run_once!(ai_benches, awvm_benches, replay_benches, server_benches,);
}

fn main() {
    let mut args = std::env::args().skip(1);
    if args.any(|arg| matches!(arg.as_str(), "--bench" | "--gungraun-run")) {
        instrumented::run();
    } else {
        run_once();
    }
}
