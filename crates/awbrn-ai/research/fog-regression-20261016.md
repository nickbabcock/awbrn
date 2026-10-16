# Fog AI regression, 2026-10-16

This record describes the first repair. The
[fog position model investigation](fog-position-model-20261016.md) supersedes
that repair and records the later strength comparison.

The saved original run scored 38.13% of available points against v6 over
280 pairs. The retained repair scored 47.77% over 56 pairs on new seeds.
Its descriptive 95% interval is 6.52 percentage points on each side.
The result is consistent with parity. It does not establish a gain over v6.

## Cause and repair

The old threat overlay gave the same track weight to every possible target.
It did not reduce a target's weight when few possible enemy positions could
attack it. A larger set of possible positions therefore added more total
threat. The diagnostic now averages threat over possible positions. It counts
each target once per possible position, including for direct attacks.

The fog production policy also replaced the v6 composition estimate. It used
tracked units and filled half the residual value with the known enemy
composition. A small observed army could therefore change build scores for
the entire unseen army. Production now uses the v6 composition estimate.
The income ledger remains available for diagnostics.

These two changes alone did not remove the regression. Their first test
scored 49.11%, but their next test scored 37.95%. The retained policy therefore
uses remembered threat only while the enemy position remains known in the
current own turn. The enemy cannot move during that turn. After an enemy turn,
the possible positions remain in the full diagnostic estimate. They do not
change live tactics. `PlannerAgent` reads `FogMemory::estimate_for_policy`;
accuracy diagnostics still read the full `FogMemory::estimate`.

## Experiments

All tests used the same 14 development maps, fog, swapped seats, and seat
seeds. Each pair contained two games. Limits were 35 days and 32 nodes.
Each run completed its preset pair count. The sealed holdout was not used.
A score is wins plus half the draws, divided by the number of games.
The interval column gives descriptive 95% half-widths in percentage points.

| Variant | Run seed | Pairs | Score | Interval |
| --- | --- | ---: | ---: | ---: |
| original | 202610160101 | 280 | 38.12% | 3.41 |
| no-threat | 202610160101 | 28 | 40.18% | 8.85 |
| probability | 202610160101 | 28 | 42.86% | 10.04 |
| probability-v6-production | 202610160101 | 28 | 49.11% | 6.89 |
| original-fresh | 202610160202 | 28 | 41.07% | 6.77 |
| probability-v6-production-fresh | 202610160202 | 56 | 37.95% | 7.49 |
| repair | 202610160303 | 56 | 47.77% | 6.52 |

The first 28 pairs of the original saved run scored 34.82%.
The three tests on seed 202610160101 used those same map and match seeds.
The second seed set rejected the probability-only repair. The retained repair
used the third seed set. Its 51 wins, 56 losses, and 5 draws came from
112 games. All runs had zero invalid commands. The configured sequential
decision was inconclusive in every run.

These are development results. They do not provide map holdout evidence.
Experiments ran concurrently on a shared host. Their times do not establish
a runtime gate.

## Verification and records

All 287 tests passed in `awbrn-ai` and `awbrn-ai-diagnostics`, with all targets.
Clippy passed with warnings denied. Formatting and diff checks passed.
Three regression tests check these requirements:

- Uncertain threat equals the mean threat from the possible positions.
- Production scores do not use the speculative composition estimate.
- A remembered position affects tactics only before the enemy can move.

The production test failed with the original production policy and passed
with the repair. The existing planner test also confirms identical commands
for v6 and the fog variant when fog is disabled.

[The data record](fog-regression-20261016.json) contains plan, source, map,
and binary identities, outcomes, pair differentials, and descriptive intervals.
The binaries were copied before each policy changed. The ablation source
fingerprints describe the working tree at run start. They are not substitutes
for the binary hashes. The original source files are retained under
`target/fog-investigation` as `fog-original.rs` and `greedy-original.rs`.
Full match results and logs are also retained there.

[The repair plan](../../../assets/ai-diagnostics/sprt/planner-v6-fog-repair-dev-fog-fixed56.json)
repeats the retained policy comparison. Use a new output directory:

```sh
cargo run --release -p awbrn-ai-diagnostics --bin ai-diagnostics -- \
  sprt --plan assets/ai-diagnostics/sprt/planner-v6-fog-repair-dev-fog-fixed56.json \
  --output target/fog-repair-repeat --jobs 7
```
