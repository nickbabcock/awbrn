# Global League map pool

This folder holds 42 two-player maps from a user-supplied AWBW Global League search snapshot. The snapshot SHA-256 is in `manifest.json`.

The map files use the official AWBW map-info API. This API includes terrain and predeployed units. The text-map page exports terrain only. The map parser reads the API's column-major terrain grid.

The manifest records each map's source URL, map page, mode, rank, size, factions, deployment count, and fingerprints. Its source paths are relative to this folder.

The manifest splits the maps into 21 development maps and 21 untouched holdout maps. The split was assigned without using match outcomes from these maps. Do not use holdout results to choose or tune an agent. Map 67945 stays in development because earlier research used it. The other researched maps do not appear in this pool.

`fog-holdout-manifest.json` lists the seven Fog holdout maps with fog enabled. It supports the recorded fog evaluation plan. It does not change the Global League split.

The split uses the fixed label `awbrn-global-league-holdout-v1`. The selector hashes `awbrn-global-league-holdout-v1:<AWBW ID>`, then takes the lowest hashes within fixed mode, rank, and size groups. A map is small when it has 420 tiles or less. The manifest records the bucket counts and prior map IDs.

All 42 files passed `AwbwMap::parse_json`. The map registry loaded their source and normalized fingerprints and built a canonical two-seat state for each map. Map 61748 also matched the checked-in source and normalized fingerprints exactly.
