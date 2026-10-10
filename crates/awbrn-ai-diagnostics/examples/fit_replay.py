"""Fit experimental replay scores. Requires NumPy and SciPy.

Keep all rows of a game in one fold. Give each game the same weight.
Use validation loss to select a fit. Reserve new cups for the final check.
"""

import argparse
import collections
import hashlib
import json
from pathlib import Path

import numpy as np
from scipy.optimize import minimize, minimize_scalar
from scipy.special import expit

BASE = ["turn_index", "material_delta", "income_delta", "own_bank",
        "unit_count_delta", "capture_progress_delta", "front_position_delta",
        "immediate_threat_safety_delta", "deferred_threat_safety_delta"]
CORE = [name for name in BASE if name != "front_position_delta"]
POSITION = ["material_front", "property_capture", "production", "contest"]
POWER = ["power_meter_army", "cop_ready_army", "scop_ready_army", "eagle_refresh"]


def read(directories):
    rows, sources = [], []
    for directory in directories:
        directory = Path(directory)
        features = [json.loads(line) for line in (directory / "features.jsonl").read_text().splitlines()]
        positions = [json.loads(line) for line in (directory / "positions.jsonl").read_text().splitlines()]
        assert len(features) == len(positions)
        for feature, position in zip(features, positions):
            assert feature["match_seed"] == position["game"]
            assert feature["turn_index"] == position["turn"]
            assert feature["mode"] == position["mode"]
            assert feature["winner"] == position["winner"]
            assert feature["perspective_seat"] == position["seat"]
            if feature["mode"] != "fog-visible":
                continue
            values = {**feature["features"], **position["extra"]}
            terms = position["terms"]
            values["v5_score"] = sum(v for k, v in terms.items() if k not in ("front", "exposure")) + .25 * terms["front"]
            rows.append((position["game"], position["map"], position["winner"], values))
        sources.append({"directory": str(directory), "positions_sha256": hashlib.sha256(
            (directory / "positions.jsonl").read_bytes()).hexdigest(),
            "features_sha256": hashlib.sha256((directory / "features.jsonl").read_bytes()).hexdigest()})
    games = [row[0] for row in rows]
    counts = collections.Counter(games)
    weights = np.array([1 / counts[game] for game in games])
    return rows, np.array(games), np.array([row[2] for row in rows], dtype=float), weights, sources


def fit(x, y, weights, penalty, positive=False):
    weights = weights / weights.sum()
    mean = weights @ x
    scale = np.maximum(np.sqrt(weights @ ((x - mean) ** 2)), 1e-9)
    z = (x - mean) / scale
    def objective(beta):
        score = beta[0] + z @ beta[1:]
        loss = weights @ (np.logaddexp(0, score) - y * score)
        residual = weights * (expit(score) - y)
        return loss + penalty * (beta[1:] @ beta[1:]) / 2, np.r_[
            residual.sum(), z.T @ residual + penalty * beta[1:]]
    result = minimize(objective, np.zeros(x.shape[1] + 1), jac=True, method="L-BFGS-B",
                      bounds=([(None, None), (None, None)] + [(0, None)] * (x.shape[1] - 1)) if positive else None,
                      options={"maxiter": 2000, "gtol": 1e-8, "ftol": 1e-12})
    assert result.success, result.message
    coefficients = result.x[1:] / scale
    return float(result.x[0] - mean @ coefficients), coefficients


def metrics(y, score, weights):
    return {"log_loss": float(weights @ (np.logaddexp(0, score) - y * score) / weights.sum()),
            "accuracy": float(weights @ ((score >= 0) == y) / weights.sum()),
            "brier": float(weights @ ((expit(score) - y) ** 2) / weights.sum())}


def folds(rows, games, repeat):
    assignment = {}
    by_map = collections.defaultdict(set)
    for game, map_id, _, _ in rows:
        by_map[map_id].add(game)
    for map_id, members in sorted(by_map.items()):
        ordered = sorted(members, key=lambda game: hashlib.sha256(f"{repeat}:{map_id}:{game}".encode()).digest())
        assignment.update({game: index % 5 for index, game in enumerate(ordered)})
    return np.array([assignment[game] for game in games])


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--train", nargs="+", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--validate", nargs="+")
    parser.add_argument("--model")
    args = parser.parse_args()
    output = Path(args.output)
    output.mkdir(parents=True, exist_ok=True)
    rows, games, y, weights, sources = read(args.train)
    fingerprint = hashlib.sha256(json.dumps(sources, sort_keys=True).encode()).hexdigest()
    if args.validate:
        model = json.loads(Path(args.model).read_text())
        assert model["corpus_fingerprint"] == fingerprint, "model training source differs"
        holdout, hg, hy, hw, hs = read(args.validate)
        assert not set(games) & set(hg), "training and holdout share games"
        names = {**model["coefficients"], **model.get("extra_coefficients", {})}
        scores = np.array([model["intercept"] + sum(values[name] * coefficient
                          for name, coefficient in names.items()) for _, _, _, values in holdout])
        leaf_scores = np.array([sum(values[name] * coefficient for name, coefficient in names.items()
                             if name != "turn_index") for _, _, _, values in holdout])
        train_stock = np.array([values["v5_score"] for _, _, _, values in rows])
        temperature = np.exp(minimize_scalar(lambda log_t: metrics(y, train_stock / np.exp(log_t), weights)["log_loss"],
                             bounds=(np.log(250), np.log(1e6)), method="bounded").x)
        holdout_stock = np.array([values["v5_score"] for _, _, _, values in holdout])
        stock_scores = holdout_stock / temperature
        loss_deltas = (np.logaddexp(0, scores) - hy * scores
                       - np.logaddexp(0, stock_scores) + hy * stock_scores)
        game_deltas = np.array([loss_deltas[hg == game].mean() for game in sorted(set(hg))])
        half_width = 1.96 * game_deltas.std(ddof=1) / np.sqrt(len(game_deltas))
        holdout_maps = np.array([r[1] for r in holdout])
        training_maps = {r[1] for r in rows}
        unseen = np.array([map_id not in training_maps for map_id in holdout_maps])
        report = {"training_sources": sources, "holdout_sources": hs,
                  "model_sha256": hashlib.sha256(Path(args.model).read_bytes()).hexdigest(),
                  "games": len(set(hg)), "rows": len(hg), "metrics": metrics(hy, scores, hw),
                  "calibration_removed_prediction": metrics(hy, leaf_scores, hw),
                  "stock_temperature": temperature,
                  "stock_frozen_calibration": metrics(hy, stock_scores, hw),
                  "stock_shipped_calibration": metrics(hy, holdout_stock / 27486, hw),
                  "paired_game_loss_difference": {"mean": float(game_deltas.mean()),
                      "descriptive_interval": [float(game_deltas.mean() - half_width), float(game_deltas.mean() + half_width)]},
                  "unseen_maps": sorted(set(holdout_maps[unseen].tolist())),
                  "unseen_map_metrics": metrics(hy[unseen], scores[unseen], hw[unseen]),
                  "unseen_map_stock": metrics(hy[unseen], stock_scores[unseen], hw[unseen])}
        (output / "holdout.json").write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps(report["metrics"]), flush=True)
        return
    candidates = [
        ("safe-core", CORE, .1),
        ("funds-front", CORE + POSITION[:1], .1),
        ("properties", CORE + POSITION, .1),
        ("power-meter", CORE + POSITION + POWER[:1], .1),
        ("power-ready", CORE + POSITION + POWER[:3], .1),
        ("eagle", CORE + POSITION + POWER, .1),
        ("weak-regularization", CORE + POSITION + POWER, .03),
        ("strong-regularization", CORE + POSITION + POWER, .3),
        ("no-deferred-threat", [n for n in CORE + POSITION + POWER if n != "deferred_threat_safety_delta"], .03),
        ("l2-01", CORE + POSITION + POWER, .01),
        ("l2-003", CORE + POSITION + POWER, .003),
        ("l2-001", CORE + POSITION + POWER, .001),
        ("l2-0003", CORE + POSITION + POWER, .0003),
        ("unregularized", CORE + POSITION + POWER, 0.0),
        ("positive-01", CORE + POSITION + POWER, .01),
        ("positive-003", CORE + POSITION + POWER, .003),
        ("without-powers", CORE + POSITION, .01),
    ]
    reports = []
    for label, names, penalty in candidates:
        positive = label.startswith("positive-")
        x = np.array([[values[name] for name in names] for _, _, _, values in rows])
        predictions = []
        for repeat in range(3):
            fold = folds(rows, games, repeat)
            scores = np.zeros(len(rows))
            for number in range(5):
                train, test = fold != number, fold == number
                intercept, coef = fit(x[train], y[train], weights[train], penalty, positive)
                scores[test] = intercept + x[test] @ coef
            predictions.append(metrics(y, scores, weights))
        map_scores = np.zeros(len(rows))
        maps = np.array([r[1] for r in rows])
        for map_id in sorted(set(maps)):
            train, test = maps != map_id, maps == map_id
            intercept, coef = fit(x[train], y[train], weights[train], penalty, positive)
            map_scores[test] = intercept + x[test] @ coef
        intercept, coef = fit(x, y, weights, penalty, positive)
        coefficients = dict(zip(names, map(float, coef)))
        model = {"schema_version": 1, "source_report_fingerprint": fingerprint,
                 "corpus_fingerprint": fingerprint, "mode": "fog-visible", "intercept": intercept,
                 "coefficients": {n: coefficients.get(n, 0.0) for n in BASE},
                 "extra_coefficients": {n: coefficients[n] for n in POSITION + POWER if n in coefficients}}
        assert coefficients["material_delta"] > 0
        (output / f"{label}-model.json").write_text(json.dumps(model, indent=2) + "\n")
        report = {"name": label, "features": names, "l2": penalty, "positive": positive,
                  "cv": {key: float(np.mean([p[key] for p in predictions])) for key in predictions[0]},
                  "leave_one_map_out": {key: float(np.mean([metrics(y[maps == m], map_scores[maps == m],
                      weights[maps == m])[key] for m in set(maps)])) for key in predictions[0]}}
        reports.append(report)
        print(json.dumps(report), flush=True)
    result = {"sources": sources, "games": len(set(games)), "rows": len(rows),
              "folds": "3 repeats of 5 folds, grouped by game and balanced per map",
              "candidate_reports": reports, "selection": "minimum grouped validation loss",
              "best": min(reports, key=lambda r: r["cv"]["log_loss"])["name"],
              "stop_threshold": .002, "stop_consecutive_failures": 3}
    (output / "fits.json").write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()
