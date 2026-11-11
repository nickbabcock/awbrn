# Experiments for planner-v8

This page records what was tried against `planner-v7` in November 2026, and
the result. Use it before you start an experiment, so that you do not repeat
a neutral or negative one. Most configurations of these experiments are not
in the repository. Only `planner-v7-wide` and `planner-v8` are.

Unless the text tells otherwise, a result is the mean pair differential
against the baseline that the text names, with the descriptive 95% half
width, in the Andy mirror on the 14 development maps, with a 35-day limit. A
"single run" has no rerun on new seeds. A gain below about +0.1 on one run
of 280 pairs is not proven until a run on new seeds or on a holdout agrees.

## What planner-v8 kept

Each step is measured against the step before it, or against `planner-v7`.

| Part | Result |
| --- | --- |
| No work threshold only | +0.089 ± 0.057 against v7 (280 pairs, single run) |
| Wide search (no threshold, about 4 times the plans and reply checks) | +0.192 ± 0.048 against v7 (560 pairs, two seed sets). Turn p95 about 770 ms, against 250 ms |
| Build model `human-v1` on the wide search | +0.443 ± 0.072 against the wide search (280 pairs); +0.571 ± 0.048 against v7 (560 pairs, two seed sets) |
| Ferry of spare capturers | +0.036 ± 0.030 against the step before (280 pairs). All of the gain is on map 160882 (+0.475) |
| Build model `human-v2a` | Andy: +0.596 ± 0.069 against v7, where `human-v1` had +0.614 on the same seeds. Commander matrix (616 pairs): +0.532 ± 0.048 against +0.494 ± 0.049. Grit mirror: +0.44 against +0.22 (56 pairs); on new seeds +0.40 against +0.21 |
| Super power policy | +0.246 ± 0.068 against the ferry configuration with `human-v1` (280 pairs, single run). Mirrors, 56 pairs: Eagle +0.48, Drake +0.02, Lash 0.00, Grit −0.21 (so Grit keeps the normal rule); Kindle, Max and Sami play the same games |
| Capture bonus of the commander | Sami mirror +0.03: neutral, kept as part of the commander effects |
| Power placement (first by default; a power that lets units act again last) | +0.105 ± 0.074 against the step before (280 pairs). Mirrors, 56 pairs: Kindle +0.304 ± 0.151, Lash +0.116, Max +0.098, Sami +0.045, Grit +0.018, Drake +0.018, Eagle −0.036 |
| March plan (Sami super power), second version | Sami mirror: +0.143 ± 0.155 against the same configuration without it, +0.107 ± 0.129 against the first version, +0.665 ± 0.097 against v7 (112 pairs each) |
| Range term of the build model, commander-aware threat maps, firing positions of indirect units | Grit mirror against v7: +0.563 and +0.714 on two seed sets (56 pairs each), where the configuration without them had +0.384 and +0.375. Max mirror: +0.063 ± 0.135 and +0.009 ± 0.183 against the configuration without them. Andy, the threat and firing-position terms only: +0.014 ± 0.021 (280 pairs) |

`planner-v8` against `planner-v7`, on the code of the Hard profile:

- Andy mirror: +0.791 ± 0.050, 88.9% wins (280 pairs).
- Mirrors, 56 pairs: Max +0.786, Eagle +0.777, Lash +0.679, Kindle +0.670,
  Drake +0.652, Sami +0.607, Grit +0.563.
- On an earlier build that differs only in two later fixes: with an agent
  built again in each turn, as the host does, +0.782 ± 0.050 (Andy, 280
  pairs); Andy mirror in fog, +0.696 ± 0.143, 83.9% wins (56 pairs).

## Neutral or negative

Against the wide search with build model `human-v1`, 280 pairs each unless
the text tells otherwise:

- Counter veto on the kind of the build model: +0.009 ± 0.033; strict
  version +0.018 ± 0.041. Human players build a b-copter in 49 of the 52 rows
  where the veto applies, so human data does not support it.
- Indirect tactics, first version: +0.021 ± 0.028; Grit mirror
  −0.009 ± 0.047.
- Build rules nearer to the human mix of kinds: −0.127 ± 0.081. Sampling at
  temperature 1: about −0.23. With the indirect tactics: +0.005 ± 0.071
  against the same mix without them.
- Build model v2 on 20,682 rows, without the reserved tournament maps:
  −0.127 ± 0.079. Its held-out top-1 was equal to `human-v1` (76.0% against
  75.8%). Held-out accuracy on human builds does not predict game strength.
- Lead push, on top of the ferry: no gain; not kept.
- First-contact tactics (a study of 368 games, not a paired run): the
  losses are decided in the first fights on days 9 to 10, but a trace study
  found no policy fault (the bot takes 88% to 94% of the attacks with a
  positive value). No fix was made.

Later experiments, against the configuration that each item names, 280
pairs unless the text tells otherwise. The first three are against the
configuration with the ferry:

- `human-v2a` refit on all rows: −0.023 ± 0.082 in the Andy mirror. It was
  kept for its commander results (see above).
- Wider search again (limits doubled): +0.014 ± 0.074, with 30% more turn
  time.
- Joint purchases over the turn: −0.014 ± 0.074.
- Build model `human-v3`: +0.015 ± 0.060 against `human-v2a` (167 pairs,
  stopped). Of 14 generic commander features, only the attack and defense
  without a power passed the selection. Human builds cannot teach commander
  adaptation: the range changes only for Max in the corpus, and the corpus
  has no Grit games. Adaptive builds need another source, for example
  self-play results.
- Power placement, earlier rules: the second rule fixed the Andy and Grit
  timing but made Eagle worse; the third rule was still worse for Eagle.
- Headquarters guard, against the combined configuration: neutral (map
  169382 +0.05). It changed 1 of 10 lost pairs on that map.
- Build term from all commander effects (attack, defense, range and
  movement): Grit mirror +0.491, +0.384 and +0.393 at the strengths 1.5, 3
  and 5, where the configuration without it had +0.384 (56 pairs). More
  indirect units alone do not fix Grit. With the threat and firing-position
  terms it helped Grit (+0.18 pooled), but Sami lost −0.277 ± 0.193 on new
  seeds (56 pairs), because `human-v2a` already reads the attack and
  defense. The range-only term replaced it. Without a build term, the threat
  and firing-position terms gave Grit no real gain (+0.420 pooled).

Earlier experiments against `planner-v7`, 280 pairs each unless the text
tells otherwise:

- Joint orders (an earlier experiment, not the current `planner-v8`):
  +0.029 ± 0.060.
- Repair plans: −0.078 ± 0.110 (103 pairs, stopped). Second version: +0.066
  on the first run, −0.041 on new seeds; pooled +0.013 ± 0.040 (560 pairs).
- Lab as headquarters on maps without one: −0.021 ± 0.045.
- Leaf score fitted to self-play games of v7: −0.186 ± 0.076. A better
  outcome predictor on the games of one policy is not a better planning
  score.
- Day-limit tile term: +0.005 ± 0.012 (275 of 280 pairs the same).
- Build-swap plans: −0.130 ± 0.078. Endgame plans: 0 of 17 plans won in a
  short check.
- Human Cups 3 to 4 refit with a tile margin as the leaf score: pooled
  +0.054 ± 0.048 on development maps (560 pairs), −0.004 ± 0.067 on the 14
  Standard and High Funds holdout maps. The development gain was a random
  result: about 10 candidates were tried on the same maps.
- Three-turn rollouts in the reply check: +0.129 ± 0.074, but against the
  same work allowance with one reply, +0.007 ± 0.110 (135 pairs, stopped).
  The gain came from the extra work, not from the depth.
- The search agent at a large budget is not a useful reference player: its
  budget did not bind, and it lost 4 of 4 short games to v7.

## Known weak spots of planner-v8

- Map 169382: the weakest map (+0.275 in the Andy mirror of the combined
  configuration). Of 14 losses, 9 are by property count at the day limit and
  5 are headquarters captures.
- Map 172238: all 10 losses are in seat 0. The cause of this difference
  between the seats is not known.
- Grit and Sami are the weakest mirrors (+0.563 and +0.607).
- Turn time: native p95 about 0.85 to 1.15 s for the combined configuration,
  against 0.25 s for v7. Wasm p95 2.2 s, maximum 2.8 s. Many chained AI
  turns in one server wake could pass a CPU limit; this is not measured.
- Fog is measured only on the Andy mirror (56 pairs).
- Deferred review items: the ferry can take a unit that guards the
  headquarters, and the seed power is put first in the plan.
- The host builds a new agent for each turn, so the march plan loses its
  log of the power meter of the earlier turns. The runs with an agent built
  again in each turn measure this play.

## Open ideas

- An attack term for vehicles only in the build model, for Sami: Sami
  players buy somewhat more indirect units.
- A conflict rule at the level of cells for the opening plays.
- A count and classification of the losses, before work on the evaluator
  or on plans over more than one turn.
