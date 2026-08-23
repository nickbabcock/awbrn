# The ladder

One JSON file for each weighting that is not compiled in. The file name is the
name a seat calls it by: `hold-0.4.json` plays as `--first hold-0.4`.

A file names the weights it moves, and in `base` the compiled weighting it
moves them from. A file without a `base` moves from the defaults.

```json
{ "base": "counter", "hold": 0.4 }
```

The arena writes a file here for you, with every field filled in, so that a
contender still means the same thing after the weighting it was swept from
moves:

```
arena --first counter --weights sweep/hold-0.4.json --freeze hold-0.4
```

`arena --round-robin` plays this directory and the compiled weightings against
each other. A weighting that wins its round joins later rounds by staying
here, and nothing has to be rebuilt for it to.

Only `.json` files are read, and a name one of the compiled weightings already
holds is refused.
