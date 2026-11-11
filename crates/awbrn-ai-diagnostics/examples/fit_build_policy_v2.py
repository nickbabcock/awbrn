"""Fit the build policy to human build decisions.

The input directories contain `rows.jsonl` files from `build_policy
extract`. The model is a conditional logit, with these properties:

- The choice set is the legal kinds of the site. The "no build" rows and the
  "no build" choice are not used, because the greedy policy does not use the
  "no build" choice.
- The rows of the maps in `--exclude-maps` are not used.
- The commander features are not standardized, and they get a larger
  penalty. Thus a commander with few rows, or with no rows, gets a weight
  near zero, which is the shared effect of all commanders.
- The script selects the feature groups and the penalties by a cross
  validation over the maps of `--train`: each map is one fold. The corpus has
  few maps, and the games of one map are similar, so a fold by game would
  reward features that learn a map. It adds one group at a time, in a
  fixed order, and keeps a group only when it decreases the cross-validated
  log loss by more than `--threshold`.
- The holdout rows of `--holdout` are split into folds by map. The model of
  each fold is fitted on `--train` and on the holdout rows of the other maps.
  Thus each holdout row gets a probability from a model that did not see its
  map in the holdout. (One holdout map also has games in `--train`; both
  models saw those games.) The
  reference model (`--reference`, human-v1) did not see the holdout games, so
  the two sets of probabilities compare.
- The model file is fitted on all rows of `--train` and `--holdout`.

The script writes the model file and a summary with the holdout metrics.
"""

import argparse
import json
import sys
from collections import Counter, defaultdict
from pathlib import Path

import numpy as np
from scipy.optimize import minimize

KINDS = [
    "anti-air", "apc", "artillery", "b-copter", "battleship", "black-boat",
    "black-bomb", "bomber", "carrier", "cruiser", "fighter", "infantry",
    "lander", "md-tank", "mech", "mega-tank", "missile", "neo-tank",
    "piperunner", "recon", "rocket", "stealth", "sub", "t-copter", "tank",
]
INDEX = {kind: i for i, kind in enumerate(KINDS)}
C = len(KINDS)
BANDS = ["1-6", "7-9", "10-12", "13-15", "16-20", "21+"]


def band(day):
    for limit, name in [(6, "1-6"), (9, "7-9"), (12, "10-12"), (15, "13-15"), (20, "16-20")]:
        if day <= limit:
            return name
    return "21+"


class Data:
    """The build rows of some directories as arrays."""

    def __init__(self, directories, excluded):
        rows = []
        self.skipped = Counter()
        for directory in directories:
            with open(Path(directory) / "rows.jsonl") as handle:
                for line in handle:
                    row = json.loads(line)
                    if row["fog"]:
                        self.skipped["fog"] += 1
                    elif row["map"] in excluded:
                        self.skipped["excluded map"] += 1
                    elif row["chosen"] == "no-build":
                        self.skipped["no build"] += 1
                    elif not row["chosen_legal"] or not row["legal"]:
                        self.skipped["illegal choice"] += 1
                    else:
                        rows.append(row)
        n = len(rows)
        self.n = n
        self.x = np.zeros((n, len(rows[0]["decision"])))
        self.z = np.zeros((n, C, len(rows[0]["kind"][0])))
        self.mask = np.zeros((n, C), dtype=bool)
        self.y = np.zeros(n, dtype=int)
        self.greedy = np.full(n, -1, dtype=int)
        self.day = np.zeros(n, dtype=int)
        self.step = np.zeros(n, dtype=int)
        self.funds = np.zeros(n)
        self.turn = []
        self.terrain = []
        self.own_co = []
        maps = []
        for i, row in enumerate(rows):
            maps.append(row["map"])
            self.x[i] = row["decision"]
            for kind, features in zip(row["legal"], row["kind"]):
                self.mask[i, INDEX[kind]] = True
                self.z[i, INDEX[kind]] = features
            self.y[i] = INDEX[row["chosen"]]
            if row["greedy"] is not None:
                self.greedy[i] = INDEX[row["greedy"]]
            self.day[i] = row["day"]
            self.step[i] = row["step"]
            self.funds[i] = row["funds"]
            self.turn.append((row["game"], row["seat"], row["day"]))
            self.terrain.append(row["terrain"])
            self.own_co.append(row["own_co"])
        self.band = np.array([band(day) for day in self.day])
        self.terrain = np.array(self.terrain)
        self.own_co = np.array(self.own_co)
        self.game = np.array([key[0] for key in self.turn], dtype=np.int64)
        self.map = np.array(maps, dtype=np.int64)

    def subset(self, keep):
        other = object.__new__(Data)
        for name, value in vars(self).items():
            if isinstance(value, np.ndarray) and value.shape[:1] == (self.n,):
                setattr(other, name, value[keep])
            elif isinstance(value, list) and len(value) == self.n:
                setattr(other, name, [v for v, k in zip(value, keep) if k])
            else:
                setattr(other, name, value)
        other.n = int(np.sum(keep))
        return other

    @staticmethod
    def join(first, second):
        other = object.__new__(Data)
        for name, value in vars(first).items():
            if isinstance(value, np.ndarray) and value.shape[:1] == (first.n,):
                setattr(other, name, np.concatenate([value, getattr(second, name)]))
            elif isinstance(value, list) and len(value) == first.n:
                setattr(other, name, value + getattr(second, name))
            else:
                setattr(other, name, value)
        other.n = first.n + second.n
        return other


def map_folds(data, folds=0):
    """Split the rows into folds by map. With `folds` zero, each map is one
    fold. Otherwise the maps go into `folds` groups: the map with the most
    rows goes first, into the group with the fewest rows. The return value is
    the fold of each row and the number of folds."""
    maps, counts = np.unique(data.map, return_counts=True)
    if folds <= 0 or folds >= len(maps):
        fold_of = {m: i for i, m in enumerate(sorted(maps))}
        return np.array([fold_of[m] for m in data.map]), len(maps)
    size = [0] * folds
    fold_of = {}
    for index in sorted(range(len(maps)), key=lambda i: (-counts[i], maps[i])):
        target = min(range(folds), key=lambda f: (size[f], f))
        fold_of[maps[index]] = target
        size[target] += counts[index]
    return np.array([fold_of[m] for m in data.map]), folds


def log_softmax(u, mask):
    u = np.where(mask, u, -np.inf)
    top = np.max(u, axis=1, keepdims=True)
    shifted = u - top
    return shifted - np.log(np.sum(np.exp(shifted), axis=1, keepdims=True))


class Logit:
    """A conditional logit over the legal kinds."""

    def __init__(self, columns, kind_columns, raw, train):
        self.columns = list(columns)
        self.kind_columns = list(kind_columns)
        x = train.x[:, self.columns]
        self.mean = x.mean(axis=0)
        self.scale = x.std(axis=0)
        self.scale[self.scale < 1e-9] = 1.0
        # The raw columns (commander one-hots) keep their values.
        self.raw = np.array([c in raw for c in self.columns], dtype=bool)
        self.mean[self.raw] = 0.0
        self.scale[self.raw] = 1.0
        z = train.z[train.mask][:, self.kind_columns]
        self.kmean = z.mean(axis=0)
        self.kscale = z.std(axis=0)
        self.kscale[self.kscale < 1e-9] = 1.0
        self.d = len(self.columns)
        self.k = len(self.kind_columns)

    def design(self, data):
        x = (data.x[:, self.columns] - self.mean) / self.scale
        z = (data.z[:, :, self.kind_columns] - self.kmean) / self.kscale
        z = np.where(data.mask[:, :, None], z, 0.0)
        return x, z

    def unpack(self, theta):
        b = theta[:C]
        w = theta[C:C + C * self.d].reshape(C, self.d)
        v = theta[C + C * self.d:]
        return b, w, v

    def objective(self, theta, x, z, mask, y, penalty, kind_mask):
        b, w, v = self.unpack(theta)
        u = b + x @ w.T + z @ v
        logp = log_softmax(u, mask)
        n = len(y)
        loss = -np.mean(logp[np.arange(n), y])
        p = np.where(mask, np.exp(logp), 0.0)
        p[np.arange(n), y] -= 1.0
        p /= n
        gb = p.sum(axis=0)
        gw = p.T @ x
        gv = np.einsum("nc,nck->k", p, z)
        loss += 0.5 * np.sum(penalty * w * w) + 0.5 * self.kind_penalty * np.sum(v * v)
        loss += 0.5 * 1e-4 * np.sum(b * b)
        gw += penalty * w
        gv += self.kind_penalty * v
        gb += 1e-4 * b
        gw[~kind_mask] = 0.0
        return loss, np.concatenate([gb, gw.ravel(), gv])

    def fit(self, data, penalty, co_multiplier):
        x, z = self.design(data)
        self.kind_mask = np.bincount(data.y, minlength=C) > 0
        column_penalty = np.where(self.raw, penalty * co_multiplier, penalty)
        self.kind_penalty = penalty
        self.penalty = penalty
        self.co_multiplier = co_multiplier
        theta = np.zeros(C + C * self.d + self.k)
        result = minimize(
            self.objective, theta, jac=True, method="L-BFGS-B",
            args=(x, z, data.mask, data.y, column_penalty[None, :], self.kind_mask),
            options={"maxiter": 4000, "maxfun": 8000},
        )
        self.theta = result.x
        self.converged = bool(result.success)
        return self

    def log_probabilities(self, data):
        x, z = self.design(data)
        b, w, v = self.unpack(self.theta)
        return log_softmax(b + x @ w.T + z @ v, data.mask)

    def to_file(self, names, kind_names, name, fit):
        b, w, v = self.unpack(self.theta)
        kinds = {
            kind: {"intercept": float(b[i]), "weights": [float(x) for x in w[i]]}
            for i, kind in enumerate(KINDS)
        }
        return {
            "schema_version": 1,
            "name": name,
            "decision_features": [names[i] for i in self.columns],
            "decision_mean": [float(x) for x in self.mean],
            "decision_scale": [float(x) for x in self.scale],
            "kind_features": [kind_names[i] for i in self.kind_columns],
            "kind_mean": [float(x) for x in self.kmean],
            "kind_scale": [float(x) for x in self.kscale],
            "kind_weights": [float(x) for x in v],
            "kinds": kinds,
            # The model is fitted without "no build". The choice gets a very
            # low utility, so a configuration that offers it never takes it.
            "no_build": {"intercept": -1000.0, "weights": [0.0] * self.d},
            "fit": fit,
        }


class FileModel:
    """A model file, evaluated as the Rust code does."""

    def __init__(self, path, names):
        self.file = json.load(open(path))
        self.columns = [names["decision"].index(n) for n in self.file["decision_features"]]
        self.kind_columns = [names["kind"].index(n) for n in self.file["kind_features"]]
        self.mean = np.array(self.file["decision_mean"])
        self.scale = np.array(self.file["decision_scale"])
        self.kmean = np.array(self.file["kind_mean"])
        self.kscale = np.array(self.file["kind_scale"])
        self.v = np.array(self.file["kind_weights"])
        self.b = np.array([self.file["kinds"][k]["intercept"] for k in KINDS])
        self.w = np.array([self.file["kinds"][k]["weights"] for k in KINDS])

    def log_probabilities(self, data):
        x = (data.x[:, self.columns] - self.mean) / self.scale
        z = (data.z[:, :, self.kind_columns] - self.kmean) / self.kscale
        z = np.where(data.mask[:, :, None], z, 0.0)
        return log_softmax(self.b + x @ self.w.T + z @ self.v, data.mask)


def metrics(logp, data, keep=None):
    if keep is None:
        keep = np.ones(data.n, dtype=bool)
    if keep.sum() == 0:
        return {"rows": 0}
    idx = np.arange(data.n)
    choice = np.argmax(np.where(data.mask, logp, -np.inf), axis=1)
    return {
        "rows": int(keep.sum()),
        "log_loss": round(float(-np.mean(logp[idx, data.y][keep])), 4),
        "top1": round(float(np.mean((choice == data.y)[keep])), 4),
    }


def turn_match(logp, data):
    """The share of turns where the argmax kinds equal the human kinds as a
    multiset. Each row is predicted from the human board, so this is an upper
    bound on the order effect, not a simulation."""
    choice = np.argmax(np.where(data.mask, logp, -np.inf), axis=1)
    turns = defaultdict(lambda: (Counter(), Counter()))
    for i, key in enumerate(data.turn):
        turns[key][0][choice[i]] += 1
        turns[key][1][data.y[i]] += 1
    return round(sum(a == b for a, b in turns.values()) / len(turns), 4)


def calibration(logp, data):
    choice = np.argmax(np.where(data.mask, logp, -np.inf), axis=1)
    prob = np.exp(np.where(data.mask, logp, -np.inf))
    out = {}
    human = np.bincount(data.y, minlength=C) / data.n
    argmax = np.bincount(choice, minlength=C) / data.n
    mean = prob.mean(axis=0)
    for i in np.argsort(-human):
        if human[i] < 0.0005 and argmax[i] < 0.0005:
            continue
        out[KINDS[i]] = {
            "human": round(float(human[i]), 4),
            "argmax": round(float(argmax[i]), 4),
            "mean_probability": round(float(mean[i]), 4),
        }
    return out


def copter_case(logp, data, names):
    """The rows of the copter veto condition: a b-copter is legal, the day is
    at most 12, at least 10% of the enemy health hard-counters a b-copter and
    the mean exchange of a b-copter is at most zero."""
    kind = names["kind"]
    predator = data.z[:, INDEX["b-copter"], kind.index("predator_share")]
    counter = data.z[:, INDEX["b-copter"], kind.index("counter")]
    keep = data.mask[:, INDEX["b-copter"]] & (data.day <= 12) & (predator >= 0.10) & (counter <= 0)
    choice = np.argmax(np.where(data.mask, logp, -np.inf), axis=1)
    return {
        "rows": int(keep.sum()),
        "human_b_copter": int(np.sum(keep & (data.y == INDEX["b-copter"]))),
        "argmax_b_copter": int(np.sum(keep & (choice == INDEX["b-copter"]))),
        "top1": round(float(np.mean((choice == data.y)[keep])), 4) if keep.sum() else None,
    }


def report(logp, data, names):
    groups = {"all": np.ones(data.n, dtype=bool)}
    for b in BANDS:
        groups[f"day {b}"] = data.band == b
    for s in range(4):
        groups[f"step {s}" if s < 3 else "step 3+"] = (data.step == s) if s < 3 else (data.step >= 3)
    for label, low, high in [("<4k", 0, 4000), ("4-7k", 4000, 7000), ("7-12k", 7000, 12000),
                             ("12-20k", 12000, 20000), ("20k+", 20000, 1e9)]:
        groups[f"funds {label}"] = (data.funds >= low) & (data.funds < high)
    for co in ["drake", "grimm", "jess", "sonja", "rachel", "koal", "sami", "andy"]:
        groups[f"own {co}"] = data.own_co == co
    out = {group: metrics(logp, data, keep) for group, keep in groups.items()}
    out["turn_multiset_match"] = turn_match(logp, data)
    out["calibration"] = calibration(logp, data)
    out["copter_veto_condition"] = copter_case(logp, data, names)
    return out


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--names", required=True)
    parser.add_argument("--train", nargs="+", required=True)
    parser.add_argument("--holdout", nargs="+", required=True)
    parser.add_argument("--exclude-maps", help="a JSON list of map IDs that the fit does not use")
    parser.add_argument("--folds", type=int, default=0,
                        help="the number of map folds of the selection; zero gives one fold for each map")
    parser.add_argument("--protocol", choices=["v2", "v1"], default="v2",
                        help="v1: fit on --train only, and score --holdout with that fit")
    parser.add_argument("--reference", required=True, help="the human-v1 model file")
    parser.add_argument("--output", required=True)
    parser.add_argument("--model", required=True)
    parser.add_argument("--name", default="human-v2a")
    parser.add_argument("--threshold", type=float, default=0.002)
    args = parser.parse_args()
    names = json.load(open(args.names))
    decision, kind_names = names["decision"], names["kind"]
    excluded = set(json.load(open(args.exclude_maps))) if args.exclude_maps else set()
    output = Path(args.output)
    output.mkdir(parents=True, exist_ok=True)

    train = Data(args.train, excluded)
    holdout = Data(args.holdout, excluded)
    print(f"rows: train {train.n}, holdout {holdout.n}", file=sys.stderr)

    def columns_of(predicate):
        return [i for i, n in enumerate(decision) if predicate(n)]

    v1_file = json.load(open(args.reference))
    v1_decision = [decision.index(n) for n in v1_file["decision_features"]]
    first_extra = decision.index("built_turn")
    base = columns_of(lambda n: "_co_" not in n and decision.index(n) < first_extra)
    co = columns_of(lambda n: "_co_" in n)
    decision_groups = {
        "co": co,
        "built": columns_of(lambda n: n.startswith("built_turn")),
        "map": columns_of(lambda n: n == "map_area"),
        "front": columns_of(lambda n: n.startswith("site_front")),
        "shares": columns_of(lambda n: "_share_" in n),
    }
    kind_index = {n: i for i, n in enumerate(kind_names)}
    v1_kind = [kind_index[n] for n in ["cost", "overspend", "counter", "own_kind_count"]]
    kind_groups = {
        "own_class": [kind_index["own_class_count"], kind_index["own_class_share"]],
        "built_kind": [kind_index["built_turn_kind"], kind_index["built_turn_class"]],
        "funds_after": [kind_index["remainder"], kind_index["fill_short"]],
        "prey": [kind_index["prey_share"], kind_index["predator_share"], kind_index["prey_value"]],
        "counter_near": [kind_index["counter_near"]],
        "co_combat": [kind_index["co_attack"], kind_index["co_defense"]],
    }
    raw = set(co)
    selection = []
    fold, fold_count = map_folds(train, args.folds)

    def cross_validate(columns, kind_columns, penalty, co_multiplier):
        logp = np.zeros((train.n, C))
        for f in range(fold_count):
            fit_rows = train.subset(fold != f)
            test = fold == f
            model = Logit(columns, kind_columns, raw, fit_rows).fit(fit_rows, penalty, co_multiplier)
            logp[test] = model.log_probabilities(train.subset(test))
        loss = metrics(logp, train)
        selection.append({
            "decision_groups": [g for g, c in decision_groups.items() if set(c) <= set(columns)],
            "kind_columns": [kind_names[i] for i in kind_columns],
            "penalty": penalty, "co_multiplier": co_multiplier, "cross_validation": loss,
        })
        print(json.dumps(selection[-1]), file=sys.stderr)
        return loss["log_loss"]

    columns = list(base)
    kind_columns = list(v1_kind)
    penalty, co_multiplier = 0.01, 10.0
    best = cross_validate(columns, kind_columns, penalty, co_multiplier)
    kept = []
    candidates = [("kind", g) for g in kind_groups] + [("decision", g) for g in decision_groups]
    for kind_of, group in candidates:
        if kind_of == "kind":
            trial = (columns, kind_columns + kind_groups[group])
        else:
            trial = (columns + decision_groups[group], kind_columns)
        loss = cross_validate(trial[0], trial[1], penalty, co_multiplier)
        if loss < best - args.threshold:
            best = loss
            columns, kind_columns = trial
            kept.append(group)
    for p in [0.003, 0.03, 0.1]:
        loss = cross_validate(columns, kind_columns, p, co_multiplier)
        if loss < best - args.threshold:
            best, penalty = loss, p
    if "co" in kept:
        for m in [3.0, 30.0]:
            loss = cross_validate(columns, kind_columns, penalty, m)
            if loss < best - args.threshold:
                best, co_multiplier = loss, m

    procedures = {
        args.name: (columns, kind_columns, penalty, co_multiplier),
        "v1 features, refit": (v1_decision, v1_kind, 0.01, 1.0),
    }
    holdout_logp = {}
    if args.protocol == "v2":
        # The holdout probabilities, fold by fold.
        holdout_fold, holdout_count = map_folds(holdout)
        for name, (cols, kcols, pen, mult) in procedures.items():
            logp = np.zeros((holdout.n, C))
            for f in range(holdout_count):
                fit_rows = Data.join(train, holdout.subset(holdout_fold != f))
                model = Logit(cols, kcols, raw, fit_rows).fit(fit_rows, pen, mult)
                logp[holdout_fold == f] = model.log_probabilities(holdout.subset(holdout_fold == f))
            holdout_logp[name] = logp
        # The same procedure without the holdout rows in the fit, for the
        # effect of the extra rows.
        dev = Logit(columns, kind_columns, raw, train).fit(train, penalty, co_multiplier)
        holdout_logp[f"{args.name}, train rows only"] = dev.log_probabilities(holdout)
    else:
        # The protocol of human-v1: each model is fitted on --train only.
        for name, (cols, kcols, pen, mult) in procedures.items():
            fitted = Logit(cols, kcols, raw, train).fit(train, pen, mult)
            holdout_logp[name] = fitted.log_probabilities(holdout)
            if name == args.name:
                dev = fitted
    reference = FileModel(args.reference, names)
    holdout_logp["human-v1 (file)"] = reference.log_probabilities(holdout)

    if args.protocol == "v2":
        full = Data.join(train, holdout)
        model = Logit(columns, kind_columns, raw, full).fit(full, penalty, co_multiplier)
    else:
        full = train
        model = dev
    summary = {
        "rows": {"train": train.n, "holdout": holdout.n, "final": full.n},
        "skipped": {"train": dict(train.skipped), "holdout": dict(holdout.skipped)},
        "kept_groups": kept,
        "penalty": penalty,
        "co_multiplier": co_multiplier,
        "converged": model.converged,
        "selection": selection,
        "holdout": {name: report(logp, holdout, names) for name, logp in holdout_logp.items()},
        "greedy_argmax_top1": round(float(np.mean(holdout.greedy == holdout.y)), 4),
    }
    fit = {
        "train": [Path(p).name for p in (args.train + args.holdout if args.protocol == "v2" else args.train)],
        "holdout": [] if args.protocol == "v2" else [Path(p).name for p in args.holdout],
        "excluded_maps": sorted(excluded),
        "choices": "legal kinds; no build is not fitted",
        "penalty": penalty,
        "co_multiplier": co_multiplier,
        "kept_groups": kept,
        "holdout_log_loss": summary["holdout"][args.name]["all"]["log_loss"],
        "holdout_top1": summary["holdout"][args.name]["all"]["top1"],
    }
    json.dump(model.to_file(decision, kind_names, args.name, fit), open(args.model, "w"), indent=1)
    json.dump(summary, open(output / "fit-summary.json", "w"), indent=1)
    for name, values in summary["holdout"].items():
        print(name, json.dumps({k: values.get(k) for k in ["all", "turn_multiset_match", "copter_veto_condition"]}))


if __name__ == "__main__":
    main()
