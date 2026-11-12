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

The split is by map: the validation maps are not in the fit. The model reads
all features, with the greedy score features. The script also reports the
agreement of the greedy policy with the human orders on the same decisions.
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
INDIRECT = {"artillery", "rocket", "piperunner"}
DIRECT = {"recon", "tank", "md-tank", "neo-tank", "mega-tank", "anti-air", "missile"}


def group_of(kind):
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
    """The orders of a set of decisions, packed."""

    def __init__(self, rows, features, columns):
        offsets = [0]
        chosen = []
        greedy = []
        parts = []
        for row in rows:
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
        self.n = len(rows)


def log_probabilities(d, utility):
    top = np.maximum.reduceat(utility, d.starts)
    shifted = utility - top[d.decision]
    total = np.add.reduceat(np.exp(shifted), d.starts)
    return shifted - np.log(total)[d.decision]


def fit(d, l2, scale):
    """Fit a conditional logit. `scale` divides the columns for the solver."""
    x = d.x / scale
    k = x.shape[1]

    def objective(beta):
        utility = x @ beta
        logp = log_probabilities(d, utility)
        loss = -logp[d.chosen].sum() / d.n + 0.5 * l2 * beta @ beta
        p = np.exp(logp)
        grad = x.T @ p - x[d.chosen].sum(axis=0)
        return loss, grad / d.n + l2 * beta

    result = minimize(objective, np.zeros(k), jac=True, method="L-BFGS-B",
                      options={"maxiter": 2000})
    return result.x / scale, result


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


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--names", required=True)
    parser.add_argument("--rows", nargs="+", required=True)
    parser.add_argument("--holdout-share", type=float, default=0.25)
    parser.add_argument("--holdout-label", default="awbrn-move-holdout-v1")
    parser.add_argument("--force-holdout", type=int, nargs="*", default=[77060])
    parser.add_argument("--max-train", type=int, default=200000,
                        help="The largest number of fit decisions for each group")
    parser.add_argument("--l2", type=float, default=1e-4)
    parser.add_argument("--model", required=True)
    parser.add_argument("--summary", required=True)
    args = parser.parse_args()

    names = json.load(open(args.names))
    rows, features = load(args.rows, len(names))
    print(f"{len(rows)} decisions, {sum(b.shape[0] for b in features)} orders", file=sys.stderr)
    held = holdout_maps(rows, args.holdout_share, args.holdout_label, args.force_holdout)
    train_rows = [r for r in rows if r["map"] not in held]
    valid_rows = [r for r in rows if r["map"] in held]
    rng = np.random.default_rng(20261112)

    blend_columns = list(range(len(names)))
    greedy_columns = [names.index(n) for n in GREEDY]
    out = {
        "holdout_maps": sorted(held),
        "train_maps": sorted({r["map"] for r in train_rows}),
        "train_decisions": len(train_rows),
        "validation_decisions": len(valid_rows),
        "train_games": len({r["game"] for r in train_rows}),
        "validation_games": len({r["game"] for r in valid_rows}),
        "groups": {},
    }
    model = {}
    pooled = {key: [] for key in ["greedy", "greedy_logit", "blend", "uniform"]}
    for group in GROUPS:
        tr = [r for r in train_rows if group_of(r["kind"]) == group]
        va = [r for r in valid_rows if group_of(r["kind"]) == group]
        if len(tr) > args.max_train:
            pick = rng.choice(len(tr), args.max_train, replace=False)
            tr = [tr[i] for i in sorted(pick)]
        print(f"{group}: {len(tr)} fit, {len(va)} validation", file=sys.stderr)
        result = {"fit_decisions": len(tr), "validation_decisions": len(va)}
        for name, columns in [("blend", blend_columns), ("greedy_logit", greedy_columns)]:
            dt = Decisions(tr, features, columns)
            scale = np.maximum(dt.x.std(axis=0), 1e-6)
            beta, solver = fit(dt, args.l2, scale)
            dv = Decisions(va, features, columns)
            result[name] = {
                "fit": summary(dt, dt.x @ beta),
                "validation": summary(dv, dv.x @ beta),
                "converged": bool(solver.success),
                "coefficients": dict(zip([names[c] for c in columns], map(float, beta))),
                "standardized": dict(zip([names[c] for c in columns],
                                         map(float, beta * scale))),
            }
            if name == "blend":
                model[group] = [float(b) for b in beta]
            pooled[name].append((dv, dv.x @ beta))
        dv = Decisions(va, features, [0])
        result["greedy"] = {"validation": summary(dv, dv.greedy)}
        result["uniform"] = {"validation": summary(dv, np.zeros_like(dv.greedy))}
        pooled["greedy"].append((dv, dv.greedy))
        pooled["uniform"].append((dv, np.zeros_like(dv.greedy)))
        out["groups"][group] = result
        print(json.dumps({k: v["validation"] for k, v in result.items()
                          if isinstance(v, dict) and "validation" in v}), file=sys.stderr)

    totals = {}
    for key, parts in pooled.items():
        n = sum(d.n for d, _ in parts)
        acc = {"top1": 0.0, "top3": 0.0, "log_loss": 0.0}
        for d, u in parts:
            s = summary(d, u)
            for m in acc:
                acc[m] += s[m] * d.n
        totals[key] = {m: v / n for m, v in acc.items()} | {"decisions": n}
    out["validation"] = totals
    # The agreement by action and by day, for the greedy policy and the blend.
    by = {}
    for key in ["greedy", "blend"]:
        for (d, u), group in zip(pooled[key], GROUPS):
            va = [r for r in valid_rows if group_of(r["kind"]) == group]
            top1, _ = ranks(d, u)
            for r, hit in zip(va, top1):
                for slot in [("action", r["action"]), ("day", min(r["day"], 25) // 5 * 5)]:
                    cell = by.setdefault(f"{slot[0]}={slot[1]}", {}).setdefault(key, [0.0, 0])
                    cell[0] += hit
                    cell[1] += 1
    out["top1_by"] = {k: {m: [v[0] / v[1], v[1]] for m, v in c.items()} for k, c in sorted(by.items())}
    print(json.dumps(totals, indent=1), file=sys.stderr)

    with open(args.model, "w") as handle:
        json.dump({
            "schema_version": 1,
            "features": names,
            "groups": model,
            "notes": {
                "fit": "fit_move_policy.py",
                "holdout_maps": out["holdout_maps"],
                "train_maps": out["train_maps"],
                "validation": {g: out["groups"][g]["blend"]["validation"] for g in GROUPS},
            },
        }, handle, indent=1)
    with open(args.summary, "w") as handle:
        json.dump(out, handle, indent=1)


if __name__ == "__main__":
    main()
