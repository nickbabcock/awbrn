---
version: 1
slug: "web-src-matches-components-matchmenu-tsx"
primary_target: "web/src/matches/components/MatchMenu.tsx"
related_targets: ["web/src/matches/screens/MatchActivePage.tsx","web/src/matches/components/UnitActionMenu.tsx"]
---

# Surface Brief: Giving the Match Up

Mode: **Operate**. One command, and the player must never send it by accident
or fail to find it when they mean it.

## Job and audience

A player who has decided the match is over for them: a lost position, a match
they no longer want, or a seat they are leaving so the opponent is not left
waiting on a clock. They may be on the move or not — most resignations happen
while somebody else is thinking.

## Outcome and proof

**Primary task:** leave the match, deliberately, in two presses.

**Success:** the player who means to resign finds the command without
searching, and no player ever resigns by a mis-tap on the strip.

**Proof it works:** resignation is never one press from the key a player uses
every turn, and the press that commits it is the one red key in the system.

## Selected direction

**Match commands are a menu, not a key.** The strip carries a `Match` key that
opens the same window the board opens for a production site or a destination:
board-anchored under a mouse, a bottom sheet under a thumb. It holds one
command today and still opens as a menu, because a command that is sometimes a
menu and sometimes a bare key is a command a player looks for twice.

**Resignation is deleting a unit, raised to the match.** It is asked the way
`UnitActionMenu` asks about delete: the list is replaced inside the frame it
already occupies, the header names what is being asked, the confirmation opens
on the harmless answer, and the commit is the one key whose cursor is red
rather than orange. No dialog is stacked on the menu — the menu is the
confirmation.

**What it adds over a deleted unit** is one line of prose: a deleted unit
leaves the board where the player can see it, and a resigned seat leaves a
record they cannot.

## Scope and boundaries

**In scope:** the `Match` key on the board strip, the menu, its confirmation,
and the states of a seat that has left.

**Untouched:** the board's own menus, the clock, the roster, and what the
engine does with a resignation.

**Anti-goals:**

- No visible `Resign` key on the strip. A match-ending command must not sit in
  thumb reach beside the key pressed every turn, and the strip carries one
  loud color.
- No second confirmation over the menu.
- No warning prose in the list state. The question is asked once, where it is
  answered.

## States

Off-turn and on-turn are the same command and the same menu: a seat may resign
whenever it wants to, and the host applies it through AWVM's adapter operation
rather than through the on-turn `resign` command AWBW records. Disconnected,
the rows are inert and the menu says why. A seat that has already left keeps no
match commands at all: the `Match` key and the `End turn` key both go with it.
