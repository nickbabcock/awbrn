# Tree move model and next-unit model

`awbrn_ai::agents::move_policy2` loads these files with `include_bytes!`, as
the first move model loads its JSON (`../move-policy/README.md`): no file is
read at run time, so they work in Wasm. `planner-v9` (`PlannerConfig::V9`)
plays them in its greedy policy (`profile::V9_GREEDY_CONFIG`).

| File | Model | Trees | Bytes |
| --- | --- | --- | --- |
| `move2-nofog.awt`, `move2-fog.awt` | Order of a unit: boosted trees with a softmax over the orders of the unit, 148 features, and the greedy reference | 500, depth 6 | 704,994 / 705,878 |
| `unit2-nofog.awt`, `unit2-fog.awt` | Next unit: boosted trees with a softmax over the units that can act, 199 features | 394 / 400, depth 6 | 521,849 / 572,283 |

The four files are 2,505,004 bytes. `fixtures/*.rows` are 500 held-out rows
of each model with the raw scores of LightGBM. The test
`move_policy2::tests::the_trees_score_as_lightgbm` checks that the Rust trees
give the same scores.

## The models

One decision is one order of one unit, as for the first move model. The
choices are the orders that the greedy policy plays: wait, capture, attack,
load, join and supply. The features of one order:

- the 50 move features of the first model;
- 18 features of the board near the destination: own and enemy units at a
  few distances, the distance from the origin and from the last destination
  of the turn, whether the target or a neighbour was attacked earlier in the
  turn, and the owner of the destination;
- the unit group, one-hot;
- each order against the other orders of the unit: the difference to the
  highest, the lowest and the mean value of 16 move features, and the rank
  of the greedy score;
- 25 features of the turn so far (`TurnLog`): the count of earlier orders by
  action and by group, the last action and group, the day, and the counts of
  ready, moved, own and enemy units.

The turn so far is the unit orders that the player gave in this turn before
the decision. In a game the agent keeps the log of its own orders. In a
planner simulation each line, each reply and each rollout turn keeps its
own log.

The move model gives the utility of an order as `s` times the tree score plus
`1 - s` times the greedy reference, a conditional logit over the three greedy
score features for each group. `Weights::move2` is `s`. `planner-v9` uses
`s = 1`, the trees alone.

The next-unit model (`Weights::move2_order`) scores each unit that can act
from three blocks of its move features (the highest value over its orders,
the order with the best greedy score and the order that stays), its group
and health, its neighbours, its rank among the units and the turn so far. Of
the units whose best greedy score is above zero, the unit with the highest
utility acts, and the move model chooses its order. The greedy scores still
decide the builds, the powers and when the turn ends.

`Weights::move2_trees` scores the orders with only the first trees of the
move model. Fewer trees take less time and fit less well. The next-unit
model always reads every tree.

## Data

| Model | Replays | Fit | Held out |
| --- | --- | --- | --- |
| Without fog | Division 1 games of Amarriner Cups 1 to 6 and Global League games rated over 1500 (corpus `standard-tournaments`) | 249,325 decisions | 98,852 decisions on 8 maps |
| Fog | Division 1 games of Smacky Cups 1 to 6 (corpus `fog-tournaments`), without the games in `fog-exclude-games.txt` | 328,117 decisions | 119,082 decisions on 6 maps |

The held-out maps are those of the first move model: 77060, 90948, 107495,
118575, 133605, 140000, 163134 and 163726 without fog, and 79493, 128648,
151871, 151877, 174768 and 181746 in fog. `fog-exclude-games.txt` lists 158
fog games: those that end on day 1 or 2, and those that the rows of the first
fog fit did not have. Early stopping uses 15% of the fit decisions, by map.

## Results

Top-1 agreement with the human choice and log loss on the held-out maps:

| Model | Without fog | Fog |
| --- | --- | --- |
| Greedy score | 0.348 | 0.317 |
| Linear over the 50 move features, fitted to the same rows | 0.437 / 1.903 | 0.445 / 1.914 |
| Move model, trees | 0.530 / 1.477 | 0.546 / 1.482 |
| Next unit: best greedy score | 0.234 | 0.217 |
| Next unit: trees | 0.450 / 1.617 | 0.416 / 1.779 |

The first fog model was measured with foot decisions weighted to 40,000
(0.413 in `../move-policy/README.md`). With that weighting the linear row
scores 0.418 and the fog trees 0.523.

Play of `planner-v9` against `planner-v8-move` is in
`crates/awbrn-ai-diagnostics/README.md` ("Measure planner-v9").

## Limits

- The features of an order do not see the orders that the other units will
  give later in the turn. The rollout check of the planner does.
- The fit has no unload decisions. The unload orders of a transport stay a
  choice of the greedy scores.
- The models are fitted to Division 1 and highly rated games only.

## Make the models again

The inputs are the replay archives and map files of the two corpora, as for
the first move model.

```text
cargo build --release -p awbrn-ai-diagnostics --example move_policy
target/release/examples/move_policy extract-policy2 <maps> <rows>/<division> <division-folder>
```

Do the extract step for each division folder, with the maps of its corpus.
Each command writes `<rows>/<division>.jsonl`, `<division>.x.f32` (the order
features), `<division>.u.f32` (the unit features) and
`<division>.coverage.json`. The extractor calls
`GreedyAgent::policy2_candidates`, the code of play, with the greedy weights
of `planner-v8` and no fog estimate. `move_policy check-policy2 <maps>
<output> <division-folder>...` plays the tree move model and the next-unit
model on the same greedy weights at each human decision, with the human turn
so far, and counts the features that differ from the extracted ones; the
test `move_policy2::tests::play_reads_the_features_that_the_extractor_writes`
does the same in short games. Both check the feature code, not other greedy
weights: a policy whose greedy weights change the greedy scores of unit
orders computes the three greedy score features, and the features built from
them, from other values than the fit saw.

The turn so far also differs in one case. In fog, a move can be trapped by a
unit out of sight and stop short. The extractor logs the human order where
the unit stopped; an agent in play logs the order and the destination that
it chose, before the move ran.

In `planner-v9` play the features also differ from the fit in two places
that the check does not cover:

- `decisive_labs`: on a map with labs and no headquarters, every lab gets the
  headquarters weights in the greedy scores, so the greedy score features of
  orders whose scores read a lab differ there. Maps with a headquarters do
  not change. (`play_last_days` changes only the score of the end of the
  turn, which is not a candidate order.)
- `fog_repair`: in fog, a property out of sight has the capture points of an
  untouched property in the position that the planner and its greedy policy
  read, where the extractor reads the observation as it is.

`planner-v9` has no fog estimate (its fog memory is off), as the extractor.

Then fit each model. The mise task `ai:fit:move2` runs `policy2/fit.py` with
uv, which installs Python 3.13.9 and the NumPy, SciPy and LightGBM versions
that the script names:

```text
mise run ai:fit:move2 -- --rows <rows>/<division>... --fog no \
  --holdout-maps 77060 90948 107495 118575 133605 140000 163134 163726 \
  --model trees --out <fit>/move2-nofog --fixture <fit>/move2-nofog.rows --fixture-keep 500
mise run ai:fit:move2 -- --rows <rows>/<division>... --fog no \
  --holdout-maps 77060 90948 107495 118575 133605 140000 163134 163726 \
  --model unit --rounds 400 --out <fit>/unit2-nofog \
  --fixture <fit>/unit2-nofog.rows --fixture-keep 500
mise run ai:fit:move2 -- --rows <rows>/<smacky-division>... --fog yes \
  --exclude-games assets/ai-diagnostics/move-policy2/fog-exclude-games.txt \
  --holdout-maps 79493 128648 151871 151877 174768 181746 \
  --model trees --out <fit>/move2-fog --fixture <fit>/move2-fog.rows --fixture-keep 500
mise run ai:fit:move2 -- --rows <rows>/<smacky-division>... --fog yes \
  --exclude-games assets/ai-diagnostics/move-policy2/fog-exclude-games.txt \
  --holdout-maps 79493 128648 151871 151877 174768 181746 \
  --model unit --rounds 400 --out <fit>/unit2-fog \
  --fixture <fit>/unit2-fog.rows --fixture-keep 500
```

Give the row prefixes in this order: Amarriner Cups 1 to 6, then the Global
League folder; Smacky Cups 1 to 6. Each fit writes `<out>.awt` and
`<out>.report.json`, with the held-out results. The script reads the
features in chunks and gives LightGBM two threads; a fit stays under 5 GB.
Copy the `.awt` and `.rows` files here. A fixture holds the first 500 of
1,000 held-out rows that the fit draws with a fixed seed; on the archived
rows of the shipped fit, `--fixture-keep 500` selects the same rows as the
committed fixtures (checked against the rows, without a fit).

What is and is not reproducible:

- The fit sets a seed and LightGBM's deterministic mode with two threads. On
  the same rows it should give the same trees; this was not checked again by
  a second fit.
- The rows depend on the extract: a parser or rules change, or more archives
  in a corpus, gives other rows and other models. The rows of the shipped
  fit are not in the repository.
