# Arithmetic

All normative arithmetic is integer arithmetic unless a rule explicitly states
otherwise. Implementations MUST NOT use binary floating point to approximate a
specified rational calculation.

Every division rule must state when rounding occurs and whether it uses floor,
ceiling, truncation toward zero, or another operation. Algebraically equivalent
expressions are not interchangeable when they move a rounding point.

Quantities are unbounded mathematical integers while evaluating a transition.
They are checked or clamped only at an explicitly specified step.
Implementation integer overflow is never AWVM behavior.

Exact unit HP uses integer points. A living unit has HP in `[1,100]`; damage may
produce zero, at which point the unit is removed according to transition
semantics. Display HP is derived and is never authoritative.
