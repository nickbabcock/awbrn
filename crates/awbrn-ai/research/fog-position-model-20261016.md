# Fog position model, 2026-10-16

The fog candidate scored 85.71% of available points against v6 on
56 fresh pairs. It won 94 games, lost 14, and drew 4. The descriptive
95% score interval is 5.84 percentage points on each side.

## Why fog planning failed

Two errors prevented the planner from using a fog observation as a complete
position.

A hidden property has no capture-point value in an observation. The legal
order generator can offer a capture there. Applying that order to the raw
observation model then fails with `ExecuteError::UnsupportedRuleset`. The
planner drops the line and can fall back to greedy play. A regression test
reproduces this exact error and verifies the modeled capture succeeds.

The planner also stored complete simulated positions as its predictions.
When an enemy left sight after an own move, the next observation omitted it.
The prediction still contained it. This expected visibility change therefore
caused a mismatch and another plan. A regression test now compares a projected
prediction with the real observation after such a move.

## Change

`FogMemory::planning_state` makes a separate position model. Hidden properties
receive full capture points. Their last observed owner is restored. A property
whose owner was never seen is modeled as neutral. Current observations remain
unchanged and take precedence over memory. A test confirms that unseen enemy
ownership does not enter the model.

The planner and its greedy fallback use this model. The queued predictions
are projected through the player's visibility rules before comparison with
the next observation. Newly revealed facts and combat luck can still cause
replanning. Model assumptions do not enter the comparison as observed facts.

Production keeps the v6 composition estimate. Tactical overlays use enemy positions known within the current own
turn. Cross-turn unit inference remains available for diagnostics.
The v6 control and games without fog retain their previous behavior.

## Results

The implementation was frozen before the fresh run. Later changes were
limited to tests and documentation. The run completed its preset size.
It used 14 development maps, fog, swapped seats, seat seeds, a 35-day limit,
and 32 nodes per planning call. The run used new seeds.
No sealed holdout maps were used.

| Run | Seed | Pairs | Score | Descriptive 95% half-width | Wins / losses / draws |
| --- | --- | ---: | ---: | ---: | ---: |
| fresh | 202610160505 | 56 | 85.71% | 5.84 | 94 / 14 / 4 |

A score is wins plus half the draws, divided by games. The preset bounded
betting decision stayed inconclusive at error rates of 1e-9. Its log evidence
against H0 was 0.909, below the 20.723 threshold. The score interval
is descriptive. These results provide development evidence, not map holdout
evidence. They measure the combined model changes. They do not isolate the
gain from remembered ownership alone.

The fresh candidate completed all 18636 attempted plan builds. The v6
control failed 2812 of 23495 builds (11.97%). Failed builds are plans
without a selected line. Fallback also occurs at work limits, so this count
is distinct from the total fallback count.

Native turn p95 was 1070.9 ms for the candidate and 924.6 ms for v6,
a ratio of 1.158. Seven workers ran on a shared host. The strength gain has
a computation cost. This run does not establish an idle-host runtime gate.
The production profile was not promoted by this test.

## Verification

All 291 AI and diagnostics tests passed with all targets. Clippy passed with
warnings denied. Formatting and diff checks passed. The new tests verify:

- A hidden-property capture fails with the original model and succeeds with the new model.
- Remembered ownership is restored, and a new observation overrides it.
- An initial projected prediction equals the actual observation model.
- An expected loss of sight does not require a new plan.

The existing fog game test also passed its legality and non-fog behavior checks.

[The data record](fog-position-model-20261016.json) contains source, binary,
plan, and map identities, outcomes, pair differentials, planner counters, and
timings. Full results, logs, source snapshots, and binaries remain in
`target/fog-strength`.

[The fixed plan](../../../assets/ai-diagnostics/sprt/planner-v6-fog-state-dev-fixed56.json)
repeats the fresh comparison. Use a new output directory:

```sh
cargo run --release -p awbrn-ai-diagnostics --bin ai-diagnostics -- \
  sprt --plan assets/ai-diagnostics/sprt/planner-v6-fog-state-dev-fixed56.json \
  --output target/fog-position-model-repeat --jobs 7
```
