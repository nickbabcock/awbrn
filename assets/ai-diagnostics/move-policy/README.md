# Move model fitted to human unit orders

`human-move-v1-blend.json` and `human-move-fog.json` are conditional logit
models fitted to the unit orders of human players.
`awbrn_ai::agents::move_policy` loads them. `planner-v8-move`
(`PlannerConfig::V8_MOVE`) is `planner-v8` with a move model at strength 0.5
in its greedy policy (`Weights::move_policy`): `human-move-v1-blend.json` in
games without fog, and `human-move-fog.json` in fog games. The Hard profile
(`ai-hard-v4`) seats `planner-v8-move`.

## The model

- One decision is one order of one unit, with the board immediately before
  the order. The choices are the legal orders of that unit: each destination
  with each action there.
- `GreedyAgent::move_candidates` calculates the 50 features of each order
  (`MOVE_FEATURES`). The extractor and the policy use the same function.
  Three features come from the greedy score of the order.
- There is one coefficient vector for each unit group: foot, direct, indirect
  and other (air, naval and transports).

In play, the greedy scores still decide which unit acts next and when the
turn ends. The model chooses the order of the unit: the chosen order gets
the highest greedy score of the unit, and the other orders of the unit get
zero. The utility of an order is the greedy part of the model plus the
strength times the other part, so at strength zero the choice is the greedy
choice. The policy calculates the features of a unit only when its chosen
order can change the selection.

In fog games, the features come from the observation of the acting player.
They also read the map that the player knows. A tile in fog has no owner in
the observation, but the properties of the player are always in sight. Thus
a property in fog is a capture target, and a headquarters in fog is the
enemy headquarters. A factory or a lab in fog does not become a goal. Only
the seat that owns the observation reads the known map: the simulated enemy
reply of the planner reads only the tiles in sight.

## Data

| Model | Replays | Fit | Validation |
| --- | --- | --- | --- |
| Without fog | Division 1 games of Amarriner Cups 1 to 6 and the Global League games of the build model README | 33 maps, 250,080 decisions | 8 maps, 98,970 decisions |
| Fog | Division 1 games of Smacky Cups 1 to 5 (corpus `fog-tournaments`) | 28 maps, 258,224 decisions | 6 maps, 87,148 decisions |

The split is by map, with the label `awbrn-move-holdout-v1`. Each fit uses
at most 120,000 decisions for each group. Each model file lists its maps in
`notes`.

The reserved maps of the Global League pool are not a clean holdout for
these models. Holdout maps 146797 and 173170 are in the fit without fog.
Maps 75796 and 143354, which the pool manifest marks as `untouched_holdout`,
are in the fog fit.

## Results

Top-1 agreement with the human order on the validation maps:

| Ranker | Without fog | Fog |
| --- | --- | --- |
| Greedy score | 0.346 | 0.282 |
| Model | 0.436 | 0.413 |

Play against `planner-v8` on the 14 development maps (mirror, day limit 35,
node budget 256, mean pair differential with the descriptive 95% half
width). No command was invalid.

| Games | Commander | Pairs | Score | Turn time p95 |
| --- | --- | --- | --- | --- |
| Without fog | Andy | 280 | +0.713 ± 0.057 | 1.8 s against 1.2 s |
| Without fog | Sami | 56 | +0.634 ± 0.136 | 1.9 s against 1.3 s |
| Fog | Andy | 280 | +0.668 ± 0.059 | 1.9 s against 1.0 s |

The plans of the 280-pair runs are
`assets/ai-diagnostics/sprt/planner-v8-move-vs-v8-fixed280.json` and
`planner-v8-move-vs-v8-fog-fixed280.json`. An earlier commit of the policy
scored +0.570 ± 0.065 over 280 pairs in the Sami mirror without fog. On map
160882 (islands) the score without fog was +0.350 over 20 pairs.

Strength 0.5 is the best of the strengths that were played (0.25, 0.5, 1
and 2). The model alone, without the greedy features, was neutral against
`planner-v8`. In fog, `human-move-v1-blend.json` was neutral
(-0.018 ± 0.195 over 56 pairs), and the fog fit without the known map
scored +0.389 ± 0.073 over 280 pairs.

## Limits

- The policy chooses one order for one unit. It does not plan the orders of
  several units together.
- The fit has no unload decisions. The unload orders of a transport are a
  separate choice group in play, and the choice between them is not fitted.
- The choices of a fit row are all the legal orders of the unit, with
  delete, hide, reveal, repair and explode. In play, the policy gives only
  wait, capture, supply, join, load and attack orders.
- A fit row with a repair order is not written when one destination has
  repair orders for different units.
- The known map does not give a goal on a map with labs and no
  headquarters, unless an enemy factory is in sight.
- The greedy scores of `planner-v8` do not read the known map. The move
  model does not change this.
- 227 archives of Smacky Cups 1 and 2 have the string `Array` in place of
  some actions. The tool cannot replay these games.

## Make the models again

The inputs are the replay archives and map files that the build model
README names (`assets/ai-diagnostics/build-policy/README.md`), and the fog
corpus `fog-tournaments`.

```text
cargo build --release -p awbrn-ai-diagnostics --example move_policy
target/release/examples/move_policy names <rows>/names.json
target/release/examples/move_policy extract <maps> <rows>/<division> <division-folder>
```

Do the extract step for each folder, with the maps of its corpus. Each
command writes `<rows>/<division>.jsonl`, `<rows>/<division>.f32` and
`<rows>/<division>.coverage.json`. Then fit with the row prefixes in name
order: the standard corpus, then the Smacky Cup folders. The mise task
`ai:fit:move` runs the fit script with uv, which installs Python 3.13.9 and
the NumPy and SciPy versions that the script names:

```text
mise run ai:fit:move -- \
  --names <rows>/names.json --rows <rows>/<division>... \
  --max-train 120000 --fog no|yes --exclude-games <short games> \
  --model <fit>/model.json --summary <fit>/summary.json
```

`--fog no` fits the games without fog, and `--fog yes` fits the fog games.
`<short games>` lists the fog games that end on day 1 or 2 (the `days`
column of the fog manifest). The script uses one BLAS thread and at most
3.3 GB.

These commands do not give the two model files exactly:

- `human-move-fog.json`: on the rows of its fit, the script gives the same
  coefficients. A new extract gives different rows, because the parser now
  reads more archives and a rules correction of Von Bolt changes 11 games.
- `human-move-v1-blend.json`: its fit put `missile` in the direct group, its
  choices included the unload orders of a transport, and its script used
  float64 features. It cannot be made again exactly.

`move_policy score <strength> <rows>/<division>` gives the top-1 agreement
of the built-in models as the policy reads them in play.
