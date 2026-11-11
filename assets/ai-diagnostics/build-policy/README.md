# Build policy fitted to human builds

`human-v2a-model.json` is a conditional logit fitted to the build decisions
of human players in Amarriner Cups 1 to 4, Division 1, and the Global League
games. Cups 5 and 6 are the holdout. `fit-summary-v2a.json` contains the
selection, the holdout metrics and the build mix.
`planner-v7-wide-build2a-press-cap-scop-place4-march2` uses this model.

`human-v1-model.json` is an earlier fit on the same split. The v2a fit reads
its decision features as the base feature set, and compares its holdout
metrics with it (`--reference`). No configuration uses it.

## Decision and features

One decision is one build at one production site, with the board just before
the build. The choices are the legal kinds at the site. The extractor also
writes one "no build" row for each production site that can build and is
empty at the end of a turn. The v2a fit does not use these rows, and the
model file gives "no build" a very low utility.

`awbrn_ai::build_policy::BuildContext::site` calculates the features. The
extractor and the greedy policy call this function on a session of the
observation of the player. A test makes sure that the two paths give the same
features and the same choices. The extractor writes all features. A model
reads only the features that its file names.

## How the greedy policy uses the model

The weight `build_model` turns on the model. It is off in all earlier
configurations, so their fingerprints do not change.

- The greedy scores still decide whether and when a site builds. The model
  chooses the kind only.
- The chosen kind gets the highest floored greedy score of the legal kinds at
  the site. Thus the build has the same rank against the other orders as the
  build that the greedy policy makes there, and the build floor stays.
- The other kinds at the site get zero, so the policy does not play them.
- The model utilities are not added to the scores. They are logits of a human
  choice, not funds, and they have no common scale with the other orders.
- The model takes the kind with the highest utility. It does not sample.

## The fit

- The fit uses all maps. Cups 1 to 4 and the Global League games are the
  training rows (49,412 build rows). Cups 5 and 6 are the holdout (19,870
  build rows). The model file is the fit on the training rows only.
- The selection uses a cross validation with 6 folds of maps. Each map is in
  one fold only. The script adds one feature group at a time, in a fixed
  order, and keeps a group only when it decreases the cross-validated log
  loss by more than the threshold. The selection kept the kind groups
  `funds_after`, `prey` and `co_combat`, and the decision group `shares`.
  The penalty is 0.01. The commander one-hot group did not pass the
  threshold.
- The commander features are not standardized, and they get a larger
  penalty.

| Holdout, Cups 5 and 6 | human-v1 | human-v2a |
| --- | --- | --- |
| Top-1 | 0.7486 | 0.7550 |
| Log loss | 0.6841 | 0.6630 |
| Tank, argmax share (human 0.1885) | 0.2966 | 0.3130 |
| Anti-air, argmax share (human 0.0543) | 0.0304 | 0.0309 |
| Artillery, argmax share (human 0.0382) | 0.0015 | 0.0004 |

The fit includes the games on the maps of `tournament-sealed/manifest.json`.
Most rows of the corpus are on these maps. Thus these maps are not a clean
holdout for the model.

## Make the data and the model

Extract the rows. Each division writes `rows.jsonl` and `coverage.json`:

```text
cargo build --release -p awbrn-ai-diagnostics --example build_policy
target/release/examples/build_policy extract <maps> <output>/cup-01 <cup-01-division>
target/release/examples/build_policy names <output>/names.json
```

Install NumPy and SciPy in a Python environment and fit:

```text
python crates/awbrn-ai-diagnostics/examples/fit_build_policy_v2.py \
  --names <output>/names.json \
  --train <output>/cup-01 <output>/cup-02 <output>/cup-03 <output>/cup-04 <output>/global \
  --holdout <output>/cup-05 <output>/cup-06 --protocol v1 --folds 6 --name human-v2a \
  --reference assets/ai-diagnostics/build-policy/human-v1-model.json \
  --output <fit-directory> --model <fit-directory>/model.json
```

`build_policy score <model> <rows.jsonl> <output.jsonl>` writes the
probabilities of the Rust evaluation, for a comparison with the fit.
