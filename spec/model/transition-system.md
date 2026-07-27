# AWVM transition system

AWVM is a deterministic, input-labelled transition system with partial
information. For a ruleset profile `R`, define:

```text
S  authoritative states
C  canonical command intents
VC validated commands
V  violations
Q  explicit random-token sequences
E  ordered semantic event sequences
O_p observations available to player p
Γ  derived rule-evaluation context
```

The Rust reference implementation exposes these public operations:

```text
execute(S, C, Q) -> execution | error
observe(visibility, S, p) -> O_p | error
observe-events(visibility, S, S', E, p) -> E_p | error
```

`execute` combines the validation and reduction stages described below.
`execution` contains the resulting state, ordered events, and number `n` of
random tokens consumed. The ruleset is selected by `S.ruleset`; the bundled
visibility implementation is selected explicitly by the caller.

The notation below uses `validate`, validated commands, and `check-invariants`
to define the semantics. They are conceptual operators rather than separate
public Rust functions.

## Transition notation

An accepted atomic transition is written:

```text
S --[C, Q[0:n]] / E--> S'
```

and exists exactly when:

```text
validate(R, S, C) = VC
execute(R, S, VC, Q) = (S', E, n)
```

For fixed `R`, `S`, `C`, and `Q`, the result MUST be unique. Implementations
MUST NOT inspect a clock, host RNG, network, database, iteration hash order, UI
state, or any other ambient input.

## Domains and well-formedness

The core state domain may be infinite, but every individual state, command,
token sequence, and event sequence is finite. A ruleset MAY impose finite map,
player, unit, funds, day, identifier, or counter limits. Such bounds belong to
the ruleset unless they are structural AWVM bounds.

`check-invariants` determines whether `S` is an admissible state. Both semantic
operations require an admissible pre-state:

```text
check-invariants(R, S) = []
```

An invalid state is an integration/specification failure, not a command
violation. An implementation MUST report it distinctly and MUST NOT attempt to
repair the state silently.

Every successful result MUST also be admissible. Producing an invalid `S'` is
an implementation or specification error even if the command was legal.

## Validation

Validation answers whether the submitted intent may begin from the supplied
state. It MUST:

- be pure and consume no random token;
- perform command-shape, authority, phase, reference, and ruleset legality
  checks;
- return one stable primary violation according to a command-specific
  precedence order;
- reveal no authoritative secret that the actor could not distinguish through
  their observation; and
- bind all resolved references and derived values needed to prevent execution
  from reinterpreting the intent.

A validated command is an abstract semantic value, not a required wire format.
Conceptually it contains:

```text
(ruleset identity, pre-state identity, canonical intent, resolved references,
 derived costs/ranges, declared random requests)
```

Implementations MAY represent it as an immutable object, capability, digest, or
internal type. They MUST reject its reuse against another state. This prevents
time-of-check/time-of-use changes.

Validation may use the authoritative state to protect integrity, but its
observable rejection must respect fog. A hidden obstruction that AWBW treats
as a trap therefore does not cause a secret-revealing validation result; it is
resolved during execution under the fog specification.

## Execution

Execution applies one validated intent atomically. It MUST:

1. verify the validated command is bound to `R` and `S`;
2. request and consume tokens only at specified stochastic decision points;
3. apply state changes in the feature's normative order;
4. emit events in the same normative order;
5. run every automatic phase hook reached by the command;
6. stop immediately if a terminal outcome is established;
7. check invariants on the result; and
8. return `S'`, `E`, and the exact token count.

Rejected validation produces no state change, event, or random consumption.
Execution errors also produce no semantic successor state; transactional
implementations MUST roll back partial host mutations.

## Random tokens

Randomness is a finite typed input sequence, not an RNG object or seed. A token
has a stable `type` and a closed scalar `value`. Feature specifications define
the allowed domain and mapping for each type, for example:

```json
{ "type": "combat-good-luck", "value": 7 }
{ "type": "weather-selection", "value": "rain" }
```

Token consumption is left-to-right and demand-driven. A reducer MUST NOT
consume a token for a decision point it does not reach. A wrong token type,
out-of-domain value, or missing token is an execution error. Extra trailing
tokens are permitted at the API boundary and are identified by `n`; a
conformance case MUST assert the expected `n`.

Random-token types and mappings are ruleset-versioned. A seed-to-token adapter
is outside AWVM and cannot establish conformance by seed equality.

## Events

Events are an ordered semantic account of the accepted transition. They are not
commands, replay instructions, animations, or a substitute for state. Each
event has a closed `type` and the minimum payload needed to describe its fact.

Events MUST be sufficient to distinguish normatively different transitions,
including random outcomes and automatic phase effects. They MUST NOT contain
presentation text or database identifiers. Event order is significant.

`E` is authoritative and may contain secrets. A recipient receives only
`observe-events(..., p)`. The authoritative event schema is
`schema/event.schema.json` (`model/events.md`), the recipient schema is
`schema/observed-event.schema.json`, and per-player redaction is defined by
`model/observation.md` and `semantics/fog.md` as feature
`fog-observation-v1`.

## Derived context Γ

`Γ` is a pure, ephemeral evaluation environment:

```text
Γ = context(R, S, actor, operation, subjects, position)
```

It may contain applicable settings, weather, terrain traits, ownership/team
relations, active commander and power effects, and unit capabilities. It is not
stored independently in authoritative state and cannot be mutated.

Base rules query named effective-value operators rather than branch on a
commander identity:

```text
effective-move(Γ, unit)
effective-cost(Γ, unit, terrain)
effective-attack(Γ, attacker, defender)
effective-defense(Γ, defender, attacker)
effective-capture(Γ, unit, tile)
```

Each operator MUST define a typed modifier algebra, applicability predicates,
composition order, rounding points, clamps, and prohibitions. Commander
profiles contribute modifiers to these operators. A profile-specific exception
that cannot use the common algebra MUST be a named operator with explicit
semantics, not an implementation-local `if commander == ...` branch.

Replay evidence shows that commander powers are not limited to scalar combat
modifiers. The closed effect algebra will also need typed transition operators
for at least: HP changes, fuel/ammo refill or scaling, unit action-state changes,
unit spawning, and general funds changes. These
are effects produced by the single
`activate-power` intent; replay result fields are evidence for the operators,
not additional commands or trusted state patches.

Two implementations are conformant only if they produce equal effective values
and transitions; their internal representation of `Γ` need not match.

## Observation and noninterference

Authoritative reality and player knowledge are separate:

```text
O_p = observe(R, S, p)
```

`observe` is pure, deterministic, consumes no randomness, and does not mutate
`S`. Commands are submitted from information in `O_p` but are evaluated against
`S`. Hidden reality may change execution through a documented trap transition;
it MUST NOT leak through a more specific validation violation.

The fog specification will define visibility, remembered public terrain,
concealment, allied sharing, private resources, event redaction, and an
observational-equivalence test. Until then, no fog-sensitive transition is
fully conformant.

## Atomic commands and automatic closure

A canonical command expresses a complete semantic intent. Movement plus an
ordinary follow-up action is one command, such as `move-attack` or
`move-capture`; UI selection and cancellation are not commands.

After the direct effect, execution performs automatic closure: immediate
victory checks and, for boundary commands, the complete `turn-end` and next
`turn-start` loop from `phases.md`. The returned state is therefore normally a
stable `unit-action` or terminal `finished` state.

If a future rule genuinely requires a player choice after mutation, it MUST add
a closed phase/substate and continuation-command family with invariants. An
implementation MUST NOT invent such intermediate states for UI convenience.

## Conformance properties

For every supported feature, fixtures SHOULD establish:

- determinism for equal inputs;
- rejection purity and zero token consumption;
- invariant preservation;
- exact event and token order;
- validated-command state binding;
- terminal short-circuiting;
- canonical serialization equality; and
- observational noninterference where hidden information differs.
