# Day six planner diagnosis

The day six replay uses an observation-based planner. The initial diagnostic
used the full state, so its internal scores and its fixed-line result did not
match the agent's view. The corrected trace builds the same session that
`PlannerAgent` receives.

The full state has funds of 1,000 for enemy player 0 and 10,000 for active
player 1. The observation reports no enemy funds and reports 10,000 for player
1. Reification gives player 0 zero funds and keeps 10,000 for player 1. This
holds with fog disabled. The internal Hard reply therefore uses the projected
state. The external V3 reply observes the authoritative state from the enemy
seat and sees the enemy's own funds.

1. The seed attacks enemy infantry 2 from (14, 9). The reply estimate exposes
   tank 16 and infantry 1, and predicts that infantry 1 will be destroyed.
2. V6 selects the Safety plan that holds infantry 1. The actual initial plan
   still loses the fixed-line V3 reply check. The V6 agent rebuilds its plan
   during the turn after three state mismatches, for four plans in total.
3. A full-state scan of 292 legal single orders found 20 with a positive
   external V3 swing. All 20 move infantry 4. None beats the Safety plan in
   the Hard reply check. Paired support and Safety lines show why the two
   orders must appear in the same prefix. These scans are retained as
   full-state legacy evidence.
4. V7_DAY6 selects a Clearance plan. Infantry 4 moves from the neutral city
   at (16, 8) to (17, 6), and infantry 1 stays at (4, 10). The move frees tank
   16 to use (16, 8) and attack enemy tank 14 at (16, 9). The V7 agent also
   builds four plans after three state mismatches. The external reply swing
   remains positive for all three seeds. The enemy city at (15, 9) belongs to
   enemy infantry 2, so this plan does not stop a capture.

The fixed-line column forces the initial plan from the observation-based
trace, then runs a V3 reply against the authoritative state. The full-turn
column runs the planner through the observation and execution lifecycle, then
runs the same V3 reply. The agent can rebuild its plan when the observed state
differs from the state it predicted.

| Agent seed | Config | Seed score | Initial line score | Reply loss | After Hard | Fixed-line V3 swing | Full-turn V3 swing | Replan mismatches |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | V6 | 579 | -316 | 4,965 | -22,197 | -800 | -1,530 | 3 |
| 2 | V6 | 88 | -702 | 4,965 | -23,161 | -800 | -800 | 3 |
| 3 | V6 | 141 | -264 | 4,965 | -21,433 | -800 | -800 | 3 |
| 1 | V7_DAY6 | 579 | 1,596 | 2,500 | -19,677 | 4,640 | 4,640 | 3 |
| 2 | V7_DAY6 | 88 | 1,648 | 2,500 | -19,921 | 4,640 | 4,640 | 3 |
| 3 | V7_DAY6 | 141 | 1,648 | 2,500 | -18,847 | 4,640 | 4,640 | 3 |

The V6 fixed-line mean is -800. Its full-turn mean is -1,043. The V7_DAY6
fixed-line and full-turn means are both 4,640. The V7_DAY6 line uses 322 work
units and leaves 17 of 32 nodes. The work limit stays at 2,000. The generator
probes at most three blockers and screens at most 24 orders for each blocker.
Each probe and screen counts as work.

The safer turn has this path through the planner:

1. V6 does not generate the joint move and hold. V7_DAY6 generates it.
2. The V7 line passes the initial filter. Its score is above the seed score
   less the unchanged 5,000-fund reply window in every seed.
3. It reaches the unchanged four-alternative simulated reply check.
4. It wins that check in every seed. Its after-reply value is higher than
   the V6 Safety line by 2,520, 3,240, and 2,586 funds.

The complete observation-based trace includes the actual and visible funds,
the initial plan, accepted commands, scores, work counts, and node counts in
[`day6-investigation/observed-v6-v7-comparison.log`](day6-investigation/observed-v6-v7-comparison.log).
The earlier full-state scans and traces are retained with `full-state-legacy`
in their names in the same directory.

Run the compact trace with:

```sh
cargo test -p awbrn-ai trace_day6_v6_and_v7_clearance --lib -- --ignored --nocapture
```

Run the day six replay check with:

```sh
cargo test -p awbrn-ai --test replay_regressions day_six_does_not_trade_a_tank_for_a_capture_stop
```

`PlannerConfig::V6.fingerprint()` remains `26569da0906e9bc4`. The freeze
manifest records the wrapped agent identity fingerprint,
`7796830904b48417`.
