# Experiments that were not kept

These experiments for `planner-v8` (November 2026) were neutral or negative.
Do not repeat them without a new reason. A result is the mean pair
differential with the descriptive 95% half width, Andy mirror, 14
development maps, 280 pairs, unless the item tells otherwise. A gain below
about +0.1 on one run is not proven until new seeds or a holdout agree.
`human-v1` is the first build model, which `human-v2a` replaced; it is not
in the repository.

Against the wide search with build model `human-v1`:

- Counter veto on the kind of the build model: +0.009 ± 0.033; strict
  +0.018 ± 0.041. Human players build the vetoed b-copter in 49 of 52 rows.
- Indirect tactics, first version: +0.021 ± 0.028; Grit mirror
  −0.009 ± 0.047.
- Build rules nearer to the human mix of kinds: −0.127 ± 0.081; sampling at
  temperature 1 about −0.23.
- Build model v2 on 20,682 rows without the reserved tournament maps:
  −0.127 ± 0.079, with the same held-out top-1 as `human-v1`. Held-out
  accuracy on human builds does not predict game strength.
- Lead push on top of the ferry: no gain.
- First-contact tactics: a trace study found no policy fault (the bot takes
  88% to 94% of the attacks with a positive value).

Against the wide search with `human-v1` and the ferry:

- Wider search again (limits doubled): +0.014 ± 0.074, 30% more turn time.
- Joint purchases over the turn: −0.014 ± 0.074.
- Build model `human-v3` with generic commander features: +0.015 ± 0.060
  against `human-v2a` (167 pairs). Human builds cannot teach commander
  adaptation: in the corpus the range changes only for Max, and there are no
  Grit games.

Against later configurations:

- Two earlier rules for the position of a power in the turn: worse for
  Eagle.
- Headquarters guard: neutral (map 169382 +0.05).
- Build term from all commander effects (attack, defense, range, movement):
  Grit mirror +0.491, +0.384, +0.393 at strengths 1.5, 3, 5 against +0.384
  without it (56 pairs). With the threat terms it helped Grit, but Sami lost
  −0.277 ± 0.193 (56 pairs), because `human-v2a` already reads attack and
  defense. `planner-v8` uses the range-only term.

Against `planner-v7`:

- Joint orders (not the current `planner-v8`): +0.029 ± 0.060.
- Repair plans: −0.078 ± 0.110 (103 pairs); second version pooled
  +0.013 ± 0.040 (560 pairs).
- Lab as headquarters on maps without one: −0.021 ± 0.045.
- Leaf score fitted to self-play games of v7: −0.186 ± 0.076. A better
  outcome predictor on the games of one policy is not a better planning
  score.
- Day-limit tile term: +0.005 ± 0.012. Build-swap plans: −0.130 ± 0.078.
- Human Cups 3 to 4 refit with a tile margin as the leaf score:
  +0.054 ± 0.048 on development maps (560 pairs), −0.004 ± 0.067 on the
  holdout.
- Three-turn rollouts in the reply check: +0.129 ± 0.074, but
  +0.007 ± 0.110 (135 pairs) against the same work allowance with one reply.
  The gain probably came from the extra work. A gain from the depth is not
  proven.
- The search agent at a large budget: its budget did not bind, and it lost
  4 of 4 short games to v7.
