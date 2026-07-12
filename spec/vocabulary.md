# Canonical vocabulary

Canonical identifiers are lowercase ASCII kebab-case. They are semantic names,
not programming-language symbols, display labels, storage keys, or map glyphs.
Identifiers are immutable within a ruleset revision.

## Coordinates and boards

A position is `[x, y]`. The origin `[0, 0]` is the upper-left tile; `x`
increases rightward and `y` increases downward.

A terrain grid is an array of rows and is indexed `terrain[y][x]`. A ruleset
profile defines the allowed cell values and their mapping to canonical terrain
identifiers. Fixtures and states MUST NOT define local legends.

For example, a profile can map an external numeric terrain code to a canonical
semantic identifier such as `plain`, `river-ne`, or `city-orange-star`. The
external encoding is not itself the semantic identity.

## Identifier categories

The core vocabulary defines the roles of identifiers, while each ruleset
profile supplies its closed sets. Relevant categories include:

```text
terrain unit movement-class unit-domain weather player team
commander power weapon phase command event violation
```

## Initial profile identifiers

Movement classes:

```text
foot boot treads tires air sea lander pipe
```

Unit domains:

```text
ground air sea
```

Weather:

```text
clear rain snow
```

Units:

```text
anti-air apc artillery b-copter battleship black-boat black-bomb bomber
carrier cruiser fighter infantry lander md-tank mech mega-tank missile
neo-tank piperunner recon rocket stealth sub t-copter tank
```

Canonical commands:

```text
move-wait move-attack move-capture move-load move-join move-supply
move-repair move-hide move-reveal move-launch move-explode unload delete-unit
produce-unit activate-power tag end-turn resign
```

These names are used in state, tables, rules, diagnostics, and events. Adapters
are responsible for mapping external names and codes to them.
