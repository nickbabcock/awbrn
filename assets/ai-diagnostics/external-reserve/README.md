# External reserve map

This folder holds map 65213, `1vs1 Verdun`, as a separate external reserve. The AWBW preview page lists the S-Rank and Fog of War categories. The map source came from the official AWBW map-info API.

The manifest records the source URLs, mode, rank, factions, setup facts, and fingerprints. The map passed the AWBW map parser and the two-seat normalization validator.

This map stays outside the Global League development and holdout pool. It was untouched before the Hard v2 Fog check. The checked-in plan compares the current Hard profile with the old production configuration. See `crates/awbrn-ai/diagnostics/hard-v2-fog-fold-in.md` for the result and the run command. Do not use this result to tune an agent.
