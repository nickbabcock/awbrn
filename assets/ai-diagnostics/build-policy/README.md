# Build model fitted to human builds

`human-v2a-model.json` is a conditional logit fitted to the build decisions
of human players in Amarriner Cups 1 to 4, Division 1, and the Global League
games. Cups 5 and 6 were the holdout: top-1 0.755 and log loss 0.663. The
greedy policy of `planner-v8` loads it (`awbrn_ai::build_policy`).

- One decision is one build at one production site, with the board
  immediately before the build. The choices are the legal kinds at the site.
- `BuildContext::site` (`awbrn_ai::build_features`) calculates the features. A
  model reads only the features that its file names.
- The greedy scores still decide whether and when a site builds. The model
  chooses the kind only: the chosen kind gets the highest floored greedy
  score of the legal kinds at the site, and the other kinds get zero.
- The model takes the kind with the highest utility. It does not sample.
- `planner-v8` adds a range term to the utility of each indirect kind: 2.5
  times the natural logarithm of the ratio of its maximum attack range under
  our commander to the range of its profile (`build_policy::range_bias`).
  Only a commander that changes the range, such as Grit or Max, is
  changed.

The fit includes the games on the maps of `tournament-sealed/manifest.json`.
Most rows of the corpus are on these maps, so these maps are not a clean
holdout for the model.

## Make the model again

The input is the AWBW replay archives (ZIP files named with the game ID) of
the Division 1 games of Amarriner Cups 1 to 6 and of 10 Global League
Standard games with a rating above 1500, and the AWBW map file of each map
(`<maps>/<map ID>.json`), as the AWBW map-info API gives it. They are not
in the repository.

Extract the rows. Each command writes one row file:

```text
cargo build --release -p awbrn-ai-diagnostics --example build_policy
target/release/examples/build_policy names <rows>/names.json
target/release/examples/build_policy extract <maps> <rows>/cup-01.jsonl <cup-01-division>
```

Do the same for Cups 2 to 6 and for the Global League games
(`<rows>/global.jsonl`). The extractor writes 69,601 build rows. A game that
the replay importer cannot play to the end keeps the rows before the error.

Install NumPy and SciPy in a Python environment and fit:

```text
python crates/awbrn-ai-diagnostics/examples/fit_build_policy.py \
  --names <rows>/names.json \
  --train <rows>/cup-01.jsonl <rows>/cup-02.jsonl <rows>/cup-03.jsonl \
          <rows>/cup-04.jsonl <rows>/global.jsonl \
  --holdout <rows>/cup-05.jsonl <rows>/cup-06.jsonl \
  --reference assets/ai-diagnostics/build-policy/human-v2a-model.json \
  --model <fit>/model.json --summary <fit>/fit-summary.json
```

The fit uses 49,412 training rows and 19,870 holdout rows. It does not use
251 training rows and 68 holdout rows, where the built kind is not legal at
the site in the imported state. The selection keeps the groups
`funds_after`, `prey`, `co_combat` and `shares`, with the penalty 0.01. On
the holdout, top-1 is 0.755 and log loss 0.663. The share of the argmax
choice is 0.313 for the tank (human 0.189), 0.031 for the anti-air (0.054)
and 0.0004 for the artillery (0.038).

With Python 3.13.9, NumPy 2.5.4 and SciPy 1.18.1, these commands give the
values of `human-v2a-model.json` exactly: all 2,137 numbers are equal. Only
the layout of the file is different. Other versions of these libraries can
give small differences.
