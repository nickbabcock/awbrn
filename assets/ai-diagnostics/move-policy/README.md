# Move model fitted to human unit orders

`human-move-v1-blend.json` is a conditional logit model fitted to the unit
orders of human players. `awbrn_ai::agents::move_policy` loads it.
`planner-v8-move` (`PlannerConfig::V8_MOVE`) is `planner-v8` with this model
at strength 0.5 in its greedy policy (`Weights::move_policy`).

## The model

- One decision is one order of one unit, with the board immediately before
  the order. The choices are all the legal orders of that unit: each
  destination with each action there (wait, attack a target, capture, load,
  join, supply). Games with fog are not read for this model.
- `GreedyAgent::move_candidates` calculates the 50 features of each order
  (`MOVE_FEATURES`). The extractor and the policy use the same function.
- Three features come from the greedy score of the order: the score, its gap
  to the best order of the unit, and whether it is the best order.
- There is one coefficient vector for each unit group: foot, direct, indirect
  and other (air, naval and transports).

In play, the greedy scores still decide which unit acts next and when the
turn ends. The model chooses the order of the unit: the chosen order gets
the highest greedy score of the unit, and the other orders of the unit get
zero. This is the same method as the build model. The utility of an order is
the greedy part of the model plus the strength times the other part, so at
strength zero the choice is the greedy choice. The policy calculates the
features of a unit only when its chosen order can change the selection.

## Data and split

The replays are the Division 1 games of Amarriner Cups 1 to 6 and the
Global League games that the build model README names
(`assets/ai-diagnostics/build-policy/README.md`). The split is by map, with
the label `awbrn-move-holdout-v1`. The model file lists the maps in
`notes.train_maps` and `notes.holdout_maps`.

- Fit: 33 maps, 250,080 decisions in 646 games, at most 120,000 decisions
  for each group.
- Validation: 77060, 90948, 107495, 118575, 133605, 140000, 163134 and
  163726. 98,970 decisions in 255 games.

In the Global League pool (`assets/ai-diagnostics/global-league-pool/`),
development map 77060 is in validation (1,101 decisions); it is the only one
of the 14 development maps of the play tests with human decisions. Holdout
map 140000 is in validation (7,718 decisions), and holdout maps 146797 and
173170 are in the fit (355 decisions). These four holdout maps were used on
2026-11-01, before this fit. Most other maps are Amarriner Cup maps: as for
the build model, the reserved tournament maps are not a clean holdout for
this model.

## Results

Validation maps, all groups:

| Ranker | Top-1 | Top-3 | Log loss |
| --- | --- | --- | --- |
| Uniform | 0.053 | 0.155 | 3.09 |
| Greedy score | 0.346 | 0.524 | 2.42 (fit of the greedy features only) |
| Model | 0.436 | 0.672 | 1.90 |

Play, against `planner-v8` on the 14 development maps (Andy mirror, day
limit 35, node budget 256, mean pair differential with the descriptive 95%
half width): strength 0.5 scored +0.661 ± 0.060 over 280 pairs (462/6/92
games). On 56-pair probes, strength 0.25, 1 and 2 scored +0.384, +0.321 and
−0.589, and strength 0.5 scored +0.652 in the Grit mirror, +0.518 in the
Sami mirror and +0.857 against `planner-v7`. Two variants were neutral and
were removed: the model without the greedy features, and the model in the
own lines of the planner only.

The plan of the 280-pair run is
`assets/ai-diagnostics/sprt/planner-v8-move-vs-v8-fixed280.json`. The run
used configuration `v8` with `baseline_weights: {"move_policy": 0.5}`, which
is the same play as `v8-move`: the first 14 pairs give the same games.

Turn time: in the same 14 pairs, the mean turn of `planner-v8-move` takes
1.5 times that of `planner-v8` (1.9 times before the features were made
lazy). For each unit of planner work it takes 1.2 times as long.

## Limits

- The fit of this model put `missile` in the direct group, but the policy
  gives it the indirect coefficients. To correct this changes the play. The
  extractor now writes the group of the policy in each row, and the fit
  script reads it, so a new fit agrees with the policy.
- The extractor of this model did not read the human unload decisions, so
  `act_unload` has no data and its coefficient is zero. In play, the legal
  orders give the unload orders after the orders of all units, so the
  unloads of a transport are a separate choice group from its other orders.
  The extractor now writes an unload decision with the unload orders of the
  transport as its choices, and the move of the transport before the unload
  as a `wait` decision (`before_unload`).
- The fit uses no fog. The model is off in fog games, where a run of 56
  pairs measured no gain (-0.018 ± 0.195). In fog, `planner-v8-move` plays
  the same games as `planner-v8`. The fog model below is for fog games.

## The fog model

`human-move-fog-v1.json` has the same features and groups. It is fitted to
the Division 1 games of Smacky Cups 1 to 5 (fog games, corpus
`fog-tournaments`). `planner-v8-move-fog` (`PlannerConfig::V8_MOVE_FOG`) is
`planner-v8-move` with this model at strength 0.5 in fog games
(`Weights::move_policy_fog`). In games without fog it plays the same games
as `planner-v8-move`: two runs of 14 pairs gave the same games.

- The features come from the observation of the acting player, which is
  what the agent gets in play. The replay adapter keeps the true state. At
  the start of each turn, the tool compares it with the snapshot of the
  archive: about 0.2% of the unit rows are different, the same share as in
  the games without fog. At a decision, the player sees about 30% of the
  enemy units.
- In the observation, a tile in fog has no owner. Thus the enemy
  headquarters and the properties in fog are not in the distance features,
  and the army lead counts only the enemy units in sight.
- The fit did not read the unloads or the move of a transport before an
  unload (`--skip-actions unload --skip-before-unload`). A fit with them
  had the same top-1 but scored +0.277 ± 0.173 in play (56 pairs).
- Fit: 258,224 of 329,770 decisions in 546 games on 28 maps, at most
  120,000 for each group. Validation: 6 fog maps (79493, 128648, 151871,
  151877, 174768 and 181746; 203 games), the lowest hashes of label
  `awbrn-move-holdout-v1` until 25% of the fog decisions. At most 40,000
  validation decisions for each group are read. The 29 games
  that end on day 1 or 2 are not read. Cup 6 is not read: its archives have
  a newer format that the parser does not read. The parser also does not
  read 227 archives of Cups 1 and 2 that have no action data.

Validation top-1 on the fog maps (87,148 decisions, without unloads):

| Ranker | Top-1 | Log loss |
| --- | --- | --- |
| Greedy score | 0.282 | 2.74 (fit of the greedy features only) |
| `human-move-v1-blend.json` | 0.391 | 2.19 |
| Fog model | 0.410 | 2.08 |

With unloads (93,641 decisions), a fog-only fit gave 0.413, a fit of fog
and no-fog games together gave 0.412, and the same fit with a second
coefficient vector for fog games gave 0.413.

Play in fog against `planner-v8` (14 development maps, day limit 35, node
budget 256). With the seeds of the earlier fog run, Andy mirror, 56 pairs:

| Model in fog | Pair score |
| --- | --- |
| `human-move-v1-blend.json` at 0.5 | -0.018 ± 0.195 |
| Fog model at 0.25 | +0.348 ± 0.164 |
| Fog model at 0.5 | +0.509 ± 0.149 |
| Fog model at 1 | +0.125 ± 0.195 |

With fresh seeds at strength 0.5: the Sami mirror scored +0.241 ± 0.187
over 56 pairs (run seed 202611121800), and the Andy mirror scored
+0.389 ± 0.073 over 280 pairs (run seed 202611121700; 387/4/169 games). In
the 280 pairs, the turn time p95 of `planner-v8-move-fog` was 3.0 s, and of
`planner-v8` 1.9 s.

To fit the fog model, add `--fog yes --skip-actions unload
--skip-before-unload --exclude-games <file>` to the fit command below, with
the row prefixes of the Smacky Cups after those of the standard corpus. The
script keeps the features as float32 and fits one group at a time. The fit
used 3.5 GB at most, and two fits gave the same model file.

## Make the model again

The inputs are the replay archives and map files that the build model
README names.

```text
cargo build --release -p awbrn-ai-diagnostics --example move_policy
target/release/examples/move_policy names <rows>/names.json
target/release/examples/move_policy extract <maps> <rows>/<division> <division-folder>
```

Do the extract step for each of the seven folders. Each command writes
`<rows>/<division>.jsonl` and `<rows>/<division>.f32`. Then fit, with the
row prefixes in name order (Cups 1 to 6, then the Global League). uv
installs Python 3.13.9 and the NumPy and SciPy versions that the script
names:

```text
uv run --python 3.13.9 --script \
  crates/awbrn-ai-diagnostics/examples/fit_move_policy.py \
  --names <rows>/names.json --rows <rows>/<division>... \
  --max-train 120000 --model <fit>/model.json --summary <fit>/summary.json
```

The script uses one BLAS thread. To use a different number, set
`OPENBLAS_NUM_THREADS`. The fit gives the same split and the same decision
counts as the model file, but no thread count gives the same coefficients.
The direct and other coefficients are always the same. The foot and indirect
coefficients change:

- With 1 thread, by up to 0.025. The validation top-1 is the same.
- With 2, 3, 4 or 8 threads, by up to 0.008. All these counts give the same
  result. The foot top-1 changes by one decision.

The model file was fitted with 3 threads. The script of the model commit,
in the same environment and with 3 threads, also gives the second result.
Thus the remaining difference does not come from the BLAS threads.

`move_policy score <strength> <rows>/<division>` gives the top-1 agreement
of the built-in model as the policy reads it in play.
