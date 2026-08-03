# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Advance Wars By Web (AWBW) players. Two jobs, in priority order:

1. **Play a live match.** A player joins or creates a match, takes turns against
   opponents, and watches the board update in real time. This is the job the
   product is positioned around and the one future work must favor when the two
   compete for attention or screen space.
2. **Review a replay.** A player loads an AWBW replay archive and steps through
   the game turn by turn to study what happened.

Replay review is currently the more finished surface; that is a state of the
implementation, not a statement of priority.

## Product Purpose

AWBRN, pronounced auburn, is a browser-native client and toolkit for Advance
Wars By Web. It lets players play matches and review battles without leaving the
browser, backed by a Rust and WebAssembly engine.

Success is a player choosing AWBRN over the incumbent site to play a real match.

## Positioning

**A modern, well-crafted interface is the position, not a side effect of it.**
The incumbent experience is dated; being genuinely well designed is the reason a
player switches. This claim is only true while the craft is visible in ordinary
use, so interface quality is a product requirement, not a polish backlog.

**Mobile must be viable.** Playing and reviewing on a phone is an explicit goal,
not a responsive courtesy. A design that only works at desktop widths does not
satisfy the position.

## Operating Context

- Players arrive knowing AWBW conventions: COs, funds, unit counts, per-turn
  actions, fog of war, day counters.
- Live match play is real time and multi-party: a match has spectators as well
  as players, and state arrives over a websocket while the player watches.
- Replay review starts with a local `.zip` replay archive the player drops or
  selects; there is no upload-first step.
- Sessions are mixed-device. A player may follow a match on a phone and play a
  turn at a desk.

## Capabilities and Constraints

Confirmed capabilities in the web app:

- Match browse, match creation, per-match play view, and a personal match list.
- Replay loading, playback, and per-player roster with CO portraits, funds, and
  unit statistics.
- Email/password accounts and sessions; signed-out visitors can still browse and
  review.
- Canvas-rendered battlefield driven by the WebAssembly engine, with input
  bridged from the DOM.

Technical constraints:

- **Cloudflare Workers deployment.** SSR runs on Workers, live match state runs
  in Durable Objects. Anything requiring a long-lived conventional server or
  large server-side dependencies is out of reach.
- **Rendering is a canvas surface, not DOM.** The board is drawn by the engine;
  the interface frames it rather than composing it.
- **Astryx design system.** UI is built from `@astryxdesign/core` components and
  its tokens, per `web/AGENTS.md`: no raw layout elements, no hardcoded colors or
  spacing, brand and accent through the theme. This came from the codebase, not
  from a user commitment, so it is a current technical constraint rather than a
  permanent identity decision.
- Rules behavior derives from AWVM, an executable specification with a versioned
  ruleset (`spec/`). Rules displayed in the interface must match it rather than
  being restated by hand.

Undecided: monetization, hosting/pricing claims, and whether a public account
system beyond email/password is planned. Do not invent these.

## Brand Commitments

- The product name is **AWBRN**, pronounced "auburn". The acronym reads Advance
  Wars By Rust (New).
- **Real Advance Wars sprite assets are used** for CO portraits, unit sprites,
  and terrain tiles. The interface must work _with_ pixel art at its native
  character rather than abstracting it away or smoothing it out.
- **No claim of AWBW affiliation.** AWBRN is independent. Never imply
  endorsement, partnership, official status, or a relationship with AWBW or
  Nintendo/Intelligent Systems.

## Evidence on Hand

- Real sprite and portrait assets under `assets/`, surfaced through an atlas.
- Real replay archives under `assets/replays/` for testing playback.
- The AWVM specification and conformance fixtures under `spec/`.
- No testimonials, user counts, benchmarks, press, or case studies exist. Do not
  fabricate any.

## Product Principles

1. **Live play leads.** When live match play and replay review compete for
   priority, navigation, or space, live play wins.
2. **Craft is the product claim.** Interface quality is the reason to switch, so
   a visibly unfinished surface is a product defect.
3. **Phone-viable, not phone-tolerated.** Every surface must be genuinely usable
   at phone width, including the board itself.
4. **Honor the pixels.** Sprite art keeps its native crispness and character;
   the surrounding interface is built to frame it, not to compete with or
   sanitize it.
5. **The spec is the source of rules truth.** Anything the interface says about
   rules, costs, or outcomes traces to AWVM.

## Accessibility & Inclusion

No formal standard has been set. Two product-specific needs are established:

- Phone-width usability is a hard requirement (see Principles).
- Faction identity is communicated by color today. Because color is doing real
  informational work, faction and player state must also be readable without
  relying on hue alone.
