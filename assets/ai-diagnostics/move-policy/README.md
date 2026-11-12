# Move model fitted to human unit orders

`human-move-v1-blend.json` is a conditional logit model fitted to the unit
orders of human players. `awbrn_ai::agents::move_policy` loads it.
`planner-v8-move` (`PlannerConfig::V8_MOVE`) is `planner-v8` with this model
at strength 0.5 in its greedy policy (`Weights::move_policy`).

## The model

- One decision is one order of one unit, with the board immediately before
  the order. The choices are all the legal orders of that unit: each
  destination with each action there (wait, attack a target, capture, load,
  join, supply). Games with fog are not read.
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

- The fit script puts `missile` in the direct group, but the policy gives it
  the indirect coefficients. To correct this changes the play.
- The extractor does not read the human unload decisions, so `act_unload`
  has no data and its coefficient is zero. In play, the legal orders give
  the unload orders after the orders of all units, so the unloads of a
  transport are usually a separate choice group from its other orders.
- The fit uses no fog. The model is off in fog games, where a run of 56
  pairs measured no gain (-0.018 ± 0.195). In fog, `planner-v8-move` plays
  the same games as `planner-v8`.

## Make the model again

The inputs are the replay archives and map files that the build model
README names.

```text
cargo build --release -p awbrn-ai-diagnostics --example move_policy
target/release/examples/move_policy names <rows>/names.json
target/release/examples/move_policy extract <maps> <rows>/<division> <division-folder>
```

Do the extract step for each of the seven folders. Each command writes
`<rows>/<division>.jsonl` and `<rows>/<division>.f32`. Then fit with NumPy
and SciPy, with the row prefixes in name order (Cups 1 to 6, then the Global
League):

```text
python crates/awbrn-ai-diagnostics/examples/fit_move_policy.py \
  --names <rows>/names.json --rows <rows>/<division>... \
  --max-train 120000 --model <fit>/model.json --summary <fit>/summary.json
```

With Python 3.13.9, NumPy 2.5.4 and SciPy 1.18.1, this gives the same split,
the same decision counts and the same validation top-1 as the model file.
The coefficients are not exactly the same: the solver path changes with the
number of BLAS threads, and the foot and indirect coefficients move by up to
0.025.

`move_policy score <strength> <rows>/<division>` gives the top-1 agreement
of the built-in model as the policy reads it in play.
