# Replay score tools

The model coefficients use the feature values in `features.jsonl`. The bank
feature is the full number of funds. The live score uses half the difference
between the two seat logits. This operation changes the bank feature to half
the bank difference.

The frozen v6 model is `refit-positive-003-model.json`. The selected training
check is in `fit-summary.json`. The Cup 5 and Cup 6 check is in `holdout.json`.
The [earlier study](https://github.com/nickbabcock/awbrn/tree/1c5dfd302e5eb59c5f7c1051addece8c73728b47/assets/ai-diagnostics/human-evaluator)
contains the rejected models and old experiments. Its Python refit games
used the old bank scale. Its Eagle games used the old COP rule.

Import the replay archives:

```text
cargo run --release -p awbrn-ai-diagnostics --example awbw_evaluator -- \
  extract <division-directory> <map-directory> <output-directory>
```

Install NumPy and SciPy in a Python environment. Fit the v6 score from the
extracted Cup 3 and Cup 4 directories. The fit uses nonnegative position
coefficients and an L2 penalty of 0.003. It writes `model.json` and
`fit-summary.json`:

```text
python crates/awbrn-ai-diagnostics/examples/fit_replay.py \
  --train <cup-03-directory> <cup-04-directory> --output <model-directory>
python crates/awbrn-ai-diagnostics/examples/test_fit_replay.py
```

Check the model on Cup 5 and Cup 6:

```text
python crates/awbrn-ai-diagnostics/examples/fit_replay.py \
  --train <cup-03-directory> <cup-04-directory> \
  --validate <cup-05-directory> <cup-06-directory> \
  --model <model-file> --output <check-directory>
```

Run `awbw_evaluator puzzles <model-file> <output-file>` for the tactical
checks. Run `awbw_evaluator power-probe <model-file> <output-file>` to check
Eagle COP and SCOP. The power test also runs in the Rust test suite.

The [coverage summary](../sprt/results/planner-v6-coverage-summary.json) records
112 games on seven additional maps without fog. Each of the four commander
matchups has 14 pairs. These samples do not establish strength for every
commander or for games against people.
