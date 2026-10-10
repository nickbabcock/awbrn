# Global League map pool

This folder holds 42 two-player maps from a user-supplied AWBW Global League
search snapshot. The snapshot SHA-256 is in `manifest.json`.

The map files use the official AWBW map-info API. This API includes terrain
and predeployed units. The text-map page exports terrain only. The map parser
reads the API's column-major terrain grid.

The manifest records each map's source URL, map page, mode, rank, size,
factions, deployment count, and fingerprints. Its source paths are relative to
this folder.

The original split had 21 development maps and 21 holdout maps. Earlier v6
commander checks used seven of those holdout maps with fog disabled. The
manifest now marks those maps as evaluated. Do not use them for a final
holdout result. The remaining 14 maps stay sealed in
`sealed-holdout-manifest.json`. Do not use their results to choose or tune an
agent. The seven evaluated maps are listed in `evaluation_ledger` in
`manifest.json`.

All seven Fog maps in the original holdout have been used in those v6 checks.
There is no untouched Fog holdout in this pool. Source metadata for the 14
sealed maps includes Standard and High Funds categories. The match runner uses
its default setup for every map: fog disabled, 0 starting funds, and 1,000
income per city. It does not apply High Funds rules.

`fog-holdout-manifest.json` keeps the map identities used by the earlier
checks. It is a historical record. It is not an untouched holdout manifest.

The original split used the fixed label
`awbrn-global-league-holdout-v1`. The selector hashes
`awbrn-global-league-holdout-v1:<AWBW ID>`, then takes the lowest hashes in
each mode, rank, and size group. A map is small when it has 420 tiles or
less. The manifest keeps the original bucket counts and reports the remaining
holdout counts.

All 42 files passed `AwbwMap::parse_json`. The map registry loaded their
source and normalized fingerprints and built a canonical two-seat state for
each map. Map 61748 also matched the checked-in source and normalized
fingerprints exactly.
