# Human replay models

The model coefficients use the feature values in `features.jsonl`. The bank
feature is the full number of funds. Do not scale this feature before fitting.
The live score uses half the difference between the two seat logits. This
operation changes the bank feature to half the bank difference.

The old Python fitter used half the bank funds. The saved bank coefficients
have been divided by two to convert them to coefficients per fund.
`bank-coefficient-conversion.json` records the old and new file hashes.
The fitted logits and their prediction metrics do not change. The live bank
score is half its earlier value.

The old refit match, power probe, and puzzle files record the earlier live
score. Their model hashes identify the old files. The Eagle match probes also used
the old COP rule. These files do not measure the corrected production score.
Use the final v6 comparison and timing records for a production decision.

The Rust example imports replay archives and writes the input files:

```text
cargo run --release -p awbrn-ai-diagnostics --example awbw_evaluator -- \
  extract <division-directory> <map-directory> <output-directory>
```

Install NumPy and SciPy in a Python environment. Fit the models from the
extracted Cup 3 and Cup 4 directories:

```text
python crates/awbrn-ai-diagnostics/examples/fit_replay.py \
  --train <cup-03-directory> <cup-04-directory> --output <model-directory>
python crates/awbrn-ai-diagnostics/examples/test_fit_replay.py
```

Check the selected model on Cup 5 and Cup 6:

```text
python crates/awbrn-ai-diagnostics/examples/fit_replay.py \
  --train <cup-03-directory> <cup-04-directory> \
  --validate <cup-05-directory> <cup-06-directory> \
  --model <model-file> --output <check-directory>
```

`calibration_removed_prediction` removes the intercept and turn term from
one seat prediction. It does not compute the live score, which also uses the
rival seat and the material coefficient.
