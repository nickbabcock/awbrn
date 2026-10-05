These archives test AWBW parser compatibility. The Insta test in
`crates/awbw-replay/tests/snapshots.rs` records each archive's entries and the
checksum of its parsed data.

| Archive | Schema case |
| --- | --- |
| `replay_799996_legacy-turn-start.zip` | Turn records omit `nextTurnStart`. |
| `replay_990706_capture-elimination.zip` | Capture vision contains `onElimination`. |
| `replay_1109718_legacy-game-over.zip` | A game-over record omits metadata. |
| `replay_1356213_sonja-masked-hp.zip` | Sonja's hit points are masked. |
| `replay_1598747_null-min-rating.zip` | A game record has a null `min_rating`. |

Some records have inconsistent gameplay data. For example, `1598747` contains
a build that overlaps an existing unit. These fixtures test the parser.
