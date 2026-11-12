# Move model fitted to human unit orders

`human-move-v1-blend.json` and `human-move-v1-pure.json` are conditional
logit models fitted to the unit orders of human players in Amarriner Cups 1
to 6, Division 1, and the Global League games. `awbrn_ai::agents::greedy::move_policy`
loads them.

- One decision is one order of one unit, with the board immediately before
  the order. The choices are all the legal orders of that unit: each
  destination with each action there (wait, attack a target, capture, load,
  join, supply). The tool reads only games without fog.
- `GreedyAgent::move_candidates` calculates the 50 features of each order
  (`MOVE_FEATURES`). The extractor and the policy call the same function.
- Each model has one coefficient vector for each unit group: foot, direct,
  indirect and other (air, naval and transports).
- The blend model also reads three greedy score features: the greedy score of
  the order, its gap to the best order of the unit, and whether it is the
  best order. The pure model does not read them.

## How the policy uses the model

Set `Weights::move_policy` to a strength above zero. The greedy scores still
decide which unit acts next and when the turn ends. The model chooses the
order of each unit: the chosen order gets the highest greedy score of the
orders of that unit, and the other orders of the unit get zero. This is the
same method as the build model. The utility of an order is the greedy part
of the model plus the strength times the other part, so at strength zero the
choice is the greedy choice.

`PlannerConfig::move_lines` is a second use: the model plays the own lines
of the planner, and the simulated enemy replies keep the plain greedy
policy. The turn of the plain greedy policy is one more plan.

## Fit and validation

The split is by map. The validation maps are 77060, 90948, 107495, 118575,
133605, 140000, 163134 and 163726 (98,970 decisions, 255 games). The fit
uses 33 other maps (250,080 decisions, 646 games), with at most 120,000
decisions for each group. 77060 is the only development map in the corpus,
and it is in the validation set.

| Ranker on validation maps | Top-1 | Top-3 | Log loss |
| --- | --- | --- | --- |
| Uniform | 0.053 | 0.155 | 3.09 |
| Greedy score | 0.346 | 0.524 | (2.42 calibrated) |
| Pure model | 0.431 | 0.663 | 1.93 |
| Blend model | 0.436 | 0.672 | 1.90 |

Ties count as a share of a hit. The calibrated log loss of the greedy score
is a fit of the three greedy features only.

## Play results

`planner-v8` with the move model in its greedy policy, against `planner-v8`.
Andy mirror unless the row tells otherwise, 14 development maps, day limit
35, node budget 256. One pair is two games with the seats swapped. The value
is the mean pair differential with the descriptive 95% half width.

| Strength | Pairs | Score | Without 172238 | W/D/L (games) | Median days | Day limit |
| --- | --- | --- | --- | --- | --- | --- |
| 0.25 | 56 | +0.384 ± 0.154 | +0.375 | 76/3/33 | 29 | 36% |
| 0.5 | 56 | +0.688 ± 0.121 | +0.683 | 94/1/17 | 27 | 24% |
| 1 | 56 | +0.321 ± 0.176 | +0.288 | 73/2/37 | 28 | 28% |
| 2 | 56 | −0.589 ± 0.177 | −0.692 | 22/2/88 | 24 | 26% |
| Pure model, 1 | 56 | +0.018 ± 0.195 | −0.038 | 56/2/54 | 29 | 31% |
| Move lines, 0.5 | 56 | +0.652 ± 0.143 | +0.644 | 92/1/19 | 25.5 | 20% |
| **0.5, new seeds** | 280 | **+0.661 ± 0.060** | +0.677 | 462/6/92 | 27.5 | 25% |
| 0.5, Grit mirror | 56 | +0.652 ± 0.137 | +0.663 | 90/5/17 | 31 | 37% |
| 0.5, Sami mirror | 56 | +0.518 ± 0.150 | +0.558 | 85/0/27 | 26 | 20% |
| 0.5 against `planner-v7` | 56 | +0.857 ± 0.092 | +0.885 | 104/0/8 | 22 | 12% |

The 56-pair rows of the Andy mirror use the same seeds. The 280-pair run
used new seeds and was chosen after the probes; it is
`assets/ai-diagnostics/sprt/planner-v8-move-vs-v8-fixed280.json`, which
names the configuration `planner-v8-move` (`PlannerConfig::V8_MOVE`). The run
itself set the same weight with `baseline_weights`. There were no invalid
commands. The median turn time was about two times that of `planner-v8`
(492 ms against 251 ms) and the 95th percentile about 1.9 times.

The offline agreement is highest at strength 1, but the play is best at 0.5.
The model without the greedy score features is neutral in play.

## Make the model again

The inputs are the replay archives and map files that the build model
README names (`assets/ai-diagnostics/build-policy/README.md`).

```text
cargo build --release -p awbrn-ai-diagnostics --example move_policy
target/release/examples/move_policy names <rows>/names.json
target/release/examples/move_policy extract <maps> <rows>/<division> <division-folder>
```

Do this for each of the seven folders. Each command writes
`<rows>/<division>.jsonl` and `<rows>/<division>.f32`. Then fit, with
NumPy and SciPy:

```text
python crates/awbrn-ai-diagnostics/examples/fit_move_policy.py \
  --names <rows>/names.json --rows <rows>/<division>... \
  --max-train 120000 \
  --blend <fit>/blend.json --pure <fit>/pure.json --summary <fit>/summary.json
```

`move_policy score <blend|pure> <strength> <rows>/<division>` gives the top-1
agreement of the built-in model as the policy reads it in play.
