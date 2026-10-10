# Day 11 planner diagnosis

The day 11 replay test requires a mean army-balance swing of at least −9,500
funds. V7_DAY6 averages −10,547. The first V7 version, which paired a
headquarters block with unit 43's retreat, averages −9,857.

V7_DAY6 generates a headquarters block and separate reroutes for the three
most exposed units. Its safety plans hold units that the reply estimate can
destroy. Infantry 21 is outside those groups. The planner does not combine
the headquarters block with a hold for this cheap exposed unit.

The simulated Hard reply rejected that retreat in seeds 1 and 3. A test-only
planner copy replaced those paired reroutes with up to three headquarters
block and original-tile hold plans. It kept the normal reroute plans. The
planner then chose headquarters block unit 37 at (14,6) and hold unit 21 at
(9,4) in all three seeds. Its actual V3 reply swings were:

| Seed | Army-balance swing |
| --- | ---: |
| 1 | −6,260 |
| 2 | −11,020 |
| 3 | −7,710 |
| Mean | −8,330 |

The diagnosis has four stages:

1. V7_DAY6 does not generate the joint headquarters block and infantry hold.
2. V7's joint line passes the initial reply-window filter in all
   three seeds.
3. The line ranks 1st, 1st, and 4th by initial score. Each rank fits the four
   alternative slots in the simulated Hard reply check.
4. The reply check selects the line in all three seeds. The full agent turn,
   including replanning, passes the replay check.

The production generator makes holds for up to three cheapest exposed units.
It sorts by unit cost, then by the order in the exposed-unit list. It adds no
screens. Its candidates use the existing 32-node budget and 2,000-work limit.
The day 11 mean is −8,330, above the unchanged
−9,500 threshold. The day 6 V7_DAY6 result remains unchanged.

Run the focused V7_DAY6 and V7 trace:

```sh
CARGO_TARGET_DIR=target/v7-day11-investigation cargo test -p awbrn-ai --lib planner::day11_diagnostics::compare_v7_day6_and_v7 -- --ignored --nocapture
```

The checked-in trace is `assets/ai-diagnostics/v7/day11-investigation/v7-day6-v7-comparison.log`.

Run the replay regression suite:

```sh
CARGO_TARGET_DIR=target/v7-day11-investigation cargo test -p awbrn-ai --test replay_regressions -- --nocapture
```
