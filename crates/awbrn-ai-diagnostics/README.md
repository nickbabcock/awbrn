# AI diagnostics

The diagnostics tool runs paired AI experiments from a plan. The plan is user
input. The tool resolves every agent, map, limit, and model before it writes a
manifest.

## Map inputs and run outputs

`assets/maps/<AWBW ID>.json` contains the map data in compact JSON.
`assets/ai-diagnostics/maps.json` selects the default diagnostic maps. Each
entry records its AWBW ID, name, source file, source factions, and category
labels. The source files in this registry are relative to `assets/maps`.
The registry can also set `fog` for a match. Category labels record imported
metadata; they do not set match settings.

The loader preserves the source setup and assigns canonical seats by AWBW
country turn order. It computes map dimensions, property records, initial
units, and fingerprints from the map data. These derived facts are not
stored in a second source manifest.

Run manifests record the computed source and normalized fingerprints. The
runner checks those identities when it resumes a run. Archived map manifests
can still supply expected fingerprints, but new registry entries omit them.
Tests check first-mover assignment, property ownership, and initial units.

## Run a plan

```text
cargo run -p awbrn-ai-diagnostics --bin ai-diagnostics -- \
  run --plan assets/ai-diagnostics/smoke-plan.json --output target/ai-smoke
```

The run always writes the manifest, append-only event log, match rows,
reduction, and performance output. The plan can also request outcome features,
producer usability, review, and verification.

Use the checked-in small plan for a smoke run. Use
`search-production-multimap-plan.json` for the saved search experiment. Search,
learned, and tactical agents are diagnostics candidates. They are not player
profiles.

## Parallel workers

The `run` command plays matches on parallel workers. Set `AWBRN_AI_JOBS` to
change the worker count. The default is the number of cores. The runner
keeps at most one uncommitted match per worker. Parallel workers write event
rows to temporary files in the output directory. The files are removed when
their handles close, including after an error or panic. A single worker writes
rows directly to the event log. The runner commits each match in the sequential
order, so the event
log and all derived outputs do not change with the worker count. Turn times
include the effect of other workers on the same host.

## Run a sequential development test

```text
cargo run --release -p awbrn-ai-diagnostics --bin ai-diagnostics -- \
  sprt --plan assets/ai-diagnostics/sprt/planner-v1-vs-hard.json \
  --output target/sprt-planner-v1 [--jobs 8]
```

The `sprt` command retains its name but uses bounded betting evidence.
One pair is two games on one map with one seed and swapped seats. The
runner checks for a decision after each complete round of maps. Each map
has the same weight. It stops at `max_pairs` if no decision occurs.

The default hypotheses are a mean pair differential at most `0` (H0) and
at least `+0.05` (H1). The default error rates are `alpha = beta = 0.05`.
Evidence against H0 must reach `1 / alpha`; evidence against H1 must reach
`1 / beta`. Bets use only previous rounds. The bounds require independent
fresh seed samples with an expected round mean that meets the hypothesis.
Repeated tuning on the same seeds does not provide these guarantees.
See [the bounded betting method](https://arxiv.org/abs/2010.09686).

The schema 3 result records the method, plan, Git source state, map
fingerprints, both log evidence values, every pair, and complete-turn timing.
Each game also records planner counters when its agent supplies them.
Schema 2 results remain readable. A fixed run that ends with a partial map
round retains the decision from its last complete round. Direct calls to
`run_sprt` do not require Git and leave the optional source record empty.
Its normal interval
is descriptive; it has no 95% coverage guarantee after a sequential stop.
Set `"stop_early": false` for a fixed-size run. Very small error rates alone
do not force a fixed-size run. Historical schema 1 results used a different
stopping rule. A clean day-limit exit scores as a draw. Invalid commands or
other missing outcomes stop the run with an error.

The planner respects the plan's `node_budget`, including reply evaluations.
Use `32` to match the current Hard profile, which uses `planner-v6`.
`planner-v3` uses at most 16 evaluations, so a budget of `32` does not change
it. A budget of `1` scores only the seed turn. Use a new run directory after a
policy or budget change. The plan
`assets/ai-diagnostics/sprt/hard-v3-vs-hard-v2-review-fixed280.json` recorded
the promotion of `planner-v3` with 16 evaluations. Its profile now seats
`planner-v4`, so it does not repeat that result. The earlier four-evaluation
plan remains available for historical runs.

Use sequential runs for development. Keep the frozen gate and the sealed
holdout for a release decision.

## Measure planner strength and turn time

Run the fixed comparison of the final `planner-v6` against `planner-v4`, the
production baseline before this stack:

```text
cargo run --release -p awbrn-ai-diagnostics --bin ai-diagnostics -- \
  sprt --plan assets/ai-diagnostics/sprt/planner-v6-vs-v4-fixed280.json \
  --output target/planner-v6-comparison
```

The [corrected comparison](../../assets/ai-diagnostics/sprt/results/planner-v6-vs-v4-fixed280-summary.json)
completed 280 pairs with a mean pair differential of +0.2679 and a descriptive
95% half-width of 0.0757. There were no invalid commands. Each of the 14
development maps has 20 pairs. The run uses fresh seat seeds. It does not
provide a map holdout result. The sequential decision remains inconclusive
at the configured error rates of 1e-9.

The [earlier v6 summary](../../assets/ai-diagnostics/sprt/results/planner-v6-vs-v5-fixed280-summary.json)
used twice the fitted bank value. It records the old score. It does not
measure the corrected score. The corrected model stores the bank coefficient
per fund. The live score uses half the difference between the two bank terms.

Run the fixed comparison of `planner-v5` against `planner-v4`:

```text
cargo run --release -p awbrn-ai-diagnostics --bin ai-diagnostics -- \
  sprt --plan assets/ai-diagnostics/sprt/planner-v5-vs-v4-fixed280.json \
  --output target/planner-v5-comparison
```

`planner-v5` adds two changes to `planner-v4`. A power plan uses a legal
commander power before all other orders, and the Hard policy plays the rest of
the turn. The Hard scoring of `ai-hard-v3` adds a build floor, so a factory is
not left empty while the funds can buy a unit. This configuration is an intermediate step. Use the final `planner-v6`
comparison against `planner-v4` for the production decision.

The plan `planner-v4-vs-v3-fixed280.json` records the earlier comparison of
`planner-v4` against `planner-v3`. Each plan plays 280 pairs on 14 development
maps. Each plan uses both seats, a 35-day limit, and 32 evaluations per
planning call. The
[archived summary](../../assets/ai-diagnostics/sprt/results/planner-v4-review-summary.json)
records the source, inputs, results, and timing environment. The
[summary of the `planner-v3` promotion](../../assets/ai-diagnostics/sprt/results/hard-v3-review-summary.json)
records the earlier comparison against `ai-hard-v2`. These maps have been used
in development. Their results do not supply holdout evidence.

The replay regression tests in `crates/awbrn-ai/tests/replay_regressions.rs`
play positions from a match that a person won against the Hard profile. They
are fast tactical checks. They do not replace a paired experiment.

The [corrected v6 timing record](../../assets/ai-diagnostics/sprt/results/planner-v6-timing.json)
measures 715 turns on one thread. Native p95 is 1,136.6 ms. Wasm p95 under
Node 24.14.1 and V8 is 1,599.1 ms. The work counts and game lengths match.
These measurements use a local virtual machine. They do not measure the
service host.

Run `mise run ai:timing` on an idle host for native turn times. To measure
Wasm turn times with Node and V8, run:

```text
rustup target add wasm32-wasip1
cargo build --release -p awbrn-ai-diagnostics --example planner_timing \
  --target wasm32-wasip1
node scripts/run-wasi.mjs \
  target/wasm32-wasip1/release/examples/planner_timing.wasm . \
  /w/assets/ai-diagnostics/global-league-pool/manifest.json 1 v6
```

The timing example plays one pair per map on one thread. Compare work
counts between builds before comparing times. Host load affects elapsed
time. Measure the service host before setting a runtime requirement.
Wall-clock limits are excluded from CI; tests check deterministic work limits.

## Run a search budget sweep

```text
cargo run -p awbrn-ai-diagnostics --bin ai-diagnostics -- \
  search-sweep --plan assets/ai-diagnostics/search-budget-sweep-plan.json \
  --output target/ai-search-sweep
```

The search budget sweep holds the evaluator, maps, seeds, and reply policy fixed.
It compares sequential-quota and round-robin allocation at 4, 16, 64, and 256
nodes. It uses separate tuning and evaluation seed sets. The output contains
`search-coverage-matrix.json`, `budget-sweep.json`, `scenario-reachability.json`,
and `search-sweep-decision.json` with a Markdown rendering beside the JSON record.

## Analyze and resume

```text
ai-diagnostics analyze --run target/ai-smoke --analysis outcome-features
ai-diagnostics analyze --run target/ai-smoke --analysis producer-usability
ai-diagnostics review --output target/ai-smoke
ai-diagnostics verify --output target/ai-smoke
```

The event log is the source of truth. The `analyze` command first rebuilds the
core derived files from it. Completed matches are skipped on resume. The
manifest must match the plan and source state that started the run. The event
log remains append-only.

Plans do not contain source provenance overrides. The runner records the Git
revision, dirty state, and a source fingerprint. A dirty source fingerprint
includes the tracked working-tree diff and the contents of untracked files.
The fingerprint is part of the manifest identity, so a changed source state
cannot resume an existing run.

Feature analysis records authoritative and fog-visible features in separate
views. It reports early, middle, and late turns, grouped pair-level validation,
map-level intervals, the corpus fingerprint, and the exact reduced model.
Threat features are post-hoc in the authoritative view. Only fog-visible
features can support a live policy.

Producer usability is a diagnostics-only stage. Add
`"producer-usability"` to a plan's `analyses` and provide its
`producer_usability` fixture and threshold settings. Both sides of this plan
must use the same accepted agent configuration. The decision record uses the
separate labels `production-property-count-v1` and `producer-usability-v1` for
the diagnostic comparison.

The manifest stores the materialized producer plan. Reanalysis loads those
settings from the manifest. The stage writes scenario, performance, behavior,
and decision artifacts. It also keeps a disabled-control event log and checks
command and event fingerprints against the enabled run. It does not change
evaluator scores, candidate generation, greedy repair, search, or selected
commands. The producer fields in `feature-analysis/features.jsonl` are the
corpus-level record and are reused by the producer stage.

Model files are resolved as safe paths relative to the plan. Their content,
not their path, enters the agent identity. A non-converged or insufficient
corpus cannot enter a learned candidate.

## Add a candidate

Add an `AgentSpec` variant in the diagnostics crate. Validate all fields,
resolve all files, include every behavior-changing value in the identity, and
add materialization and smoke tests. Keep experimental policy code in this
crate. Do not add experimental fields to `AiProfile`.

## Promotion gate

Do not promote a candidate because an outcome model predicts completed games.
Require a fresh paired experiment with independent seeds and maps, complete
coverage, stable pair-level uncertainty, no invalid-command regression, and a
runtime result that is acceptable for the target. Review the action-selection
result against the locked baseline before changing a production profile.

## Agent seed protocol

Set `"agent_seed_protocol": "seat-seeded"` in a new experiment plan to give
each physical seat the same agent random stream in both games of a pair.
This permits a comparison of policy changes with fixed seat streams.

If the field is absent, the runner uses `role-seeded`: the candidate uses
stream 0 and the baseline uses stream 1 in both games. The default field is
omitted from plan and manifest JSON. The existing plan identity and seed
assignment stay the same. A seat-based plan records its protocol in the
manifest and includes it in the configuration fingerprint. Use a new run
directory when the protocol changes.

SPRT plans accept the same `agent_seed_protocol` field. The SPRT result records
a seat-based protocol. Old SPRT plans and results use role-based seeds. Both
runners use the same seed assignment with one or more workers.

Use `assets/ai-diagnostics/sprt/planner-v3-seat-seeded-smoke.json` to check the
planner with seat-based seeds. Its two pairs and two-day limit test the run
path only. They do not measure match strength. Historical results require
the original source revision and plans.
