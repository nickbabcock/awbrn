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

`--fog no` fits the games without fog, and `--fog yes` fits the games with
fog. The unit group of a row is the group that the policy uses in play (the
`group` field of the row).
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


# The fields of a row that the script reads. The other fields are dropped
# when the rows load, to save memory.
KEEP = ("game", "map", "day", "action", "kind", "group", "fog", "offset", "count", "chosen")


def load(prefixes, count):
    """The rows of all prefixes, and the feature file of each prefix. Each row
    names its prefix in `file`. The greedy scores of a row are a float32
    array. The features stay on disk until a set of decisions reads them."""
    rows = []
    files = []
    for index, prefix in enumerate(prefixes):
        files.append((prefix + ".f32", count))
        with open(prefix + ".jsonl") as handle:
            for line in handle:
                full = json.loads(line)
                row = {key: full[key] for key in KEEP if key in full}
                row["greedy"] = np.asarray(full["greedy"], dtype=np.float32)
                row["file"] = index
                rows.append(row)
    return rows, files


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
    """The orders of a set of decisions, packed. The features are float32."""

    def __init__(self, rows, files, columns):
        total = sum(row["count"] for row in rows)
        self.x = np.empty((total, len(columns)), dtype=np.float32)
        offsets = np.zeros(len(rows) + 1, dtype=np.int64)
        chosen = np.empty(len(rows), dtype=np.int64)
        self.greedy = np.empty(total, dtype=np.float64)
        handles = {}
        at = 0
        for i, row in enumerate(rows):
            path, count = files[row["file"]]
            handle = handles.get(path)
            if handle is None:
                handle = handles[path] = open(path, "rb")
            n = row["count"]
            handle.seek(row["offset"] * count * 4)
            block = np.fromfile(handle, dtype="<f4", count=n * count).reshape(n, count)
            self.x[at : at + n] = block[:, columns]
            self.greedy[at : at + n] = row["greedy"]
            chosen[i] = at + row["chosen"]
            at += n
            offsets[i + 1] = at
        for handle in handles.values():
            handle.close()
        self.starts = offsets[:-1]
        self.offsets = offsets
        self.chosen = chosen
        self.decision = np.repeat(np.arange(len(rows)), np.diff(offsets))
        self.n = len(rows)


def log_probabilities(d, utility):
    top = np.maximum.reduceat(utility, d.starts)
    shifted = utility - top[d.decision]
    total = np.add.reduceat(np.exp(shifted), d.starts)
    return shifted - np.log(total)[d.decision]


def fit(d, l2, scale):
    """Fit a conditional logit. `scale` divides the columns for the solver.
    The features are float32, so the products with the features are float32
    and the sums of the loss are float64."""
    # The columns are scaled in place to save memory. The caller does not
    # read `d.x` after the fit.
    x = d.x
    x /= scale.astype(np.float32)
    k = x.shape[1]
    chosen_x = x[d.chosen].astype(np.float64).sum(axis=0)

    def objective(beta):
        utility = (x @ beta.astype(np.float32)).astype(np.float64)
        logp = log_probabilities(d, utility)
        loss = -logp[d.chosen].sum() / d.n + 0.5 * l2 * beta @ beta
        p = np.exp(logp).astype(np.float32)
        grad = (x.T @ p).astype(np.float64) - chosen_x
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


def add_summary(total, part):
    """Add the summary `part` to the running total, weighted by decisions."""
    n = part["decisions"]
    for m in ("top1", "top3", "log_loss"):
        total[m] = total.get(m, 0.0) + part[m] * n
    total["decisions"] = total.get("decisions", 0) + n


def mean_summary(total):
    n = total.get("decisions", 0)
    return {m: (total.get(m, 0.0) / n if n else 0.0)
            for m in ("top1", "top3", "log_loss")} | {"decisions": n}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--names", required=True)
    parser.add_argument("--rows", nargs="+", required=True)
    parser.add_argument("--holdout-share", type=float, default=0.25)
    parser.add_argument("--holdout-label", default="awbrn-move-holdout-v1")
    parser.add_argument("--force-holdout", type=int, nargs="*", default=[77060])
    parser.add_argument("--max-train", type=int, default=200000,
                        help="The largest number of fit decisions for each group")
    parser.add_argument("--max-validation", type=int, default=40000,
                        help="The largest number of validation decisions for each group "
                        "and for each of the two validation sets (fog and no fog)")
    parser.add_argument("--l2", type=float, default=1e-4)
    parser.add_argument("--fog", choices=["no", "yes"], default="no")
    parser.add_argument("--exclude-games", help="A file with one game ID on each line")
    parser.add_argument("--model", required=True)
    parser.add_argument("--summary", required=True)
    args = parser.parse_args()

    names = json.load(open(args.names))
    rows, features = load(args.rows, len(names))
    excluded = set()
    if args.exclude_games:
        excluded = {int(line) for line in open(args.exclude_games) if line.strip()}
    rows = [r for r in rows if r["game"] not in excluded]
    print(f"{len(rows)} decisions, {sum(r['count'] for r in rows)} orders", file=sys.stderr)
    fog_rows = [r for r in rows if r.get("fog")]
    clear_rows = [r for r in rows if not r.get("fog")]
    held = (holdout_maps(clear_rows, args.holdout_share, args.holdout_label, args.force_holdout)
            | holdout_maps(fog_rows, args.holdout_share, args.holdout_label, []))
    use_fog = args.fog == "yes"
    train_rows = [r for r in rows if r["map"] not in held and bool(r.get("fog")) == use_fog]
    valid = {
        "fog": [r for r in fog_rows if r["map"] in held],
        "clear": [r for r in clear_rows if r["map"] in held],
    }
    rng = np.random.default_rng(20261112)

    blend_columns = list(range(len(names)))
    greedy_columns = [names.index(n) for n in GREEDY]
    out = {
        "fog": args.fog,
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
    keys = ["greedy", "greedy_logit", "blend", "uniform"]
    totals = {side: {key: {} for key in keys} for side in valid}
    by = {}

    def sample(part, limit):
        if len(part) > limit:
            pick = rng.choice(len(part), limit, replace=False)
            part = [part[i] for i in sorted(pick)]
        return part

    for group in GROUPS:
        tr = sample([r for r in train_rows if r["group"] == group], args.max_train)
        va = {side: sample([r for r in v if r["group"] == group], args.max_validation)
              for side, v in valid.items()}
        print(f"{group}: {len(tr)} fit, "
              + ", ".join(f"{len(v)} {side} validation" for side, v in va.items()),
              file=sys.stderr)
        result = {"fit_decisions": len(tr),
                  "validation_decisions": {side: len(v) for side, v in va.items()}}
        betas = {}
        for name, columns in [("blend", blend_columns), ("greedy_logit", greedy_columns)]:
            dt = Decisions(tr, features, columns)
            scale = np.maximum(dt.x.std(axis=0, dtype=np.float64), 1e-6)
            beta, solver = fit(dt, args.l2, scale)
            del dt
            betas[name] = beta
            result[name] = {
                "converged": bool(solver.success),
                "validation": {},
                "coefficients": dict(zip([names[c] for c in columns], map(float, beta))),
            }
            if name == "blend":
                model[group] = [float(b) for b in beta]
        for key in ["greedy", "uniform"]:
            result[key] = {"validation": {}}
        for side, rows_side in va.items():
            if not rows_side:
                continue
            # One set of validation decisions for all rankers, freed before
            # the next set.
            dv = Decisions(rows_side, features, blend_columns)

            def utility_of(beta, columns=None):
                x = dv.x if columns is None else dv.x[:, columns]
                return (x @ beta.astype(np.float32)).astype(np.float64)

            utilities = {
                "blend": lambda: utility_of(betas["blend"]),
                "greedy_logit": lambda: utility_of(betas["greedy_logit"], greedy_columns),
                "greedy": lambda: dv.greedy,
                "uniform": lambda: np.zeros_like(dv.greedy),
            }
            for key, make in utilities.items():
                utility = make()
                part = summary(dv, utility)
                result[key]["validation"][side] = part
                add_summary(totals[side][key], part)
                if key in ("greedy", "blend"):
                    top1, _ = ranks(dv, utility)
                    for r, hit in zip(rows_side, top1):
                        for slot in [("action", r["action"]), ("day", min(r["day"], 25) // 5 * 5)]:
                            cell = (by.setdefault(side, {}).setdefault(f"{slot[0]}={slot[1]}", {})
                                    .setdefault(key, [0.0, 0]))
                            cell[0] += hit
                            cell[1] += 1
                del utility
            del dv
        out["groups"][group] = result
        print(json.dumps({k: v["validation"] for k, v in result.items()
                          if isinstance(v, dict) and "validation" in v}), file=sys.stderr)

    out["validation"] = {side: {key: mean_summary(t) for key, t in by_key.items()}
                         for side, by_key in totals.items()}
    out["max_train"] = args.max_train
    out["max_validation"] = args.max_validation
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
                "holdout_maps": out["holdout_maps"],
                "train_maps": out["train_maps"],
                "validation": {g: out["groups"][g]["blend"]["validation"] for g in GROUPS},
            },
        }, handle, indent=1)
    with open(args.summary, "w") as handle:
        json.dump(out, handle, indent=1)


if __name__ == "__main__":
    main()
