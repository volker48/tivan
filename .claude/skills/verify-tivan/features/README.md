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
- [Groups](./groups.md): choose a group count, make and reshuffle groups, lock groups against shuffling, move students, rename groups.
- [Attendance](./attendance.md): mark a student absent for the day so groups and the oracle skip them.
- [Centers](./centers.md): turn learning centers on for a grouped class, rotate the groups through them, rename them.
- [Whiteboard and oracle](./whiteboard.md): the projected group wall and the random student and group pickers.
- [Timer](./timer.md): preset countdowns on the whiteboard, pause, cancel, Time's up, and the Build-mode chip.
- [Account and Classroom sync](./account.md): sign in with Google, keep classes on the server, and pull classes and rosters from Google Classroom.
- [Settings](./settings.md): the gear page, and choosing a seasonal avatar skin.
- [Avatars](./avatars.md): the 30 animal stickers, their blinks and smiles, and how they celebrate on the wall and in the oracle.

## Proof status

Verification on 2026-10-16, app rev `b3307e7`, using dx 0.7.10, agent-browser 0.39.1 and HeadlessChrome 155:

- **Seed**: `McCurdy HR` is selected with the exact 18-name roster in order; the other classes have 20 and 27 students. Screen and saved state agree after reload. Evidence: `target/verify/20261016-090731-2406/seed{,-reloaded}.{png,aria.txt,state.json}`.
- **Groups**: the 3- and 4-group hints, making groups, shuffling, moving a student, Escape-cancel, renaming with Enter, and rerolling passed, with reload proof. Evidence: `groups-*` captures in `target/verify/20261016-090731-2406/`.
- **Whiteboard and oracle**: wall headings and members match saved groups; student and group picks, both complete rounds and refills, regroup resetting the group round, shuffle preserving it, and the empty wall passed. At 402×874, all 18 students fit their three center cards and the wall scrolls. Evidence: `wb-*` and `oracle-*` captures in `target/verify/20261016-090731-2406/`.
- **Centers**: enable, rotate through the full cycle, rename with Enter, shuffle retention, regroup reset, removal confirmation and cancellation, whiteboard rotation, and disabled rotation for one center passed, with reload proof. Evidence: `centers-*` captures in `target/verify/20261016-090731-2406/`.
- **Roster**: add, remove, paste two valid names and skip a 41-character line, dismiss the notice, collapse/expand, and restore the 18-name roster passed. Add/remove/paste survived reload. Evidence: `roster-*` captures in `target/verify/20261016-091813-5122/`.
- **Classes**: switch, create with a roster, rename with Enter, cancel and confirm deletion, and delete all classes to reach the welcome form without `Cancel` passed, with reload proof. Evidence: `classes-*` captures in `target/verify/20261016-091813-5122/`.
- **Timer**: start, pause with unchanged digits across 3 seconds, resume, open via the Build chip, reach Time's up, dismiss with Escape, cancel, and clear on reload passed. Saved class data stayed unchanged. Evidence: `timer-*` captures in `target/verify/20261016-091813-5122/`.

- **Settings**: the gear opens Settings, the four skin radios save to `doc.settings.skin` (`auto`, `plain`, `{"fixed":"christmas"}`), the whiteboard shows `#skin-halloween` or `#skin-christmas` on all 18 avatars or none, the choice survives reload, and Escape returns to Build. Run `20261016-100817-17074` (rev `ed53995+dirty`, before commit); evidence in `target/verify/20261016-100817-17074/` and `target/verify/20261016-100508-15439/` (phone width 375, no horizontal scroll).
- **Halloween costumes** (2026-10-22): with `Halloween` fixed, the Settings previews show `#costume-witch-hat`, `#costume-pumpkin-hat` and `#costume-bat` on bear, panda and cat, and the whiteboard dresses its 18 avatars in all four Halloween costumes, the frog in the spider and the bunny in the bat. The choice survived reload and the stored version stayed 6. Run `20261022-090103-50451` (rev `1c9a81a+dirty`); evidence `settings-halloween` and `wall-halloween` in `target/verify/20261022-090103-50451/`. Since then the sprite symbols are `#costume-<name>`, not `#skin-<name>`.
- **Nine Halloween costumes** (2026-10-22): with `Halloween` fixed, the Settings previews show witch hat, pumpkin hat and ghost, and the whiteboard's 18 avatars wear all nine Halloween costumes, two each (frog: bat; lion: werewolf; sheep: spider). The choice survived reload and the stored version stayed 6. Run `20261022-111052-55240` (rev `b0798e1+dirty`); evidence `settings-halloween` and `wall-halloween` in `target/verify/20261022-111052-55240/`.

- **Avatars** (2026-10-24): every step of `avatars.md` passed at 1280x720 before commit: a fresh profile saves version 7 and `Reading Club`'s 27 students have 27 different animals, hovering a roster row shows happy eyes, flying chips grin during `Shuffle again` and stop on landing, the wall's avatars arrive in a wave and blink (16 blinking frames sampled in 3 s on the wall, 0 in Build), breathe (`av-breathe` running on every wall avatar) and fidget one at a time (`fidget-tilt-right`, `fidget-lean-left` and `fidget-twirl` seen in 8 s, never two at once in 15 s, none under the oracle's card), the oracle's chosen avatar leaps in with sparks and settles with open eyes and every spark faded, a picked group's members pop up in turn, and reduced motion stops the blinking, fidgets and breathing and shows the reveal settled. The saved version stayed 7 after reload. Run `20261024-184611-1716` (rev `f6c7707+dirty`) covered every step at 1280x720, including the idle ones. Run `20261024-185230-5299` (rev `f3f2ff3+dirty`, the current wall markup) checked one fidget at a time, Christmas and the phone layout. With `Christmas` fixed the wall lists `#costume-santa-hat` and `#costume-holly`, and the shark wears holly. At 375 px the wall and the reveal do not scroll sideways. Evidence `avatars-*` in `target/verify/20261024-184611-1716/`, and `settings-christmas`, `wall-christmas` and `phone-*` in `target/verify/20261024-185230-5299/`.

- **Locked groups** (2026-10-24): a scripted run asserted every step against the saved state and the cards on screen (`e2e-proof.log`, 115 checks).
  - **Lock and shuffle.** On `McCurdy HR` with 3 groups, locking the second card saved `.grouping.locked` as `[1]` and pressed its padlock. Ten `Shuffle again` clicks kept its six members while the other twelve re-dealt 6 and 6. Every reload kept the saved class identical.
  - **Absences.** A locked member marked absent stayed in the locked group through three shuffles, and an absent unlocked member stayed in an unlocked group. A student added afterwards joined an unlocked group.
  - **Make groups.** `Make groups` asked first, and `Keep my groups` changed nothing. Locking all three disabled `Shuffle again` with its tooltip. `Yes, make groups` saved no `locked` key and released every padlock.
  - **Review cases.** In new classes, 8 students in 4 groups with groups 0 and 1 locked and 6 absent kept all 4 groups and came back as 2 each with the locked groups unchanged. 4 students in 2 groups with the first locked and 3 absent kept both groups unchanged.
  - **Final reload** left the whole saved document identical at version 9.
  - **Run.** `20261024-215201-30944` (rev `fe85ba1`); evidence `e2e-*` in `target/verify/20261024-215201-30944/`. The log keeps nine harness failures, each explained by a `NOTE` and rerun.


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

- **Account and Classroom sync**: written from the code, on 2026-10-16. Every step except the cookie steps and the real Worker was driven on agent-browser 0.39.1 against a throwaway in-memory stand-in for the Worker, run `20261016-083519-80719` (rev `b045e44` plus the uncommitted UI). The stand-in lives outside the repo and answered `/api/*` and `/auth/*` while proxying the rest to the `tivan-up` server. It covered signing in (and the browser's saved doc staying byte for byte the same), the picker, Apply, resync as a no-op, one `PUT` for a burst of edits, a fresh session getting the doc back, Sign out sending a waiting change first, a refused session falling back to the browser, a `403 reauthorize` leaving for `/auth/login`, `?auth_error=`, an unreachable account service, an unreadable and a newer server doc (no `PUT` after either), a refused save, and the 375 px layout. Not run: the real Worker, `cookies get` and `cookies clear`, real Google consent, and the `Secure` cookie flag. The first agent to run it against the Worker fixes any step that does not match and updates this list.

Not mapped yet: the storage notices (unreadable saved data, a newer version's data, blocked storage, failed save). They are reachable only by corrupting or blocking `localStorage`, which breaks the no-setup-writes rule above. They are covered by `cargo test` in `src/store.rs`.
