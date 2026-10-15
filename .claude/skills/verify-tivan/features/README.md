# Tivan verification map

This directory is the maintained source for verifying what a teacher can do in Tivan. Read this index before driving the app. Then use the matching feature file as the recipe.

## Baseline preconditions

- A run started with `.claude/skills/verify-tivan/bin/tivan-up`. `tivan-doctor <run>` passes.
- The run's session is fresh from `tivan-up`, so the page shows the seed: three classes `Room 12 Morning` (24 students, selected), `Period 3 Science` (20), `Reading Club` (27), none grouped, no timer, Build mode.
- To get back to baseline mid-run, start a new run. Never clear or write `localStorage` by hand.
- Every drive command starts with `source target/verify/<run>/run.env &&` so `$SESSION` and `$EVIDENCE` are set.

## Driving conventions

- Drive through `agent-browser --session "$SESSION"` with `find role|label|placeholder … --name`, `press` and `fill`. Take a fresh `snapshot` after each change before using any `@eN` ref.
- `--name` matches substrings. Use `--exact` when one visible name contains another (`Ava` and `Remove Ava`).
- Read saved state with `eval "JSON.parse(localStorage.getItem('tivan.state'))" | jq …`. The selected class is `.doc.classes[] | select(.id == $doc.selected)`. The seed selects class id `0`.
- Feature files assume the seed's first class unless they say otherwise.

## Proof and skip reporting

- Use `tivan-capture <run> <label>` before and after each action under test.
- A mutation is proven only when the saved state shows it, and a capture after `reload` shows the same screen and state.
- Random output (group names, membership, oracle picks) is asserted by structure, never by value.
- Name the feature ID and entry point with every artifact you report.
- Report a path you could not reach with the command you ran and the precondition that was missing. Do not report it as verified through another path.

## Feature entry contract

Each feature file starts with an H1 title and one paragraph on the user-visible behavior. It then has exactly four H2 sections in this order:

1. `Sub-features`: short IDs, one line each.
2. `How to get to it (user POV)`: every entry point a teacher uses.
3. `Driving it with agent-browser`: starts with `Preconditions:`, then labeled bullets pairing an action with an exact command and its observable result.
4. `Gotchas`: traps that waste or invalidate a run.

Keep implementation details out. Name only user paths, stable handles, required state, commands and observable proof.

## Features

- [Classes](./classes.md): create, switch, rename and delete classes.
- [Roster](./roster.md): add, remove and paste students into a class.
- [Groups](./groups.md): choose a group count, make and reshuffle groups, move students, rename groups.
- [Whiteboard and oracle](./whiteboard.md): the projected group wall and the random student picker.
- [Timer](./timer.md): preset countdowns on the whiteboard, pause, cancel, Time's up, and the Build-mode chip.

## Proof status

Status as of 2026-10-15, rev `5f58097`:

- **Groups**: run end to end.
  - `groups-count`, `groups-make` and `groups-shuffle` passed, with reload proof for `groups-make`.
  - `groups-move` (including Escape-cancel) passed, proven by screen and saved state. Its reload capture was lost to the rename hang.
  - `groups-reroll` passed with reload proof in a separate run.
  - `groups-rename` **fails**: Enter in the `Group name` box hung the page in 4 of 6 tries. See the Groups gotchas.
  - Evidence: `target/verify/20261015-155559-59094/` (main journey) and `target/verify/20261015-155811-59592/` (reroll), plus `target/verify/20261015-154121-57727/` (an earlier hang).
- **Classes, Roster, Whiteboard and oracle, Timer**: written from the code and from one exploratory pass over their handles. They have not been run step by step. The first agent to run one fixes any step that does not match the app and updates this list.

Not mapped yet: the storage notices (unreadable saved data, a newer version's data, blocked storage, failed save). They are reachable only by corrupting or blocking `localStorage`, which breaks the no-setup-writes rule above. They are covered by `cargo test` in `src/store.rs`. Google Classroom sync is not built.
