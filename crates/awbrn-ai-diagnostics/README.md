# AI diagnostics

The diagnostics tool runs paired AI experiments from a plan. The plan is user
input. The tool resolves every agent, map, limit, and model before it writes a
manifest.

## Run a plan

```text
cargo run -p awbrn-ai-diagnostics --bin ai-diagnostics -- \
  run --plan assets/ai-diagnostics/smoke-plan.json --output target/ai-smoke
```

The run always writes the manifest, append-only event log, match rows,
reduction, and performance output. The plan can also request outcome features,
review, and verification.

Use the checked-in small plan for a smoke run. Use
`search-production-multimap-plan.json` for the saved search experiment. Search,
learned, and tactical agents are diagnostics candidates. They are not player
profiles.

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
