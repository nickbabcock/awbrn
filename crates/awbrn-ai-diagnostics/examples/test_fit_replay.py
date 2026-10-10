"""Check the bank feature at the input to the fitter."""

import json
from pathlib import Path
import tempfile
import unittest

from fit_replay import read


class BankFeatureTest(unittest.TestCase):
    def test_bank_funds_are_not_scaled_before_fitting(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            features = []
            positions = []
            for turn, funds in enumerate([5000, 20000], start=1):
                features.append({
                    "match_seed": 1, "turn_index": turn, "mode": "fog-visible",
                    "winner": True, "perspective_seat": 0,
                    "features": {"own_bank": funds},
                })
                positions.append({
                    "game": 1, "turn": turn, "mode": "fog-visible",
                    "winner": True, "seat": 0, "map": 1,
                    "terms": {"front": 0}, "extra": {},
                })
            for name, rows in [("features", features), ("positions", positions)]:
                (root / f"{name}.jsonl").write_text(
                    "".join(json.dumps(row) + "\n" for row in rows)
                )
            rows, _, _, weights, _ = read([root])
            self.assertEqual([row[3]["own_bank"] for row in rows], [5000, 20000])
            self.assertEqual(weights.tolist(), [0.5, 0.5])


if __name__ == "__main__":
    unittest.main()
