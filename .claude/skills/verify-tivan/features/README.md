# Tivan verification map

This directory is the maintained source for verifying what a teacher can do in Tivan. Read this index before driving the app. Then use the matching feature file as the recipe.

## Baseline preconditions

- A run started with `.claude/skills/verify-tivan/bin/tivan-up`. `tivan-doctor <run>` passes.
- The run's session is fresh from `tivan-up`, so the page shows the seed: three classes `McCurdy HR` (18 students, selected), `Period 3 Science` (20), `Reading Club` (27), none grouped, no timer, Build mode.
- To get back to baseline mid-run, start a new run. Never clear or write `localStorage` by hand.
- Every drive command starts with `source target/verify/<run>/run.env &&` so `$SESSION` and `$EVIDENCE` are set.

## Driving conventions

- Drive through `agent-browser --session "$SESSION"` with `find role|label|placeholder … --name`, `press` and `fill`. Take a fresh `snapshot` after each change before using any `@eN` ref.
- `--name` matches substrings. Use `--exact` when one visible name contains another (`Abigail` and `Remove Abigail`).
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
- [Centers](./centers.md): turn learning centers on for a grouped class, rotate the groups through them, rename them.
- [Whiteboard and oracle](./whiteboard.md): the projected group wall and the random student and group pickers.
- [Timer](./timer.md): preset countdowns on the whiteboard, pause, cancel, Time's up, and the Build-mode chip.

## Proof status

Verification on 2026-10-16, app rev `b3307e7`, using dx 0.7.10, agent-browser 0.39.1 and HeadlessChrome 155:

- **Seed**: `McCurdy HR` is selected with the exact 18-name roster in order; the other classes have 20 and 27 students. Screen and saved state agree after reload. Evidence: `target/verify/20261016-090731-2406/seed{,-reloaded}.{png,aria.txt,state.json}`.
- **Groups**: the 3- and 4-group hints, making groups, shuffling, moving a student, Escape-cancel, renaming with Enter, and rerolling passed, with reload proof. Evidence: `groups-*` captures in `target/verify/20261016-090731-2406/`.
- **Whiteboard and oracle**: wall headings and members match saved groups; student and group picks, both complete rounds and refills, regroup resetting the group round, shuffle preserving it, and the empty wall passed. At 402×874, all 18 students fit their three center cards and the wall scrolls. Evidence: `wb-*` and `oracle-*` captures in `target/verify/20261016-090731-2406/`.
- **Centers**: enable, rotate through the full cycle, rename with Enter, shuffle retention, regroup reset, removal confirmation and cancellation, whiteboard rotation, and disabled rotation for one center passed, with reload proof. Evidence: `centers-*` captures in `target/verify/20261016-090731-2406/`.
- **Roster**: add, remove, paste two valid names and skip a 41-character line, dismiss the notice, collapse/expand, and restore the 18-name roster passed. Add/remove/paste survived reload. Evidence: `roster-*` captures in `target/verify/20261016-091813-5122/`.
- **Classes**: switch, create with a roster, rename with Enter, cancel and confirm deletion, and delete all classes to reach the welcome form without `Cancel` passed, with reload proof. Evidence: `classes-*` captures in `target/verify/20261016-091813-5122/`.
- **Timer**: start, pause with unchanged digits across 3 seconds, resume, open via the Build chip, reach Time's up, dismiss with Escape, cancel, and clear on reload passed. Saved class data stayed unchanged. Evidence: `timer-*` captures in `target/verify/20261016-091813-5122/`.

Each run records browser commands in `commands.log`; every capture includes a screenshot, accessibility tree and saved JSON. Doctor passed before and after both runs, and both sessions and servers were cleaned up. The first launch, `20261016-090628-99697`, lost its server when its command runner exited; it was cleaned up and excluded from proof. The roster placeholder selector and the centers recipe's stored-version expectation were corrected to match the observed app.

Earlier verification:

Status as of 2026-10-15, rev `5f58097`:

- **Groups**: run end to end.
  - `groups-count`, `groups-make` and `groups-shuffle` passed, with reload proof for `groups-make`.
  - `groups-move` (including Escape-cancel) passed, proven by screen and saved state. Its reload capture was lost to the rename hang.
  - `groups-reroll` passed with reload proof in a separate run. It passed again on a renamed group in the rename run below.
  - `groups-rename` passed with Enter, with reload proof, on agent-browser 0.39.1 and Chrome for Testing 155.0.8059.79, in `target/verify/20261015-171929-68457/` (rev `5a93ab4`, same `src/` as `5f58097`). Eight more Enter renames, each followed straight away by a snapshot, all passed. Saving by clicking away passed in `target/verify/20261015-163754-65558/`. The earlier failures were a Chrome for Testing 151 bug, not the app. See the Groups gotchas.
  - Evidence: `target/verify/20261015-155559-59094/` (main journey), `target/verify/20261015-155811-59592/` (reroll), `target/verify/20261015-163754-65558/` (rename by clicking away, then reroll) and `target/verify/20261015-171929-68457/` (rename with Enter). `target/verify/20261015-154121-57727/` holds an earlier hang.
- **Centers**: written from the code, after a smoke pass over the handles on one run (rev `8c89a04+dirty`, with the UI changes uncommitted): `Use centers`, `Rotate` in Build and on the whiteboard (cards animated), a center rename with Enter, `Make groups` keeping the renamed center and appending `Center 4`..`Center 12`, `Remove centers`, and the `.centers` saved shape after a reload. The recipe has not been run step by step with captures, so the first agent to run it fixes any step that does not match the app and updates this list.
- **Whiteboard and oracle**: run end to end on agent-browser 0.39.1 and Chrome for Testing 155, run `20261015-233815-30238` (rev `7e918da+dirty`, with the group oracle uncommitted). Every step of `whiteboard.md` passed, including the student oracle steps, the `oracle-group-*` steps with reload proof, the empty wall, and a drained student round on `Period 3 Science`. Evidence: `target/verify/20261015-233815-30238/`.
- **Classes, Roster, Timer**: written from the code and from one exploratory pass over their handles. They have not been run step by step. The first agent to run one fixes any step that does not match the app and updates this list.
  - Exception: `classes-rename` passed with reload proof, on the then-current first seed class rather than the recipe's new class. It passed with Enter in `target/verify/20261015-171929-68457/` and by clicking away in `target/verify/20261015-163949-66174/`.

Not mapped yet: the storage notices (unreadable saved data, a newer version's data, blocked storage, failed save). They are reachable only by corrupting or blocking `localStorage`, which breaks the no-setup-writes rule above. They are covered by `cargo test` in `src/store.rs`. Google Classroom sync is not built.
