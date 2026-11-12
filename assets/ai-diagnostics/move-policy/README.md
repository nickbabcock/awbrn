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
