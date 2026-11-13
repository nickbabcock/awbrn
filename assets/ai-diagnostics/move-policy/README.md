# Move model fitted to human unit orders

`human-move-v1-blend.json` and `human-move-fog.json` are conditional logit
models fitted to the unit orders of human players.
`awbrn_ai::agents::move_policy` loads them. `planner-v8-move`
(`PlannerConfig::V8_MOVE`) is `planner-v8` with a move model at strength 0.5
in its greedy policy (`Weights::move_policy`): `human-move-v1-blend.json` in
games without fog, and `human-move-fog.json` in fog games. The Hard profile
(`ai-hard-v4`) seats `planner-v8-move`.

## The model without fog

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
half width). These runs used earlier commits of the policy, before the
correction of the unload groups (see the limits below), and the
configuration `v8` with `baseline_weights: {"move_policy": 0.5}`: strength
0.5 scored +0.661 ± 0.060 over 280 pairs (462/6/92 games). On 56-pair
probes, strength 0.25, 1 and 2 scored +0.384, +0.321 and −0.589, and
strength 0.5 scored +0.652 in the Grit mirror, +0.518 in the Sami mirror and
+0.857 against `planner-v7`. Two variants were neutral and were removed: the
model without the greedy features, and the model in the own lines of the
planner only.

With the current policy (configuration `v8-move`), without fog:
+0.713 ± 0.057 over 280 pairs (Andy mirror, 477/5/78 games; +0.725 without
map 172238) and +0.634 ± 0.136 over 56 pairs (Sami mirror, run seed
202611131501). The plan of the 280-pair run is
`assets/ai-diagnostics/sprt/planner-v8-move-vs-v8-fixed280.json`. On the
same seeds, the correction of the unload groups changed the score by
-0.005 ± 0.013 (280 pairs) and -0.036 ± 0.049 (Sami, 56 pairs). On map
160882 (islands, 20 pairs) the score was +0.350 before and after. In the
280 pairs, the turn time p95 of `planner-v8-move` was 1.8 s, and of
`planner-v8` 1.2 s.

Turn time: in the same 14 pairs, the mean turn of `planner-v8-move` takes
1.5 times that of `planner-v8` (1.9 times before the features were made
lazy). For each unit of planner work it takes 1.2 times as long.

## Limits of the model without fog

- The fit of this model put `missile` in the direct group, but the policy
  gives it the indirect coefficients. To correct this changes the play. Each
  row now names the group of the policy, and the fit script reads it, so a
  new fit agrees with the policy.
- The extractor does not read the human unload decisions. In play, the
  unload orders of a transport are a separate choice group. Before a
  correction, the unload orders of the transport that the legal orders give
  last were in the group of its other orders. The fit rows have the same
  groups, so the coefficient of `act_unload` is -19.4 in the group `other`
  (0 in the other groups): in that group, the model did not choose an
  unload. In a group of unload orders only, `act_unload` is the same for all
  orders and does not change the choice. The model chooses only in a group:
  the greedy scores choose between the groups. The correction changes few
  rows (1 of 143,818 rows in Cup 4 and Smacky Cup 2).
- The choices of a row are all the legal orders of the unit, with delete,
  hide, reveal, repair and explode. In play, the policy gives only wait,
  capture, supply, join, load and attack orders, and the unloads in their
  own group. The rows of the fits have these wider choices, and decisions
  with those actions.

## The fog model

`human-move-fog.json` has the same features and groups. It is fitted to the
Division 1 games of Smacky Cups 1 to 5 and the 3 games of Smacky Cup 6 that
the parser read at the time (fog games, corpus `fog-tournaments`).

- The features come from the observation of the acting player, which is
  what the agent gets in play. At a decision, the player sees about 30% of
  the enemy units. The replay adapter keeps the true state. At the start of
  each turn, the true state agreed with the snapshot of the archive for
  about 99.8% of the unit rows, the same share as in games without fog.
- The features read the map that the player knows. In the observation, a
  tile in fog has no owner. The properties of the player are always in
  sight, so a property in fog is an enemy or neutral property: it is a
  capture target, and a headquarters in fog is a goal of the distance to the
  enemy headquarters. A factory or a lab in fog does not become a goal,
  because it can be neutral. On a map with labs and no headquarters, a goal
  comes only from an enemy factory in sight. Only the seat that owns the
  observation reads the known map. The planner plays the simulated enemy
  reply on its own observation, where a tile in fog can be the enemy's
  property, so the reply reads only the tiles in sight.
- The greedy scores of `planner-v8` do not read the known map: a property
  in fog is not a capture target for them. The move model does not change
  this.
- The fit did not read the unloads or the move of a transport before an
  unload. A fit with them had the same top-1 but scored lower in play
  (+0.277 ± 0.173 over 56 pairs, against +0.509 without the known map).
- Fit: 258,224 of 329,770 decisions in 546 games on 28 maps, at most
  120,000 for each group. The 29 games that end on day 1 or 2 are not read.
  Validation: 6 fog maps (79493, 128648, 151871, 151877, 174768 and 181746;
  203 games), the lowest hashes of label `awbrn-move-holdout-v1` until 25%
  of the fog decisions, at most 40,000 decisions for each group.
- The fit maps include 75796 and 143354, which the Global League pool
  manifest marks as `untouched_holdout`. Thus these two maps are not a
  sealed holdout for this model in fog games.
- No fog game of the corpus is on a map of the corpus without fog, so the 8
  validation maps of the model without fog have no fog games.
- At the time of the fit, the parser did not read the archives of Cup 6.
  227 archives of Cups 1 and 2 have no data for some actions: the archive
  holds the string `Array` in place of the action. The tool cannot replay
  these games.

Validation top-1 on the 6 fog maps (87,148 decisions):

| Ranker | Top-1 | Log loss |
| --- | --- | --- |
| Greedy score | 0.282 | 2.74 (fit of the greedy features only) |
| `human-move-v1-blend.json` | 0.391 | 2.19 |
| Fog fit without the known map | 0.410 | 2.08 |
| `human-move-fog.json` | 0.413 | 2.06 |

With the known map, `hq_distance_change` is not zero for 74% of the orders,
against 13% without it. Other fits gave no more: a fit of fog and no-fog
games together, the same fit with a second coefficient vector for fog
games, and a fit with Cup 6 and the other archives that the parser reads
now (537,444 fog decisions).

Play in fog against `planner-v8` (14 development maps, Andy mirror, day
limit 35, node budget 256), with the seeds of the first fog run (run seed
202611122010), 56 pairs:

| Model in fog | Pair score |
| --- | --- |
| `human-move-v1-blend.json` at 0.5 | -0.018 ± 0.195 |
| Fog fit without the known map, at 0.25 | +0.348 ± 0.164 |
| Fog fit without the known map, at 0.5 | +0.509 ± 0.149 |
| Fog fit without the known map, at 1 | +0.125 ± 0.195 |
| `human-move-fog.json` at 0.5, known map also in the reply | +0.652 ± 0.143 |
| `human-move-fog.json` at 0.5 | +0.616 ± 0.146 |

The last two rows are a paired comparison on the same seeds: the reply
without the known map scored -0.036 ± 0.132 against the reply with it.

With fresh seeds, before the correction of the unload groups,
`planner-v8-move` against `planner-v8` in fog scored +0.689 ± 0.057 over
280 pairs (Andy mirror, run seed 202611131300) and +0.670 ± 0.133 over 56
pairs (Sami mirror, run seed 202611131400). With the current policy, the
same Andy seeds give +0.668 ± 0.059 over 280 pairs (466/2/92 games; -0.021
± 0.025 against the earlier policy on the same seeds). In these 280 pairs,
the turn time p95 of `planner-v8-move` was 1.9 s, and of `planner-v8` 1.0 s. The plan is
`assets/ai-diagnostics/sprt/planner-v8-move-vs-v8-fog-fixed280.json`. No
command was invalid.

## Make the models again

The inputs are the replay archives and map files that the build model
README names, and the fog corpus `fog-tournaments`.

```text
cargo build --release -p awbrn-ai-diagnostics --example move_policy
target/release/examples/move_policy names <rows>/names.json
target/release/examples/move_policy extract <maps> <rows>/<division> <division-folder>
```

Do the extract step for each folder, with the maps of its corpus. Each
command writes `<rows>/<division>.jsonl`, `<rows>/<division>.f32` and
`<rows>/<division>.coverage.json` (the counts of read, failed and unmatched
archives and decisions). Then fit with the row prefixes in name order: Cups
1 to 6 and the Global League of the standard corpus, then the six folders
of Smacky Cups 1 to 6. The mise task `ai:fit:move` runs the fit script with
uv, which installs Python 3.13.9 and the NumPy and SciPy versions that the
script names:

```text
mise run ai:fit:move -- \
  --names <rows>/names.json --rows <rows>/<division>... \
  --max-train 120000 --fog no|yes --exclude-games <short games> \
  --model <fit>/model.json --summary <fit>/summary.json
```

`--fog no` fits the games without fog, and `--fog yes` fits the fog games.
`<short games>` lists the fog games that end on day 1 or 2 (the `days`
column of the fog manifest). The script uses one BLAS thread. It keeps the
features as float32 and fits one group at a time. The fog fit used 3.3 GB
at most.

These commands do not give the two model files exactly:

- `human-move-fog.json`: on the rows of its fit, the script gives the same
  coefficients. A new extract gives different rows. The parser now reads
  more archives (545,535 fog decisions against 470,703), and a rules
  correction of Von Bolt changes 11 games of the fit (12,352 decisions).
  All other games of the fit give the same rows.
- `human-move-v1-blend.json`: it cannot be made again exactly. Its fit put
  `missile` in the direct group, its choices included the unload orders of
  a transport, and its script used float64 features. With 3 threads, that
  script gave coefficients that were different by up to 0.008 from the
  model file.

`move_policy score <strength> <rows>/<division>` gives the top-1 agreement
of the built-in models as the policy reads them in play.
