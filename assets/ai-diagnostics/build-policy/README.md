# Build model fitted to human builds

`human-v2a-model.json` is a conditional logit fitted to the build decisions
of human players in Amarriner Cups 1 to 4, Division 1, and the Global League
games. Cups 5 and 6 were the holdout: top-1 0.755 and log loss 0.663. The
greedy policy of `planner-v8` loads it (`awbrn_ai::build_policy`).

- One decision is one build at one production site, with the board just
  before the build. The choices are the legal kinds at the site.
- `awbrn_ai::build_policy::BuildContext::site` calculates the features. A
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

The tools that extract the rows from the replays and fit the model are not
in the repository yet.
