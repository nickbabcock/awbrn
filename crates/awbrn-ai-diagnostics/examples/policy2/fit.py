# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.4", "lightgbm==4.6.0"]
# ///
"""Fit the second move model to the rows of `move_policy
extract-policy2`, and write the model files of `move_policy2.rs`.

Inputs: for each prefix, `<prefix>.jsonl` (one row per decision),
`<prefix>.x.f32` (148 features per candidate order) and `<prefix>.u.f32`
(199 features per unit). The extractor computes them with the code of play
(see the README for what its check covers); this script computes none.

Models (`--model`):

- `trees`: boosted trees with a per-decision softmax over the orders of the
  unit, on all 148 features (b+l+g+r+t). Writes `<out>.awt`.
- `unit`: boosted trees with a per-decision softmax over the units with
  candidate orders; the label is the unit that the human moved. Writes
  `<out>.awt`.

The split is by map: `--holdout-maps` are evaluated only. Early stopping uses
inner maps (15% of the train decisions by map hash). Memory: the features are
read from disk in chunks; LightGBM gets them through `lightgbm.Sequence`.
"""

import argparse
import hashlib
import json
import os
import sys
import time

os.environ.setdefault("OPENBLAS_NUM_THREADS", "1")
os.environ.setdefault("OMP_NUM_THREADS", "1")
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import numpy as np

import awt
from names import GROUPS, order_names, unit_names

NX = 148
NU = 199
GROUP_COL = 68  # grp_foot


# The label only seeds the hash that orders the maps. It keeps the name it had
# when the shipped models were fitted, so that a fit picks the same inner maps.
def inner_maps(maps, share=0.15, label="probe-a-inner"):
    order = sorted(set(maps.tolist()), key=lambda m: hashlib.sha256(f"{label}:{m}".encode()).hexdigest())
    counts = {m: int((maps == m).sum()) for m in order}
    total = sum(counts.values())
    out, held = set(), 0
    for m in order:
        if held >= share * total:
            break
        out.add(m)
        held += counts[m]
    return out


class Rows:
    """The decisions of all prefixes, as arrays."""

    def __init__(self, prefixes, fog, exclude):
        cols = {k: [] for k in ("file", "game", "map", "day", "seat", "offset", "count", "chosen",
                                "uoffset", "ucount", "actor", "action", "group")}
        for fi, prefix in enumerate(prefixes):
            with open(prefix + ".jsonl") as h:
                for line in h:
                    r = json.loads(line)
                    if bool(r["fog"]) != fog or r["game"] in exclude:
                        continue
                    cols["file"].append(fi)
                    cols["game"].append(r["game"])
                    cols["map"].append(r["map"])
                    cols["day"].append(r["day"])
                    cols["seat"].append(r["seat"])
                    cols["offset"].append(r["offset"])
                    cols["count"].append(r["count"])
                    cols["chosen"].append(r["chosen"])
                    cols["uoffset"].append(r["units_offset"])
                    cols["ucount"].append(r["units_count"])
                    cols["actor"].append(r["actor_index"])
                    cols["action"].append(r["action"])
                    cols["group"].append(GROUPS.index(r["group"]))
        for k, v in cols.items():
            dtype = np.int64 if k not in ("action",) else None
            setattr(self, k, np.array(v, dtype=dtype) if dtype else np.array(v))
        self.prefixes = prefixes
        self.n = len(self.game)


class Gather:
    """Read the feature rows of a set of decisions from the files, in chunks.
    `kind` is `x` (orders) or `u` (units)."""

    def __init__(self, rows, select, kind):
        self.kind = kind
        self.width = NX if kind == "x" else NU
        off = rows.offset if kind == "x" else rows.uoffset
        cnt = rows.count if kind == "x" else rows.ucount
        self.select = np.asarray(select)
        self.counts = cnt[self.select].astype(np.int64)
        self.starts = np.concatenate([[0], np.cumsum(self.counts)]).astype(np.int64)
        self.total = int(self.starts[-1])
        # Runs of consecutive rows in one file.
        runs = []
        for i, d in enumerate(self.select):
            f, o, c = int(rows.file[d]), int(off[d]), int(cnt[d])
            if c == 0:
                continue
            if runs and runs[-1][0] == f and runs[-1][1] + runs[-1][2] == o:
                runs[-1][2] += c
            else:
                runs.append([f, o, c])
        self.runs = np.array(runs, dtype=np.int64).reshape(-1, 3)
        self.run_starts = np.concatenate([[0], np.cumsum(self.runs[:, 2])]).astype(np.int64)
        suffix = ".x.f32" if kind == "x" else ".u.f32"
        self.handles = [open(p + suffix, "rb") for p in rows.prefixes]

    def close(self):
        for h in self.handles:
            h.close()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()

    def read(self, lo, hi, columns=None):
        """Virtual rows [lo, hi) as float32 (all columns, or `columns`)."""
        out = np.empty((hi - lo, self.width if columns is None else len(columns)), dtype=np.float32)
        at = lo
        r = int(np.searchsorted(self.run_starts, lo, side="right") - 1)
        while at < hi:
            f, o, c = self.runs[r]
            begin = at - self.run_starts[r]
            take = min(c - begin, hi - at)
            h = self.handles[f]
            h.seek(int(o + begin) * self.width * 4)
            block = np.fromfile(h, dtype="<f4", count=int(take) * self.width).reshape(-1, self.width)
            out[at - lo:at - lo + take] = block if columns is None else block[:, columns]
            at += take
            r += 1
        return out


def make_sequence(gather):
    import lightgbm as lgb

    class Seq(lgb.Sequence):
        batch_size = 200000

        def __len__(self):
            return gather.total

        def __getitem__(self, idx):
            if isinstance(idx, slice):
                lo, hi, step = idx.indices(gather.total)
                assert step == 1
                return gather.read(lo, hi)
            # LightGBM samples single rows for the bins, in double precision.
            if isinstance(idx, (int, np.integer)):
                i = int(idx)
                return gather.read(i, i + 1)[0].astype(np.float64)
            idx = np.asarray(idx)
            return np.stack([gather.read(int(i), int(i) + 1)[0] for i in idx]).astype(np.float64)

    return Seq()


class Decisions:
    """Offsets and labels of the selected decisions, in the gather space."""

    def __init__(self, rows, select, kind):
        self.select = np.asarray(select)
        cnt = rows.count if kind == "x" else rows.ucount
        lab = rows.chosen if kind == "x" else rows.actor
        self.count = cnt[self.select].astype(np.int64)
        self.offsets = np.concatenate([[0], np.cumsum(self.count)]).astype(np.int64)
        self.starts = self.offsets[:-1]
        self.n = len(self.select)
        self.decision = np.repeat(np.arange(self.n), self.count)
        self.chosen = self.starts + lab[self.select]
        self.y = np.zeros(int(self.offsets[-1]), dtype=np.float32)
        self.y[self.chosen] = 1
        self.group = rows.group[self.select]
        self.map = rows.map[self.select]
        self.game = rows.game[self.select]


def log_softmax(d, u):
    top = np.maximum.reduceat(u, d.starts)
    s = u - top[d.decision]
    tot = np.add.reduceat(np.exp(s), d.starts)
    return s - np.log(tot)[d.decision]


def evaluate(d, u):
    u = u.astype(np.float64)
    logp = log_softmax(d, u)[d.chosen]
    c = u[d.chosen][d.decision]
    above = np.add.reduceat((u > c).astype(np.int64), d.starts)
    tied = np.add.reduceat((u == c).astype(np.int64), d.starts)
    top1 = np.where(above == 0, 1.0 / tied, 0.0)
    top3 = np.clip(3 - above, 0, tied) / tied
    return {"top1": top1, "top3": top3, "logp": logp}


def summary(d, r, foot_cap=None):
    out = {"decisions": int(d.n), "top1": float(r["top1"].mean()), "top3": float(r["top3"].mean()),
           "log_loss": float(-r["logp"].mean())}
    if foot_cap:
        # Foot decisions weighted down to 40,000 in all, as
        # fit_move_policy.py measured the first fog model.
        w = np.ones(d.n)
        foot = d.group == 0
        if foot.sum() > foot_cap:
            w[foot] = foot_cap / foot.sum()
        out["top1_foot_capped"] = float((r["top1"] * w).sum() / w.sum())
        out["log_loss_foot_capped"] = float((-r["logp"] * w).sum() / w.sum())
    by_map = {}
    for m in sorted(set(d.map.tolist())):
        k = d.map == m
        by_map[int(m)] = {"decisions": int(k.sum()), "top1": float(r["top1"][k].mean())}
    out["by_map"] = by_map
    return out


def greedy_utility(rows, sel):
    """The greedy score of each order (column 0)."""
    d = Decisions(rows, sel, "x")
    with Gather(rows, sel, "x") as gather:
        u = np.empty(gather.total)
        for lo in range(0, gather.total, 500000):
            hi = min(gather.total, lo + 500000)
            u[lo:hi] = gather.read(lo, hi, [0])[:, 0]
    return d, u


def fit_trees(rows, train_sel, inner_sel, kind, a):
    import lightgbm as lgb
    dtr = Decisions(rows, train_sel, kind)
    dva = Decisions(rows, inner_sel, kind)
    names = order_names() if kind == "x" else unit_names()

    def make_obj(dd):
        def obj(preds, _data):
            lp = log_softmax(dd, preds.astype(np.float64))
            p = np.exp(lp)
            grad = p - dd.y
            hess = np.maximum(p * (1 - p), 1e-6)
            return grad, hess
        return obj

    def make_eval(dd):
        def ev(preds, _data):
            lp = log_softmax(dd, preds.astype(np.float64))
            return "softmax_ll", float(-lp[dd.chosen].mean()), False
        return ev

    params = {
        "objective": make_obj(dtr),
        "learning_rate": a.lr,
        "num_leaves": a.leaves,
        "max_depth": a.depth,
        "min_data_in_leaf": a.min_leaf,
        "min_sum_hessian_in_leaf": 1e-2,
        "lambda_l2": 1.0,
        "feature_fraction": 0.8,
        "max_bin": 255,
        "num_threads": 2,
        "verbose": -1,
        "seed": 20261113,
        "deterministic": True,
        "force_row_wise": True,
    }
    t0 = time.time()
    gtr = Gather(rows, train_sel, kind)
    train = lgb.Dataset([make_sequence(gtr)], label=dtr.y, feature_name=names,
                        params={"max_bin": 255, "verbose": -1}).construct()
    gva = Gather(rows, inner_sel, kind)
    valid = lgb.Dataset([make_sequence(gva)], label=dva.y, reference=train).construct()
    print(f"datasets {time.time() - t0:.0f}s: train {dtr.n} decisions ({gtr.total} rows), "
          f"inner {dva.n} ({gva.total})", file=sys.stderr)
    evals = {}
    booster = lgb.train(params, train, num_boost_round=a.rounds, valid_sets=[valid],
                        valid_names=["inner"], feval=make_eval(dva),
                        callbacks=[lgb.early_stopping(a.patience, verbose=False),
                                   lgb.log_evaluation(50), lgb.record_evaluation(evals)])
    gtr.close()
    gva.close()
    best = booster.best_iteration or a.rounds
    print(f"best iteration {best}, inner ll {booster.best_score['inner']['softmax_ll']:.4f}, "
          f"{time.time() - t0:.0f}s", file=sys.stderr)
    del train, valid
    imp = dict(zip(names, booster.feature_importance("gain").tolist()))
    info = {"best_iteration": best, "inner_ll": booster.best_score["inner"]["softmax_ll"],
            "train_decisions": int(dtr.n), "train_rows": int(gtr.total),
            "inner_decisions": int(dva.n), "seconds": time.time() - t0,
            "importance_gain_top": dict(sorted(imp.items(), key=lambda kv: -kv[1])[:30]),
            "inner_curve": evals["inner"]["softmax_ll"][::25]}
    return booster, best, info


def predict(booster, best, rows, sel, kind):
    with Gather(rows, sel, kind) as gather:
        out = np.empty(gather.total)
        for lo in range(0, gather.total, 300000):
            hi = min(gather.total, lo + 300000)
            out[lo:hi] = booster.predict(gather.read(lo, hi), num_iteration=best, raw_score=True)
    return out


def fixture(path, booster, best, rows, sel, kind, n, keep, seed=7):
    """Held-out rows and their raw LightGBM scores, for the Rust test: `n`
    rows drawn with `seed`, in file order, of which the first `keep` are
    written (all of them when `keep` is zero)."""
    with Gather(rows, sel, kind) as gather:
        rng = np.random.default_rng(seed)
        pick = np.sort(rng.choice(gather.total, min(n, gather.total), replace=False))
        if keep:
            pick = pick[:keep]
        x = np.stack([gather.read(int(i), int(i) + 1)[0] for i in pick])
    raw = booster.predict(x, num_iteration=best, raw_score=True).astype(np.float64)
    with open(path, "wb") as h:
        h.write(np.array([len(x), x.shape[1]], dtype="<u4").tobytes())
        h.write(x.astype("<f4").tobytes())
        h.write(raw.astype("<f8").tobytes())


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--rows", nargs="+", required=True)
    p.add_argument("--fog", choices=["no", "yes"], required=True)
    p.add_argument("--exclude-games")
    p.add_argument("--holdout-maps", type=int, nargs="+", required=True)
    p.add_argument("--model", choices=["trees", "unit"], required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--max-train", type=int, default=0, help="cap of train decisions (0: all)")
    p.add_argument("--lr", type=float, default=0.1)
    p.add_argument("--leaves", type=int, default=63)
    p.add_argument("--depth", type=int, default=6)
    p.add_argument("--min-leaf", type=int, default=200)
    p.add_argument("--rounds", type=int, default=500)
    p.add_argument("--patience", type=int, default=50)
    p.add_argument("--fixture", help="write held-out fixture rows here")
    p.add_argument("--fixture-rows", type=int, default=1000, help="held-out rows drawn")
    p.add_argument("--fixture-keep", type=int, default=0,
                   help="write only the first rows of the draw (0: all)")
    a = p.parse_args()
    t0 = time.time()
    exclude = set()
    if a.exclude_games:
        with open(a.exclude_games) as h:
            exclude = {int(x) for x in h if x.strip()}
    rows = Rows(a.rows, a.fog == "yes", exclude)
    held = np.isin(rows.map, a.holdout_maps)
    train = np.flatnonzero(~held)
    valid = np.flatnonzero(held)
    if a.max_train and len(train) > a.max_train:
        train = np.sort(np.random.default_rng(7).choice(train, a.max_train, replace=False))
    print(f"{rows.n} decisions; train {len(train)}, held-out {len(valid)} "
          f"({len(set(rows.map[valid].tolist()))} maps)", file=sys.stderr)
    foot_cap = 40000 if a.fog == "yes" else None
    out = {"model": a.model, "fog": a.fog, "args": vars(a), "train_decisions": int(len(train)),
           "valid_decisions": int(len(valid))}
    if a.model == "trees":
        # The greedy argmax, on the same decisions.
        dv, uraw = greedy_utility(rows, valid)
        out["greedy_argmax_held_out"] = summary(dv, evaluate(dv, uraw), foot_cap)
        maps = rows.map[train]
        inner = np.isin(maps, list(inner_maps(maps)))
        booster, best, info = fit_trees(rows, train[~inner], train[inner], "x", a)
        out["fit"] = info
        u = predict(booster, best, rows, valid, "x")
        dv = Decisions(rows, valid, "x")
        out["held_out"] = summary(dv, evaluate(dv, u), foot_cap)
        dump = booster.dump_model(num_iteration=best)
        roots, nodes, leaves = awt.flatten(dump)
        size = awt.write(a.out + ".awt", order_names(), [], roots, nodes, leaves)
        out["model_file"] = {"bytes": size, "trees": len(roots), "nodes": len(nodes), "leaves": len(leaves)}
        if a.fixture:
            fixture(a.fixture, booster, best, rows, valid, "x", a.fixture_rows, a.fixture_keep)
    else:
        ok = rows.actor >= 0
        train = train[ok[train] & (rows.ucount[train] > 0)]
        valid_all = valid
        valid = valid[ok[valid] & (rows.ucount[valid] > 0)]
        out["valid_dropped"] = int(len(valid_all) - len(valid))
        maps = rows.map[train]
        inner = np.isin(maps, list(inner_maps(maps)))
        booster, best, info = fit_trees(rows, train[~inner], train[inner], "u", a)
        out["fit"] = info
        u = predict(booster, best, rows, valid, "u")
        dv = Decisions(rows, valid, "u")
        out["held_out"] = summary(dv, evaluate(dv, u))
        # The greedy order: the unit with the best greedy score.
        g = np.empty(int(dv.offsets[-1]))
        with Gather(rows, valid, "u") as gather:
            for lo in range(0, gather.total, 500000):
                hi = min(gather.total, lo + 500000)
                g[lo:hi] = gather.read(lo, hi, [5])[:, 0]
        out["greedy_held_out"] = summary(dv, evaluate(dv, g))
        out["uniform_held_out"] = summary(dv, evaluate(dv, np.zeros_like(g)))
        dump = booster.dump_model(num_iteration=best)
        roots, nodes, leaves = awt.flatten(dump)
        size = awt.write(a.out + ".awt", unit_names(), [], roots, nodes, leaves)
        out["model_file"] = {"bytes": size, "trees": len(roots), "nodes": len(nodes), "leaves": len(leaves)}
        if a.fixture:
            fixture(a.fixture, booster, best, rows, valid, "u", a.fixture_rows, a.fixture_keep)
    out["seconds"] = time.time() - t0
    with open(a.out + ".report.json", "w") as h:
        json.dump(out, h, indent=1)
    brief = {k: out[k]["top1"] for k in ("held_out",) if k in out}
    print(json.dumps({"model": a.model, "fog": a.fog, **brief,
                      "log_loss": out.get("held_out", {}).get("log_loss")}), file=sys.stderr)


if __name__ == "__main__":
    main()
