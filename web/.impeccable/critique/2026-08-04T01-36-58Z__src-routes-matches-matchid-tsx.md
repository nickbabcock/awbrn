---
target: src/routes/matches/$matchId.tsx
total_score: 18
max_score: 40
na_heuristics:
p0_count: 2
p1_count: 2
timestamp: 2026-08-04T01-36-58Z
slug: src-routes-matches-matchid-tsx
---

Method: dual-agent (A: a5ad9dd330f44fb28 · B: a458df6bb1a8bdba5)

# Critique: `/matches/$matchId` — the live match surface

**Scope note:** `src/routes/matches/$matchId.tsx` is a 36-line router shim. The real surface is `MatchActivePage.tsx` (phase `active`) and `MatchLobbyPage.tsx` (everything else). Both were reviewed. Neither agent had browser automation — the Claude-in-Chrome extension is not connected — so nothing below rests on a screenshot. Assessment A did fetch real SSR output for a live lobby at `localhost:5173/matches/oklp4dwp4r6ru`; there is no active match in the local DB, so every claim about `MatchActivePage` is from source. Sizes are computed from theme tokens, not observed.

## Design Health Score

| #         | Heuristic                       | Score     | Key Issue                                                                                                                                |
| --------- | ------------------------------- | --------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| 1         | Visibility of System Status     | 2         | No day, funds, or turn owner during a live match; the lone connection dot sits _below_ a 520px board (`MatchActivePage.tsx:184-195`)     |
| 2         | Match System / Real World       | 2         | AWBW players expect a HUD and get none; lobby speaks app ("Economy", "Choose CO and army look") not game                                 |
| 3         | User Control and Freedom        | 2         | An active match has no leave, resign, or back-to-matches; `MatchActivePage` renders zero controls                                        |
| 4         | Consistency and Standards       | 2         | Two board framings in one product — `ReplayPage.tsx:217-221` aspect-locked with a HUD vs `MatchActivePage.tsx:175` fixed 520px with none |
| 5         | Error Prevention                | 2         | Optimistic rollback is solid (`MatchLobbyPage.tsx:79-116`), but `Leave` fires with no confirm and seat claims race off stale state       |
| 6         | Recognition Rather Than Recall  | 2         | Unlabeled canvas, no controls hint; army identity carried by a 14px insignia with no text name on the active page                        |
| 7         | Flexibility and Efficiency      | 1         | No shortcuts, no player/spectator distinction, no way to act; join link is inert text                                                    |
| 8         | Aesthetic and Minimalist Design | 3         | Handsome system, but triple-stacked chrome and a 67px title fight the content                                                            |
| 9         | Error Recovery                  | 1         | `error` and `spectatorNotice` frames silently dropped (`MatchActivePage.tsx:40-59`); desync is invisible                                 |
| 10        | Help and Documentation          | 1         | None. No onboarding, no controls hint, no first-run state on the board                                                                   |
| **Total** |                                 | **18/40** | **Poor — major UX work required**                                                                                                        |

## Design Specificity Verdict

**The design system is authored for AWBRN. This surface is not.**

The theme is the real thing: one ink at `#16181D`, the two-part cast shadow (`awbrnTheme.ts:260`), faction cards wearing the army color as a 6px inset bar (`awbrnTheme.ts:73-81`), 20 real AWBW palettes, `image-rendering: pixelated` everywhere. A player recognizes the game from the chrome alone.

But the live-match screen — the product's stated #1 job — is a two-column SaaS detail page with a canvas dropped into the left card. `MatchActivePage.tsx:75-117` is: title, metadata line, 50/50 grid, board left, "Players" right. Strip the canvas and it is a generic entity-detail view. Nothing on it is a game HUD, and the engine already emits everything one would need: `PlayerRosterUpdated` carries `day`, `activePlayerId`, `funds`, `unitCount`, `unitValue`, `income` (`src/wasm/awbrn_wasm.d.ts:60-72`), and `game_runner.ts:150-154` already routes it into the store. `MatchActivePage` never imports `useGameStore`. The data is on the floor.

**Deterministic scan:** clean. `detect.mjs --json src/routes/matches src/matches src/components` exited 0 with an empty array — zero findings across the match components. That is a real result and also a blind spot: the regex engine reads HTML/CSS, so it cannot see design-system violations expressed as component props in TSX. Every violation in P2 below is one the detector structurally cannot catch. The 118 findings from the earlier whole-tree scan all live in `src/themes/awbrn.css`, correctly out of scope here, and all 118 remain sanctioned by DESIGN.md (faction bars, army palettes).

**Visual overlays:** none. Browser automation was categorically unavailable, so no injection was attempted and no overlay exists. No live server was left running.

## Overall Impression

The craft is real and it is in the wrong place. The replay surface got the game-client treatment — aspect-locked board with a comment explaining why ("so the map is never stretched on a phone", `ReplayPage.tsx:217-221`), a Day badge strip, a roster row with funds and income. The live-match surface, which PRODUCT.md says wins every priority contest, got a thinner HUD-less version of the same board sitting 200 lines away from the better one.

The single biggest opportunity is not a visual one: you cannot take a turn on the page whose job is taking turns. Everything else is downstream of that.

## What's Working

1. **Faction identity is genuinely hue-independent, by structure not by patch.** `factionCardVariants` (`awbrnTheme.ts:73-81`) pairs the army wash with the inset top bar, and `FactionCrest`/`FactionLogo` add a distinct 14px pixel insignia per army with a real `aria-label` plus `isLabelHidden` to avoid double-announcing (`FactionLogo.tsx:63-71`). PRODUCT.md's color-not-alone requirement is met in the component contract.
2. **The optimistic lobby mutation is correct and complete.** `onMutate` snapshots, patches only the current user's participant, `onError` restores, `onSuccess` writes server truth and invalidates both list queries (`MatchLobbyPage.tsx:79-116`). Ready/unready feels instant without lying about other players.
3. **Sprite handling honors the pixels.** `CoPortrait` scales through `background-size` on whole pixels with an explicit comment about avoiding transform resampling (`CoPortrait.tsx:22-31`). This is "craft visible in ordinary use" actually delivered.

## Priority Issues

**[P0] You cannot take a turn.**
`MatchActivePage.tsx:60` destructures `{ status }` and discards `sendMessage`; the engine emits no outbound action (`GameEvent` in `awbrn_wasm.d.ts:6` has no command variant); `handleGameEvent` ignores `UnitMoved`/`UnitBuilt`/`TileSelected`; and `endTurn` appears nowhere in `web/src` outside `match_protocol.test.ts:111`. The Rust play mode does full unit selection and destination selection (`crates/awbrn-client/src/modes/play/mod.rs:32-56`) and emits nothing.
**Why it matters:** the product's #1 job is unachievable on its own surface. A player who switches to AWBRN to play cannot play; `phase === "active"` routes to a spectator viewer labeled as play.
**Fix:** add a `GameEvent::CommandRequested { command }` emitted from `play/mod.rs` on destination confirm, forward it through a runner callback in `handleGameEvent`, and have `MatchActivePage` call `sendMessage({type:"command", command})`. Add a persistent HUD bar with `END TURN` in Silkscreen, enabled only when `activePlayerId` matches the viewer's slot from the `connected` frame.
**Suggested command:** `/impeccable shape`

**[P0] The lobby never updates, so a match never visibly starts.**
`MatchLobbyPage` never mounts `useMatchWebSocket`, and `matchDetailQueryOptions` (`matches.queries.ts:34-41`) sets no `refetchInterval` — there is no `refetchInterval` anywhere in `src/`. Players joining, readying, or the phase flipping to `active` are invisible until a manual reload. The banner "All players are ready. Starting the match…" (`MatchLobbyPage.tsx:212-214`) is only seen by whoever's own request flipped the phase.
**Why it matters:** the multi-party wait is the highest-anxiety moment before play, and it renders as a dead page.
**Fix:** mount `useMatchWebSocket(matchId, …)` in the lobby and write snapshots into the detail query cache; let the router re-render into `MatchActivePage` on `phase === "active"`. A 5s `refetchInterval` is the stopgap.
**Suggested command:** `/impeccable harden`

**[P1] No live HUD: no day, funds, turn owner, or unit counts.**
The data already flows into the store and the surface ignores it.
**Why it matters:** AWBW players read funds and turn order constantly. Without them the board is not readable as a game state.
**Fix:** reuse the replay pattern. `src/replay/RosterRow.tsx` already renders CO, funds, unit count, value, income, and an active-player `StatusDot`. Swap the ad-hoc player cards at `MatchActivePage.tsx:87-113` for it and add the Day badge strip from `ReplayPage.tsx:158-167`.
**Suggested command:** `/impeccable layout`

**[P1] Match end and connection loss are unhandled — the two moments the interface most needs to speak.**
When a match finishes, `phase !== "active"` routes back to `MatchLobbyPage` (`$matchId.tsx:32-35`), which has banners for `starting` and `active` only (`MatchLobbyPage.tsx:212-217`). You win a match and land on a lobby headed "Choose CO and army look" with every control greyed out — no winner, no result, no exit link. That is the last thing a player sees. Meanwhile `disconnected` maps to `"neutral"` (`MatchActivePage.tsx:201-206`) — a gray dot for "your live match dropped" — with backoff up to 30s (`match_websocket.ts:12`), no countdown, no manual reconnect.
**Why it matters:** peak-end rule. The end is currently the worst screen in the product.
**Fix:** add a completed/cancelled result panel (winner, day count, back to matches); handle `error` and `spectatorNotice` into a `Banner`; map `disconnected` to `warning`; move the status line above the board with `role="status"`.
**Suggested command:** `/impeccable harden`

**[P2] Design-system violations the detector cannot see.**
Three eyebrows, banned by DESIGN.md (`MatchLobbyPage.tsx:140-142` "Lobby setup", `:167-169` "Map", `:200-202` "Roster"). Metadata strips set in Nunito `type="large"` (`MatchActivePage.tsx:69-72`) when DESIGN.md uses that literal string — "Map 162795 · Fog off" — as its example of HUD voice. Triple-stacked chrome at `MatchActivePage.tsx:83→92→94` and `MatchLobbyPage.tsx:197→230→232`, which DESIGN.md names as the tell of a system that stopped thinking. And `Heading level={1} type="display-2"` resolves to `--font-size-4xl` = 4.1875rem ≈ 67px Bungee with no clamp anywhere, against a documented 37.9px.
**Why it matters:** the position is "craft is visible in ordinary use." These are the details the position is made of.
**Fix:** delete the eyebrows; switch metadata to `type="label"`; replace inner `Section variant="muted"` seat wrappers with the recessed tan well + dashed rule the system already defines; clamp the display size.
**Suggested command:** `/impeccable polish`

## Persona Red Flags

**Casey (distracted, one-handed, phone)** — the hard product requirement, and the worst-served:

- `touch-action: none` is set on every engine canvas (`dom_transport.ts:58`). A 520px-tall gesture sink on an ~844px screen means a drag started on the board never scrolls the page; the only scrollable strips are the 24px gutters.
- That compounds in the lobby: `MatchMapPreview` is a fixed `width={600}` surface inside an `overflowX: auto` card (`MatchMapPreview.tsx:53-61`). The horizontal scroll it depends on cannot be performed by touch, so the preview is permanently cropped at ~340 of 600px on a phone.
- At 390px the board is a 338 × 520 vertical slot — the wrong shape for a wide AW map, on the one surface that most needed the replay page's `aspectRatio: 3/2`.
- ~67px title eats most of the first viewport before any game content.
- Ready and Leave sit adjacent at 8px gap as `size="sm"` buttons, and Leave has no confirmation. A thumb-miss drops her out of the match.

**Sam (accessibility / keyboard / screen reader):**

- The board is an unlabeled `tabIndex={0}` canvas with no `aria-label`, no `role`, no fallback (`MatchActivePage.tsx:176-182`) — a focus stop that announces nothing and has no announced escape.
- Connection status has no `role="status"`/`aria-live`, so a drop is never announced — and reads twice manually, because `StatusDot` renders `role="img" aria-label={statusText}` beside a `<Text>` with the identical string.
- Five buttons labeled exactly `"Claim seat"` (`MatchLobbyPage.tsx:253-271`); seat index is conveyed by visual position only.
- `actionError` renders a `Banner` with no `role="alert"`.

**Alex (impatient power user):** nothing to press; the lobby is frozen until reload; the CO selector is 29 flat unsearchable options with no country grouping; the private join URL is inert text with no copy button; no per-match page `<title>`, so three open match tabs all read "AWBRN".

**Cognitive load: 5 of 8 checks fail** — single focus, grouping, visual hierarchy, ≤4 options, working memory. Three decision points blow the limit: five identical full-width orange "Claim seat" buttons, a 20-army flat faction popover, and a 29-option flat CO list.

## Minor Observations

- `statusText` says `"{mapName} loaded from match state"` (`MatchActivePage.tsx:162-170`) — implementation language shown to players in the slot that should read out game state.
- Private-link spectators break on an active match: the route loads with the join slug (`$matchId.tsx:18-22`) but `MatchActivePage.tsx:28` re-queries with `null`, so `canViewMatch` (`matches.server.ts:789-807`) rejects non-participants. The invite link fails exactly when the match goes live.
- `shareUrl` is `typeof window` guarded (`MatchLobbyPage.tsx:130-133`), so the private-link row pops in on hydration — layout shift on the row the host came for.
- The `connected` frame's `slotIndex` (`match_protocol.ts:47-50`) is never captured, so the client does not know whether the viewer is a player or a spectator.
- `StatusDot isPulsing` adds a looping opacity animation — a second gesture in a system whose stated vocabulary is exactly one. It does respect `prefers-reduced-motion`.
- Both match screens Card-wrap roster list items, which `AGENTS.md:24` forbids and `ReplayPage.tsx:175-193` gets right.

## Questions to Consider

1. If a player cannot take a turn here, what is `phase === "active"` routing them to — and should it honestly say "Spectate" until commands ship?
2. The replay surface already has the board framing, the HUD strip, and the funds readout. What does it say that the stated priority surface got a thinner version of a board that already existed 200 lines away?
3. Is "phone-viable" satisfied by a page you can barely scroll, or does the board need its own gesture contract — one-finger pan, two-finger page scroll — before that claim is true?
4. The lobby's whole job is the wait, and the wait is where it is completely inert. Should lobby and active match be one continuous websocket-backed screen rather than two routes separated by a manual refresh?
