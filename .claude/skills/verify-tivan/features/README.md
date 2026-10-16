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
- `tivan-up` serves only the static build, so a page there is always signed out. The account feature needs the local Worker and has its own preconditions.

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
- [Account and Classroom sync](./account.md): sign in with Google, keep classes on the server, and pull classes and rosters from Google Classroom.

## Proof status

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
  - Exception: `classes-rename` passed with reload proof, on the seed's `Room 12 Morning` rather than the recipe's new class. It passed with Enter in `target/verify/20261015-171929-68457/` and by clicking away in `target/verify/20261015-163949-66174/`.

- **Account and Classroom sync**: written from the code, on 2026-10-16. Every step except the cookie steps and the real Worker was driven on agent-browser 0.39.1 against a throwaway in-memory stand-in for the Worker, run `20261016-083519-80719` (rev `b045e44` plus the uncommitted UI). The stand-in lives outside the repo and answered `/api/*` and `/auth/*` while proxying the rest to the `tivan-up` server. It covered signing in (and the browser's saved doc staying byte for byte the same), the picker, Apply, resync as a no-op, one `PUT` for a burst of edits, a fresh session getting the doc back, Sign out sending a waiting change first, a refused session falling back to the browser, a `403 reauthorize` leaving for `/auth/login`, `?auth_error=`, an unreachable account service, an unreadable and a newer server doc (no `PUT` after either), a refused save, and the 375 px layout. Not run: the real Worker, `cookies get` and `cookies clear`, real Google consent, and the `Secure` cookie flag. The first agent to run it against the Worker fixes any step that does not match and updates this list.

Not mapped yet: the storage notices (unreadable saved data, a newer version's data, blocked storage, failed save). They are reachable only by corrupting or blocking `localStorage`, which breaks the no-setup-writes rule above. They are covered by `cargo test` in `src/store.rs`.
