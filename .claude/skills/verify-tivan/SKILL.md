---
name: verify-tivan
description: Launch the Tivan classroom-grouping web app (Rust + Dioxus 0.7, runs in the browser, saves to localStorage, or to the teacher's account once signed in with Google) on an isolated port and drive it headlessly with agent-browser to prove a change works the way a teacher would use it. Use whenever a change touches what the browser shows or saves (Build mode classes, roster, groups; Whiteboard wall, oracle, timer; Google sign-in and Classroom sync) and you need evidence beyond `cargo test`.
---

# Verify Tivan

Tivan has one surface: a single-page web app. Signed out there is no server-side state. Everything a teacher does is saved by the page into `localStorage["tivan.state"]` of whichever browser profile opened it, and a profile with nothing saved starts from three seed classes. So a fresh browser profile is a fresh data dir, and the stored JSON is the side effect to check. Signing in with Google moves saving to the Worker's database, and `tivan-up` serves only the static bundle, so it covers the signed-out app. The sign-in and Classroom sync recipe, [`features/account.md`](features/account.md), starts the Worker and a fake Google instead.

The harness is the `agent-browser` CLI (headless Chrome over CDP, accessibility-tree selectors). It works the same on a laptop and in a cloud agent. In the Claude desktop app the built-in browser pane can open the same URL instead, but its profile persists between sessions, so it does not start from seed data. Prefer `agent-browser`.

All helpers live in `.claude/skills/verify-tivan/bin/`. Run them from the repo root.

## Prerequisites

On a fresh x86_64 Linux machine with Node 24 or newer, `scripts/bootstrap.sh` installs the first three. On Linux ARM64 it skips `agent-browser`, since Chrome for Testing has no build there.

- Rust with the `wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown`).
- `dx` 0.7.10, matching the `dioxus = "=0.7.10"` pin: `cargo install dioxus-cli --version 0.7.10 --locked`. `tivan-up` refuses any other version.
- `agent-browser` (proven with 0.39.1) and its Chrome for Testing (proven with 155.0.8059.79): `npm i -g agent-browser && agent-browser install`. On Linux add `--with-deps`. Do not use Chrome for Testing 151, which hangs the page after Enter (see Doctor).
- `curl`, `lsof`, `jq`, `perl` (for timeouts).

Read `agent-browser skills get core` once if you have never used it. The commands below are all you need for Tivan.

## Launch

```bash
.claude/skills/verify-tivan/bin/tivan-up
```

It:

1. Refuses to start if another `dx` process already runs in this checkout. Two `dx` servers share `target/dx/tivan` build output and can serve each other's builds. If the user's own `dx serve` (port 8080, from `.claude/launch.json`) is running, ask them before stopping it.
2. Picks the first free port in 8090-8099. It never uses 8080.
3. Runs `dx serve --platform web --addr 127.0.0.1 --port <port> --open false --interactive false --watch false --hot-reload false`. Rebuilds are off, so the served build is pinned to the source as it was at launch. After editing source, run `tivan-down` and then `tivan-up` again.
4. Waits for `Build completed successfully` in `dx.log`. dx prints `Serving your app` and answers HTTP 200 *before* the build exists, so neither means ready. A warm build takes about 5 s. A cold `target/` takes minutes; the wait is capped at 900 s (`TIVAN_BUILD_TIMEOUT_S` overrides).
5. Opens a fresh agent-browser session `tivan-verify-<run>` on the URL and waits until the wasm app has rendered its `Tivan` heading. That is the ready point: the page is usable and the seed data is loaded and saved.

It ends by printing the run's settings, which are also saved in `target/verify/<run>/run.env`:

```
ready: run 20261015-153421-56145
RUN=20261015-153421-56145
PORT=8090
URL=http://127.0.0.1:8090/
SESSION=tivan-verify-20261015-153421-56145
EVIDENCE=/…/tivan/target/verify/20261015-153421-56145
STARTED=…
GIT_REV=5f58097
```

Shell variables do not survive between tool calls, and zsh does not word-split a `$A`-style command variable. Start every drive command by sourcing the run file:

```bash
source target/verify/<run>/run.env && agent-browser --session "$SESSION" snapshot -i -c
```

## Doctor

```bash
.claude/skills/verify-tivan/bin/tivan-doctor <run>
```

Read-only. It starts no browser and kills nothing. It checks that:

- this run's `dx serve` pid is alive and owns the port
- the build completed with no `ERROR`/`Build failed` in `dx.log`
- the URL serves the Tivan page
- no file under `src/`, `assets/`, `Cargo.*` or `Dioxus.toml` changed after launch
- the browser session is open, and its page answers within 10 s on this run's URL

It exits 1 if any check fails and says what to do next. Run it first whenever anything looks off.

Do not probe a session with any other `agent-browser --session X …` command. Every such command *creates* session X if it does not exist. `agent-browser session list` is the safe check.

If the page gets stuck in a loop, every `agent-browser` call to that session blocks. agent-browser may then relaunch Chrome on `about:blank` with an empty profile, and the app's saved state is gone. Symptoms:

- a command that never returns
- `localStorage` `SecurityError`s from `eval`
- `No element found` for something that was just on screen
- a Chrome `--type=renderer` process at 100% CPU

The helpers wrap every agent-browser call in a timeout. Doctor and `tivan-capture` fail when the page does not answer or is not on this run's URL, and `tivan-down` kills the session's own daemon (pid in `~/.agent-browser/<session>.pid`) when `close` cannot get through. In your own commands, `wait` for the expected result after each action before you `snapshot` or `eval`. To bound your own calls the same way, define `ab() { perl -e 'alarm shift; exec @ARGV' 40 agent-browser --session "$SESSION" "$@"; }` in the command and use `ab …`. Any journey that crossed a hang or relaunch is invalid as proof: keep its evidence, run `tivan-down`, and repeat the journey in a new run. If it hangs again at the same step, report it as a failure.

The one known trigger was a browser bug, not the app. With Chrome for Testing 151, any `press` the page did not consume started a storm of thousands of synthetic `Unidentified` key events per second. The usual case was Enter in a rename box, which saves and removes the field, so the Enter reached the page body. This was seen on macOS arm64 with agent-browser 0.33.1 and 0.39.1. Chrome for Testing 155.0.8059.79 does not do it. If a run hangs right after a `press`, check the version in `agent-browser eval navigator.userAgent` before suspecting the app.

## Drive

Use accessibility roles and names, which this app sets deliberately. Avoid coordinates, CSS classes and `@eN` refs from an old snapshot. Refs go stale after every re-render.

```bash
source target/verify/<run>/run.env
agent-browser --session "$SESSION" snapshot -i -c                                   # interactive elements only
agent-browser --session "$SESSION" snapshot                                         # everything, including plain text
agent-browser --session "$SESSION" find role button click --name "Make groups"
agent-browser --session "$SESSION" find label "Class name" fill "Period 5 Art"
agent-browser --session "$SESSION" find role dialog text --name "The oracle has chosen"
agent-browser --session "$SESSION" press Escape
agent-browser --session "$SESSION" wait --text "Time's up!" --timeout 75000
agent-browser --session "$SESSION" reload
agent-browser --session "$SESSION" eval "JSON.parse(localStorage.getItem('tivan.state'))" | jq '.doc.classes[0].grouping'
```

`find role button … --name` matches substrings. `"Abigail"` also matches `"Remove Abigail"`, and `"Ask the oracle"` matches `"Ask the oracle 18 left"`. Add `--exact` when a shorter name is a prefix or substring of another visible button.

The handles by screen:

- **Build mode** (start screen). It has a heading `Tivan`, a button `Whiteboard`, and a navigation `Classes` with one button per class plus `+ New class`. Class pills report `pressed` for the current class. The class header has a level-2 heading with the class name, then `Rename` and `Delete`. `Delete` asks `Delete <name> for good?` with `Yes, delete` / `Keep it`. The roster panel has the toggle button `Students <n>`, the textbox `Add a student`, `Add`, one `Remove <name>` per student, and `Paste a list`. Pasting uses the textarea with placeholder `One name per line…`, then `Add these names` or `Cancel`. The group stepper is the group `Number of groups` with `Fewer groups` / `More groups` and a size hint like `6 each`. Then come `Make groups` and, once grouped, `Shuffle again` and `Use centers`, which reads `Remove centers` once centers are on (it asks `Remove the centers and their names?` with `Yes, remove` / `Keep them`) and is then followed by the text `Round <r> of <n>` and a button `Rotate` (disabled with one group). Each group card is an `article` with a button named for the group (click to rename via textbox `Group name`), a button `Pick a new name for <group>`, a toggle `Lock <group>` (pressed when locked), and one button per member. Clicking a member shows a status `Moving <name>. Tap a group to place them.` with `Cancel`, and every other card shows `Move <name> here`. With centers on, each card sits in a frame headed by a button named for its center (`Center 1` and so on, click to rename via textbox `Center name`), and the cards read in center order.
- **New class form**: heading `New class`, textbox label `Class name`, textarea label `Students (optional, you can add them later)`, `Create class`, and `Cancel` (only when other classes exist).
- **Notices**: role `status` with `Dismiss` for dismissible ones such as skipped roster lines. Role `alert` for storage problems the teacher cannot dismiss.
- **Whiteboard mode**: `Back to Build`, the class name, `Ask the oracle <n> left`, for a grouped class `Pick a group <n> left` right after it (absent when there are no groups), with centers on the text `Round <r> of <n>` and `Rotate`, and the group `Timer` with `Start a {1,2,3,5,10} minute timer`. While a timer runs it shows `Pause`/`Resume` and `Cancel`. The countdown is role `timer` showing `m:ss`, and the wall has one level-2 heading per group (center names are plain text above the cards, not headings). The oracle opens dialog `The oracle has chosen` with the picked name in bold, `<n> left`, `Pick again` and `Done`. `Pick a group` opens the same dialog with the group's name in bold, its members listed, and `<n> groups left`, and `Pick again` repeats whichever kind opened it. A finished timer opens alertdialog `Time's up` with `Dismiss`. Entering the whiteboard goes fullscreen, and the first Escape there only leaves fullscreen. After that, Escape closes the top layer first: reveal, then Time's up, then back to Build.
- **Account bar in Build**: a link `Sign in with Google` for a visitor, or the teacher's first name, `Sync Classroom` and `Sign out`. `Sync Classroom` opens a panel headed `Sync Classroom` with one checkbox per new course, `Apply` and `Cancel`, then a `status` with the result and `Done`. See `features/account.md` for the exact flow.
- **Timer chip in Build**: while a timer exists, the Build top bar shows a button with the digits and `Timer`/`Paused`, or `Time's up!` + `Dismiss`.

Per-feature recipes, with exact commands and expected results, are in [`features/README.md`](features/README.md). Read the index, then the feature file for what you changed.

## Evidence

```bash
.claude/skills/verify-tivan/bin/tivan-capture <run> <label>
```

This writes three files into `target/verify/<run>/`: `<label>.png` (full-page screenshot), `<label>.aria.txt` (full accessibility tree with text), and `<label>.state.json` (the saved document as JSON, or `null`). Labels are kebab-case, such as `groups-made` or `groups-after-reload`. `dx.log` and `run.env` in the same directory record which build and revision you proved.

Proof standards:

- Drive the real user path: buttons, text fields, keys. Never write `localStorage` or call into the wasm to set up or change state. `eval` is for *reading* state only.
- Capture before and after the action, not only the final screen.
- Every mutation needs its side effect proven. `state.json` must show the change, and after `agent-browser --session "$SESSION" reload` a second capture must show the same thing. That proves the save effect ran, not just in-memory signals.
- Group names, group membership and oracle picks come from `Math.random`. Assert structure (count of cards, members partition the roster, a pick is a roster name, `n left` decrements) and assert that what the page shows equals what was saved. Never assert a specific random name.
- Report each journey with the run id, the commands, the expected result and the artifact paths. If a journey's precondition is missing, report the failure and the reason. Do not change the journey until it passes.

## Cleanup

```bash
.claude/skills/verify-tivan/bin/tivan-down <run>
```

It closes only this run's browser session (re-checking `session list`, up to three tries, and killing that session's own daemon if a stuck page blocks `close`). Then it stops only the `dx serve` pid this run recorded, after confirming that pid is still a `dx serve` on this run's port. It never kills by name and never runs `agent-browser close --all`, which would close sessions it did not start. It deletes `server.pid` and keeps everything else. Evidence stays in `target/verify/<run>/` until someone deletes it. `cargo clean` deletes it too. Run cleanup after failed attempts as well, so servers and browsers don't pile up on 8090-8099.

To list past runs: `ls target/verify/`. To find a run you lost track of, run `grep -l . target/verify/*/server.pid`. Runs whose `server.pid` still exists were never cleaned up.

## Helpers

| Command | Does |
| --- | --- |
| `bin/tivan-up` | Build, serve on 8090-8099, open a fresh session, print the run id |
| `bin/tivan-doctor <run>` | Read-only health check, exit 1 on any failure |
| `bin/tivan-capture <run> <label>` | Screenshot + accessibility tree + saved state into the evidence dir |
| `bin/tivan-down <run>` | Close this run's session, stop this run's server, keep evidence |

`bin/lib.sh` is sourced by the others and is not run on its own. Its `ab` function is `agent-browser --session "$SESSION"` with a 30 s limit (perl `alarm`, since macOS has no `timeout`).
