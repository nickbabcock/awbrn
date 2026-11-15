# Tree move model and next-unit model

`awbrn_ai::agents::move_policy2` loads these files with `include_bytes!`, so
they work in Wasm. `planner-v9` (`PlannerConfig::V9`) plays them in its
greedy policy (`profile::V9_GREEDY_CONFIG`).

| File | Model | Trees | Bytes |
| --- | --- | --- | --- |
| `move2-nofog.awt`, `move2-fog.awt` | Order of a unit: boosted trees with a softmax over the orders of the unit, 148 features | 500, depth 6 | 704,994 / 705,878 |
| `unit2-nofog.awt`, `unit2-fog.awt` | Next unit: boosted trees with a softmax over the units that can act, 199 features | 394 / 400, depth 6 | 521,849 / 572,283 |

The move model files also hold 12 extra values from their fit, which play
does not read (a fit with `policy2/fit.py` writes none). `fixtures/*.rows` are 50 held-out rows of each model with the
raw scores of LightGBM; the test
`move_policy2::tests::the_trees_score_as_lightgbm` checks that the Rust
trees give the same scores.

## The models

One decision is one order of one unit, among the orders that the greedy
policy plays: wait, capture, attack, load, join and supply. The features of
one order are the 50 move features of the first model, 18 features of the
board near the destination, the unit group, a comparison with the other
orders of the unit, and 25 features of the turn so far (`TurnLog`): the
unit orders that the player gave in this turn before the decision. In a
planner simulation each line, reply and rollout turn keeps its own log.

`Weights::move2` plays the order with the highest tree score.
`Weights::move2_order` adds the next-unit model: of the units whose best
greedy score is above zero, the unit with the highest utility acts, and the
move model chooses its order. The greedy scores still decide the builds,
the powers, the unloads and when the turn ends.

## Data

| Model | Replays | Fit | Held out |
| --- | --- | --- | --- |
| Without fog | Division 1 games of Amarriner Cups 1 to 6 and Global League games rated over 1500 (corpus `standard-tournaments`) | 249,325 decisions | 98,852 decisions on 8 maps |
| Fog | Division 1 games of Smacky Cups 1 to 6 (corpus `fog-tournaments`), without the games in `fog-exclude-games.txt` | 328,117 decisions | 119,082 decisions on 6 maps |

The held-out maps are those of the first move model. `fog-exclude-games.txt`
lists the fog games that end on day 1 or 2 and those that the rows of the
first fog fit did not have. Early stopping uses 15% of the fit decisions, by
map.

Top-1 agreement with the human choice and log loss on the held-out maps:

| Model | Without fog | Fog |
| --- | --- | --- |
| Greedy score | 0.348 | 0.317 |
| Linear over the 50 move features, fitted to the same rows | 0.437 / 1.903 | 0.445 / 1.914 |
| Move model, trees | 0.530 / 1.477 | 0.546 / 1.482 |
| Next unit: best greedy score | 0.234 | 0.217 |
| Next unit: trees | 0.450 / 1.617 | 0.416 / 1.779 |

## Make the models again

The inputs are the replay archives and map files of the two corpora, as for
the first move model. Extract the rows of each division folder with the maps
of its corpus:

```text
cargo build --release -p awbrn-ai-diagnostics --example move_policy
target/release/examples/move_policy extract-policy2 <maps> <rows>/<division> <division-folder>
```

The extractor calls `GreedyAgent::policy2_candidates`, the code of play,
with the greedy weights of `planner-v8`; the test
`move_policy2::tests::play_reads_the_features_that_the_extractor_writes`
checks that play reads the same features. Then fit each model with the mise
task `ai:fit:move2` (`policy2/fit.py`, run with uv):

```text
mise run ai:fit:move2 -- --rows <rows>/<division>... --fog no \
  --holdout-maps 77060 90948 107495 118575 133605 140000 163134 163726 \
  --model trees --out <fit>/move2-nofog --fixture <fit>/move2-nofog.rows --fixture-keep 50
mise run ai:fit:move2 -- --rows <rows>/<smacky-division>... --fog yes \
  --exclude-games assets/ai-diagnostics/move-policy2/fog-exclude-games.txt \
  --holdout-maps 79493 128648 151871 151877 174768 181746 \
  --model trees --out <fit>/move2-fog --fixture <fit>/move2-fog.rows --fixture-keep 50
```

For the next-unit models, give `--model unit --rounds 400` and the outputs
`unit2-nofog` and `unit2-fog`. Give the row prefixes in this order:
Amarriner Cups 1 to 6, then the Global League folder; Smacky Cups 1 to 6.
Each fit writes `<out>.awt` and `<out>.report.json` with the held-out
results, and stays under 5 GB. Copy the `.awt` and `.rows` files here.
