# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.4", "scipy==1.18.1"]
# ///
"""Fit the move model to the unit orders of human players.

The input is the files of the `move_policy` example: for each prefix,
`<prefix>.jsonl` (one row for each decision) and `<prefix>.f32` (the features
of each legal order of the unit). One decision is one order of one unit. The
model is a conditional logit over the legal orders of the unit, with one
coefficient vector for each unit group.

The split is by map: the validation maps are not in the fit. The games with
fog and the games without fog have separate validation maps. The model reads
all features, with the greedy score features. The script also reports the
agreement of the greedy policy with the human orders on the same decisions.

`--fog` selects the decisions of the fit:

- `no`: only the games without fog.
- `yes`: only the games with fog.
- `pooled`: all games, with one coefficient vector for each group.
- `interact`: all games, with one coefficient vector for each group and one
  more vector that adds to it in the games with fog. The model file holds
  the sum, which is the model for the games with fog.

The unit group of a row is the group that the policy uses in play (the
`group` field of the row). `--eval-model` gives the validation result of a
model file on the same decisions.
"""

import argparse
import hashlib
import json
import math
import os
import sys

# The result of the fit changes with the number of BLAS threads. Set the
# number before NumPy loads. A value in the environment has priority.
os.environ.setdefault("OPENBLAS_NUM_THREADS", "1")

import numpy as np
from scipy.optimize import minimize

GREEDY = ["greedy", "greedy_gap", "greedy_top"]
GROUPS = ["foot", "direct", "indirect", "other"]
FOOT = {"infantry", "mech"}
# The missile has indirect fire, so the policy gives it the indirect
# coefficients.
INDIRECT = {"artillery", "rocket", "piperunner", "missile"}
DIRECT = {"recon", "tank", "md-tank", "neo-tank", "mega-tank", "anti-air"}


def group_of(row):
    """The unit group of a row. A row of the current extractor names it."""
    if "group" in row:
        return row["group"]
    kind = row["kind"]
    if kind in FOOT:
        return "foot"
    if kind in INDIRECT:
        return "indirect"
    if kind in DIRECT:
        return "direct"
    return "other"


def load(prefixes, count):
    """The rows of all prefixes, and one memory map of features for each
    prefix. Each row names its prefix in `file`."""
    rows = []
    blocks = []
    for index, prefix in enumerate(prefixes):
        blocks.append(np.memmap(prefix + ".f32", dtype="<f4", mode="r").reshape(-1, count))
        with open(prefix + ".jsonl") as handle:
            for line in handle:
                row = json.loads(line)
                row["file"] = index
                rows.append(row)
    return rows, blocks


def holdout_maps(rows, share, label, forced):
    """The validation maps: the maps with the lowest hashes until `share` of
    the decisions, and the maps in `forced`."""
    counts = {}
    for row in rows:
        counts[row["map"]] = counts.get(row["map"], 0) + 1
    order = sorted(
        counts,
        key=lambda m: hashlib.sha256(f"{label}:{m}".encode()).hexdigest(),
    )
    chosen = set(forced) & set(counts)
    total = sum(counts.values())
    held = sum(counts[m] for m in chosen)
    for m in order:
        if held >= share * total:
            break
        if m in chosen:
            continue
        chosen.add(m)
        held += counts[m]
    return chosen


class Decisions:
    """The orders of a set of decisions, packed. `fog` is 1 for each order
    of a game with fog."""

    def __init__(self, rows, features, columns):
        offsets = [0]
        chosen = []
        greedy = []
        parts = []
        fog = []
        for row in rows:
            fog.append(1.0 if row.get("fog") else 0.0)
            start = row["offset"]
            block = features[row["file"]][start : start + row["count"]]
            parts.append(np.asarray(block[:, columns], dtype=np.float32))
            chosen.append(offsets[-1] + row["chosen"])
            greedy.append(np.asarray(row["greedy"], dtype=np.float64))
            offsets.append(offsets[-1] + row["count"])
        self.x = np.concatenate(parts).astype(np.float64) if parts else np.zeros((0, len(columns)))
        del parts
        self.starts = np.asarray(offsets[:-1], dtype=np.int64)
        self.offsets = np.asarray(offsets, dtype=np.int64)
        self.chosen = np.asarray(chosen, dtype=np.int64)
        self.greedy = np.concatenate(greedy) if greedy else np.zeros(0)
        self.decision = np.repeat(np.arange(len(rows)), np.diff(self.offsets))
        self.fog = np.asarray(fog, dtype=np.float64)[self.decision] if rows else np.zeros(0)
        self.n = len(rows)


def log_probabilities(d, utility):
    top = np.maximum.reduceat(utility, d.starts)
    shifted = utility - top[d.decision]
    total = np.add.reduceat(np.exp(shifted), d.starts)
    return shifted - np.log(total)[d.decision]


def fit(d, l2, scale, interact=False):
    """Fit a conditional logit. `scale` divides the columns for the solver.

    With `interact`, the utility of an order of a game with fog adds a second
    coefficient vector. The result is then the vector of the games without
    fog and the vector of the games with fog."""
    # The columns are scaled in place to save memory. The caller does not
    # read `d.x` after the fit.
    x = d.x
    x /= scale
    k = x.shape[1]
    fog = d.fog
    chosen_x = x[d.chosen].sum(axis=0)
    chosen_fog = (x[d.chosen] * fog[d.chosen, None]).sum(axis=0)

    def objective(theta):
        beta = theta[:k]
        utility = x @ beta
        if interact:
            delta = theta[k:]
            utility = utility + fog * (x @ delta)
        logp = log_probabilities(d, utility)
        loss = -logp[d.chosen].sum() / d.n + 0.5 * l2 * theta @ theta
        p = np.exp(logp)
        grad = x.T @ p - chosen_x
        if interact:
            grad = np.concatenate([grad, x.T @ (p * fog) - chosen_fog])
        return loss, grad / d.n + l2 * theta

    size = 2 * k if interact else k
    result = minimize(objective, np.zeros(size), jac=True, method="L-BFGS-B",
                      options={"maxiter": 2000})
    beta = result.x[:k] / scale
    if interact:
        return (beta, beta + result.x[k:] / scale), result
    return (beta, beta), result


def ranks(d, utility):
    """The expected top-1 and top-3 hit of the chosen order, with ties
    broken at random, for each decision."""
    top1 = np.zeros(d.n)
    top3 = np.zeros(d.n)
    for i in range(d.n):
        u = utility[d.offsets[i] : d.offsets[i + 1]]
        c = u[d.chosen[i] - d.offsets[i]]
        above = np.sum(u > c)
        tied = np.sum(u == c)
        # The chosen order takes each of the tied places with equal chance.
        top1[i] = (1.0 / tied) if above == 0 else 0.0
        hit = sum(1 for place in range(above, above + tied) if place < 3)
        top3[i] = hit / tied
    return top1, top3


def summary(d, utility, mask=None):
    top1, top3 = ranks(d, utility)
    logp = log_probabilities(d, utility)[d.chosen]
    if mask is None:
        mask = np.ones(d.n, dtype=bool)
    return {
        "decisions": int(mask.sum()),
        "top1": float(top1[mask].mean()),
        "top3": float(top3[mask].mean()),
        "log_loss": float(-logp[mask].mean()),
    }


def pooled_summary(parts):
    """The summary of several (decisions, utility) parts together."""
    n = sum(d.n for d, _ in parts)
    acc = {"top1": 0.0, "top3": 0.0, "log_loss": 0.0}
    for d, u in parts:
        if d.n == 0:
            continue
        s = summary(d, u)
        for m in acc:
            acc[m] += s[m] * d.n
    return {m: (v / n if n else 0.0) for m, v in acc.items()} | {"decisions": n}


def read_model(path, names):
    """The coefficient vector of each group of a model file, over `names`."""
    file = json.load(open(path))
    index = [names.index(n) for n in file["features"]]
    out = {}
    for group in GROUPS:
        values = file["groups"].get(group, file["groups"].get("all"))
        beta = np.zeros(len(names))
        beta[index] = values
        out[group] = beta
    return out


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--names", required=True)
    parser.add_argument("--rows", nargs="+", required=True)
    parser.add_argument("--holdout-share", type=float, default=0.25)
    parser.add_argument("--holdout-label", default="awbrn-move-holdout-v1")
    parser.add_argument("--force-holdout", type=int, nargs="*", default=[77060])
    parser.add_argument("--max-train", type=int, default=200000,
                        help="The largest number of fit decisions for each group, "
                        "for the games with fog and for the games without fog")
    parser.add_argument("--l2", type=float, default=1e-4)
    parser.add_argument("--fog", choices=["no", "yes", "pooled", "interact"], default="no")
    parser.add_argument("--skip-actions", nargs="*", default=[],
                        help="Do not read the decisions with these actions")
    parser.add_argument("--skip-before-unload", action="store_true",
                        help="Do not read the moves of a transport before an unload")
    parser.add_argument("--exclude-games", help="A file with one game ID on each line")
    parser.add_argument("--eval-model", nargs="*", default=[])
    parser.add_argument("--model", required=True)
    parser.add_argument("--summary", required=True)
    args = parser.parse_args()

    names = json.load(open(args.names))
    rows, features = load(args.rows, len(names))
    excluded = set()
    if args.exclude_games:
        excluded = {int(line) for line in open(args.exclude_games) if line.strip()}
    rows = [r for r in rows
            if r["game"] not in excluded
            and r["action"] not in args.skip_actions
            and not (args.skip_before_unload and r.get("before_unload"))]
    print(f"{len(rows)} decisions, {sum(b.shape[0] for b in features)} orders", file=sys.stderr)
    fog_rows = [r for r in rows if r.get("fog")]
    clear_rows = [r for r in rows if not r.get("fog")]
    held = (holdout_maps(clear_rows, args.holdout_share, args.holdout_label, args.force_holdout)
            | holdout_maps(fog_rows, args.holdout_share, args.holdout_label, []))
    use = {"no": [False], "yes": [True], "pooled": [False, True], "interact": [False, True]}[args.fog]
    train_rows = [r for r in rows if r["map"] not in held and bool(r.get("fog")) in use]
    valid = {
        "fog": [r for r in fog_rows if r["map"] in held],
        "clear": [r for r in clear_rows if r["map"] in held],
    }
    rng = np.random.default_rng(20261112)
    evals = [read_model(path, names) for path in args.eval_model]

    blend_columns = list(range(len(names)))
    greedy_columns = [names.index(n) for n in GREEDY]
    out = {
        "fog": args.fog,
        "skip_actions": args.skip_actions,
        "skip_before_unload": args.skip_before_unload,
        "excluded_games": len(excluded),
        "holdout_maps": sorted(held),
        "train_maps": sorted({r["map"] for r in train_rows}),
        "train_decisions": len(train_rows),
        "validation_decisions": {k: len(v) for k, v in valid.items()},
        "train_games": len({r["game"] for r in train_rows}),
        "validation_games": {k: len({r["game"] for r in v}) for k, v in valid.items()},
        "groups": {},
    }
    model = {}
    keys = ["greedy", "greedy_logit", "blend", "uniform"] + [f"eval{i}" for i in range(len(evals))]
    pooled = {side: {key: [] for key in keys} for side in valid}
    for group in GROUPS:
        tr = []
        for flag in use:
            part = [r for r in train_rows
                    if group_of(r) == group and bool(r.get("fog")) == flag]
            if len(part) > args.max_train:
                pick = rng.choice(len(part), args.max_train, replace=False)
                part = [part[i] for i in sorted(pick)]
            tr.extend(part)
        va = {side: [r for r in v if group_of(r) == group] for side, v in valid.items()}
        print(f"{group}: {len(tr)} fit, "
              + ", ".join(f"{len(v)} {side} validation" for side, v in va.items()),
              file=sys.stderr)
        result = {"fit_decisions": len(tr),
                  "validation_decisions": {side: len(v) for side, v in va.items()}}
        betas = {}
        for name, columns in [("blend", blend_columns), ("greedy_logit", greedy_columns)]:
            dt = Decisions(tr, features, columns)
            scale = np.maximum(dt.x.std(axis=0), 1e-6)
            (clear_beta, fog_beta), solver = fit(dt, args.l2, scale, args.fog == "interact")
            del dt
            betas[name] = (clear_beta, fog_beta)
            entry = {"converged": bool(solver.success), "validation": {}}
            for label, beta in [("coefficients", fog_beta), ("clear_coefficients", clear_beta)]:
                if label == "clear_coefficients" and args.fog != "interact":
                    continue
                entry[label] = dict(zip([names[c] for c in columns], map(float, beta)))
            result[name] = entry
            if name == "blend":
                model[group] = [float(b) for b in fog_beta]
        for key in ["greedy", "uniform"] + [f"eval{i}" for i in range(len(evals))]:
            result[key] = {"validation": {}}
        for side, rows_side in va.items():
            # One set of validation decisions for all rankers. The features are
            # dropped after the utilities are calculated, to save memory.
            dv = Decisions(rows_side, features, blend_columns)
            pick = 1 if side == "fog" else 0
            utilities = {
                "blend": dv.x @ betas["blend"][pick],
                "greedy_logit": dv.x[:, greedy_columns] @ betas["greedy_logit"][pick],
                "greedy": dv.greedy,
                "uniform": np.zeros_like(dv.greedy),
            }
            for i, model_betas in enumerate(evals):
                utilities[f"eval{i}"] = dv.x @ model_betas[group]
            dv.x = None
            for key, utility in utilities.items():
                pooled[side][key].append((dv, utility))
                if key in result and dv.n:
                    result[key]["validation"][side] = summary(dv, utility)
        out["groups"][group] = result
        print(json.dumps({k: v["validation"] for k, v in result.items()
                          if isinstance(v, dict) and "validation" in v}), file=sys.stderr)

    out["validation"] = {side: {key: pooled_summary(parts) for key, parts in by.items()}
                         for side, by in pooled.items()}
    out["eval_models"] = args.eval_model
    # The agreement by action and by day, for the greedy policy and the blend.
    by = {}
    for side in valid:
        for key in ["greedy", "blend"] + [f"eval{i}" for i in range(len(evals))]:
            for (d, u), group in zip(pooled[side][key], GROUPS):
                va = [r for r in valid[side] if group_of(r) == group]
                if not va:
                    continue
                top1, _ = ranks(d, u)
                for r, hit in zip(va, top1):
                    for slot in [("action", r["action"]), ("day", min(r["day"], 25) // 5 * 5)]:
                        cell = (by.setdefault(side, {}).setdefault(f"{slot[0]}={slot[1]}", {})
                                .setdefault(key, [0.0, 0]))
                        cell[0] += hit
                        cell[1] += 1
    out["top1_by"] = {side: {k: {m: [v[0] / v[1], v[1]] for m, v in c.items()}
                             for k, c in sorted(cells.items())}
                      for side, cells in by.items()}
    print(json.dumps(out["validation"], indent=1), file=sys.stderr)

    with open(args.model, "w") as handle:
        json.dump({
            "schema_version": 1,
            "features": names,
            "groups": model,
            "notes": {
                "fit": "fit_move_policy.py",
                "fog": args.fog,
                "skip_actions": args.skip_actions,
                "skip_before_unload": args.skip_before_unload,
                "holdout_maps": out["holdout_maps"],
                "train_maps": out["train_maps"],
                "validation": {g: out["groups"][g]["blend"]["validation"] for g in GROUPS},
            },
        }, handle, indent=1)
    with open(args.summary, "w") as handle:
        json.dump(out, handle, indent=1)


if __name__ == "__main__":
    main()
