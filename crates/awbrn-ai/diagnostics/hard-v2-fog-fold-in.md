# Hard v2 Fog evaluation

The current Hard profile keeps ID `ai-hard-v2`. It uses the production weights with one change: `conceal=2.0`. The vision score is zero when fog is off, so Standard play stays the same. The profile fingerprint is `d5e39223474b7cd3`. The unchanged comparison configuration has fingerprint `992c47dd6c7609c0`.

The conceal value was selected on development maps. The first holdout run used a diagnostic candidate with the same production weights and conceal value. The current Hard profile later replayed the same maps, seeds, and seat orders. All 112 keyed outcomes matched the first run, including match points, terminal day, and terminal reason. This replay checks the implementation; it is not a new holdout.

## Global League Fog holdout

The holdout used seven Fog maps, eight paired seeds per map, both seat orders, and a 35-day limit. All 56 pairs and 112 matches were valid. Hard v2 scored 101 of 112 match points: 99 wins, 9 losses, and 4 draws. The paired mean was `+0.803571` against the old production configuration. Every map mean was positive.

| Map ID | Pair mean |
| ---: | ---: |
| 75796 | `+1.0000` |
| 80008 | `+0.8750` |
| 143354 | `+1.0000` |
| 144739 | `+0.5000` |
| 166637 | `+0.9375` |
| 170819 | `+0.4375` |
| 176149 | `+0.8750` |

Forty-eight matches ended before day 35, and Hard v2 won all of them. Of the 64 matches that reached day 35, Hard v2 won 51, lost 9, and drew 4. The game awards a win to the sole property leader at the day limit; a 28–14 property lead is a win. There were no invalid commands, refusals, preflight rejections, or unrealizable plays. The current-profile replay had a 34.0 ms p95 complete-turn time and a 67.3 ms maximum.

The [holdout plan](hard-v2-fog-holdout-profile-replay-20260923-plan.json) uses the [Fog map manifest](../../../assets/ai-diagnostics/global-league-pool/fog-holdout-manifest.json). Run it from the repository root with a new output directory:

```text
cargo run -p awbrn-ai-diagnostics --bin run-external-plan -- assets/ai-diagnostics/global-league-pool/fog-holdout-manifest.json crates/awbrn-ai/diagnostics/hard-v2-fog-holdout-profile-replay-20260923-plan.json target/hard-v2-fog-holdout-check
```

## External Fog map

Map 65213 was outside the Global League pool and was untouched when it was first used. The comparison used eight pairs, both seat orders, and the same 35-day limit. The current Hard behavior won all 16 matches against the old production configuration. Thirteen games ended before day 35; three reached the limit. All 16 outcomes were reproduced after the profile was folded into `ai-hard-v2`. This one-map check was not used to change weights.

The [external plan](hard-v2-external-fog-65213-plan.json) uses the [external map manifest](../../../assets/ai-diagnostics/external-reserve/manifest.json). Run it from the repository root:

```text
cargo run -p awbrn-ai-diagnostics --bin run-external-plan -- assets/ai-diagnostics/external-reserve/manifest.json crates/awbrn-ai/diagnostics/hard-v2-external-fog-65213-plan.json target/hard-v2-external-fog-check
```

## Release controls

Tests lock the profile ID and fingerprint, check a complete Standard match against the old production configuration, and check legal Fog matches in both seat orders. The server test checks that the Hard tier seats `ai-hard-v2`.
