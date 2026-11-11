"""Fit the build model to human build decisions.

The inputs are the row files of `build_policy extract`. The model is a
conditional logit over the legal kinds of the site:

- The rows of games with fog and the rows where the built kind is not
  legal are not used.
- The commander features are not standardized, and they get a larger
  penalty. Thus a commander with few rows gets a weight near zero.
- The script selects the feature groups and the penalty by a cross
  validation over the maps of `--train`, with `--folds` folds of maps. Each
  map is in one fold only, because the games of one map are similar. The
  script adds one group at a time, in a fixed order, and keeps a group only
  when it decreases the cross-validated log loss by more than `--threshold`.
- The model is fitted on the `--train` rows only. The `--holdout` rows get
  the probabilities of this model.

The script writes the model file (`--model`) and a summary with the holdout
metrics (`--summary`). See `assets/ai-diagnostics/build-policy/README.md`.
"""

import argparse
import json
import sys
from collections import Counter

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


class Data:
    """The build rows of some row files as arrays."""

    def __init__(self, paths):
        rows = []
        self.skipped = Counter()
        for path in paths:
            with open(path) as handle:
                for line in handle:
                    row = json.loads(line)
                    if row["fog"]:
                        self.skipped["fog"] += 1
                    elif not row["chosen_legal"] or not row["legal"]:
                        self.skipped["illegal choice"] += 1
                    else:
                        rows.append(row)
        if not rows:
            sys.exit(f"no usable rows in {' '.join(paths)}")
        self.n = len(rows)
        self.x = np.array([row["decision"] for row in rows], dtype=float)
        self.z = np.zeros((self.n, C, len(rows[0]["kind"][0])))
        self.mask = np.zeros((self.n, C), dtype=bool)
        self.y = np.array([INDEX[row["chosen"]] for row in rows], dtype=int)
        self.map = np.array([row["map"] for row in rows], dtype=np.int64)
        for i, row in enumerate(rows):
            for kind, features in zip(row["legal"], row["kind"]):
                self.mask[i, INDEX[kind]] = True
                self.z[i, INDEX[kind]] = features

    def subset(self, keep):
        other = object.__new__(Data)
        for name, value in vars(self).items():
            if isinstance(value, np.ndarray):
                setattr(other, name, value[keep])
        other.n = int(np.sum(keep))
        return other


def map_folds(data, folds):
    """Put the maps into `folds` groups. The map with the most rows goes
    first, into the group with the fewest rows. Return the fold of each
    row."""
    maps, counts = np.unique(data.map, return_counts=True)
    size = [0] * folds
    fold_of = {}
    for index in sorted(range(len(maps)), key=lambda i: (-counts[i], maps[i])):
        target = min(range(folds), key=lambda f: (size[f], f))
        fold_of[maps[index]] = target
        size[target] += counts[index]
    return np.array([fold_of[m] for m in data.map])


def log_softmax(u, mask):
    u = np.where(mask, u, -np.inf)
    shifted = u - np.max(u, axis=1, keepdims=True)
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
        logp = log_softmax(b + x @ w.T + z @ v, mask)
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
        # A kind that no row chooses keeps zero decision weights.
        gw[~kind_mask] = 0.0
        return loss, np.concatenate([gb, gw.ravel(), gv])

    def fit(self, data, penalty, co_multiplier):
        x, z = self.design(data)
        kind_mask = np.bincount(data.y, minlength=C) > 0
        column_penalty = np.where(self.raw, penalty * co_multiplier, penalty)
        self.kind_penalty = penalty
        theta = np.zeros(C + C * self.d + self.k)
        result = minimize(
            self.objective, theta, jac=True, method="L-BFGS-B",
            args=(x, z, data.mask, data.y, column_penalty[None, :], kind_mask),
            options={"maxiter": 4000, "maxfun": 8000},
        )
        self.theta = result.x
        self.converged = bool(result.success)
        return self

    def log_probabilities(self, data):
        x, z = self.design(data)
        b, w, v = self.unpack(self.theta)
        return log_softmax(b + x @ w.T + z @ v, data.mask)

    def to_file(self, names, kind_names, name):
        b, w, v = self.unpack(self.theta)
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
            "kinds": {
                kind: {"intercept": float(b[i]), "weights": [float(x) for x in w[i]]}
                for i, kind in enumerate(KINDS)
            },
        }


class FileModel:
    """A model file, evaluated as the Rust code does."""

    def __init__(self, path, names):
        file = json.load(open(path))
        self.columns = [names["decision"].index(n) for n in file["decision_features"]]
        self.kind_columns = [names["kind"].index(n) for n in file["kind_features"]]
        self.mean = np.array(file["decision_mean"])
        self.scale = np.array(file["decision_scale"])
        self.kmean = np.array(file["kind_mean"])
        self.kscale = np.array(file["kind_scale"])
        self.v = np.array(file["kind_weights"])
        self.b = np.array([file["kinds"][k]["intercept"] for k in KINDS])
        self.w = np.array([file["kinds"][k]["weights"] for k in KINDS])

    def log_probabilities(self, data):
        x = (data.x[:, self.columns] - self.mean) / self.scale
        z = (data.z[:, :, self.kind_columns] - self.kmean) / self.kscale
        z = np.where(data.mask[:, :, None], z, 0.0)
        return log_softmax(self.b + x @ self.w.T + z @ self.v, data.mask)


def metrics(logp, data):
    """The log loss, the top-1 rate, and the argmax share of each kind next to
    the human share."""
    choice = np.argmax(np.where(data.mask, logp, -np.inf), axis=1)
    human = np.bincount(data.y, minlength=C) / data.n
    argmax = np.bincount(choice, minlength=C) / data.n
    return {
        "rows": data.n,
        "log_loss": round(float(-np.mean(logp[np.arange(data.n), data.y])), 4),
        "top1": round(float(np.mean(choice == data.y)), 4),
        "shares": {
            KINDS[i]: {"human": round(float(human[i]), 4), "argmax": round(float(argmax[i]), 4)}
            for i in np.argsort(-human)
            if human[i] >= 0.0005 or argmax[i] >= 0.0005
        },
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--names", required=True, help="the output of `build_policy names`")
    parser.add_argument("--train", nargs="+", required=True, help="row files")
    parser.add_argument("--holdout", nargs="+", required=True, help="row files")
    parser.add_argument("--folds", type=int, default=6, help="the number of map folds")
    parser.add_argument("--reference", help="a model file to score on the holdout rows")
    parser.add_argument("--threshold", type=float, default=0.002)
    parser.add_argument("--name", default="human-v2a")
    parser.add_argument("--model", required=True, help="the model file to write")
    parser.add_argument("--summary", required=True, help="the summary file to write")
    args = parser.parse_args()
    names = json.load(open(args.names))
    decision, kind_names = names["decision"], names["kind"]

    train = Data(args.train)
    holdout = Data(args.holdout)
    print(f"rows: train {train.n}, holdout {holdout.n}", file=sys.stderr)
    if args.folds < 2 or len(np.unique(train.map)) < 2:
        parser.error("the cross validation needs two or more folds and --train maps")

    def columns_of(predicate):
        return [i for i, n in enumerate(decision) if predicate(n)]

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
    base_kind = [kind_index[n] for n in ["cost", "overspend", "counter", "own_kind_count"]]
    kind_groups = {
        "own_class": ["own_class_count", "own_class_share"],
        "built_kind": ["built_turn_kind", "built_turn_class"],
        "funds_after": ["remainder", "fill_short"],
        "prey": ["prey_share", "predator_share", "prey_value"],
        "counter_near": ["counter_near"],
        "co_combat": ["co_attack", "co_defense"],
    }
    kind_groups = {g: [kind_index[n] for n in group] for g, group in kind_groups.items()}
    raw = set(co)
    selection = []
    fold = map_folds(train, args.folds)

    def cross_validate(columns, kind_columns, penalty, co_multiplier):
        logp = np.zeros((train.n, C))
        for f in range(args.folds):
            fit_rows = train.subset(fold != f)
            model = Logit(columns, kind_columns, raw, fit_rows).fit(fit_rows, penalty, co_multiplier)
            logp[fold == f] = model.log_probabilities(train.subset(fold == f))
        loss = metrics(logp, train)
        selection.append({
            "decision_groups": [g for g, c in decision_groups.items() if set(c) <= set(columns)],
            "kind_columns": [kind_names[i] for i in kind_columns],
            "penalty": penalty, "co_multiplier": co_multiplier,
            "log_loss": loss["log_loss"], "top1": loss["top1"],
        })
        print(json.dumps(selection[-1]), file=sys.stderr)
        return loss["log_loss"]

    columns, kind_columns = list(base), list(base_kind)
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

    model = Logit(columns, kind_columns, raw, train).fit(train, penalty, co_multiplier)
    holdout_metrics = {args.name: metrics(model.log_probabilities(holdout), holdout)}
    if args.reference:
        reference = FileModel(args.reference, names)
        holdout_metrics["reference"] = metrics(reference.log_probabilities(holdout), holdout)
    summary = {
        "rows": {"train": train.n, "holdout": holdout.n},
        "skipped": {"train": dict(train.skipped), "holdout": dict(holdout.skipped)},
        "kept_groups": kept,
        "penalty": penalty,
        "co_multiplier": co_multiplier,
        "converged": model.converged,
        "selection": selection,
        "holdout": holdout_metrics,
    }
    with open(args.model, "w") as handle:
        json.dump(model.to_file(decision, kind_names, args.name), handle, indent=1)
    with open(args.summary, "w") as handle:
        json.dump(summary, handle, indent=1)
    for name, values in holdout_metrics.items():
        print(name, values["log_loss"], values["top1"])


if __name__ == "__main__":
    main()
