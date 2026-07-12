# Combat data

The base-damage table is a relation:

```text
base-damage(attacker-kind, defender-kind) -> integer | cannot-attack
```

An absent defender entry means the attacker cannot attack that unit kind. Zero
is a damage value and MUST NOT be used to mean cannot attack.

The AWBW damage chart reports the effective weapon selected for a unit matchup,
not separate named-weapon matrices. AWVM therefore preserves the unit-pair
relation without inventing weapon names. `weapon_policy` records whether a unit
has only an ammo-consuming weapon, only an unlimited weapon, or falls back to
an unlimited weapon when its ammo-consuming weapon is unavailable or has no
ammo.

The profile `weapons.json` preserves ammo and unlimited matchup matrices.
With sufficient ammo, selection tries ammo and then unlimited. Otherwise it
skips ammo and tries unlimited. The selected entry supplies base damage and
ammo cost; absence from both matrices means cannot attack.

Direct versus indirect fire, range, moving-and-firing restrictions, ammo
consumption, and counterattacks are behavioral semantics. Base damage alone
does not determine whether an attack command is legal.
