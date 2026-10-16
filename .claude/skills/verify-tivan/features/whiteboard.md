# Whiteboard and oracle

Whiteboard mode drops the editing UI and projects the current class's groups as a wall of colored cards. The oracle button picks a random student who has not been picked yet this round, shows `The oracle has chosen <name>!` in a dialog with how many are left, and starts a new round once everyone has been picked. A smaller `Pick a group` button beside it, shown only when the class has groups, does the same for the groups: the dialog names the group and lists its members. Students and groups each keep their own round, and both are saved. Making groups again starts a new group round.

## Sub-features

- `wb-enter`: the `Whiteboard` button in the Build top bar opens the wall for the selected class.
- `wb-wall`: one card per group, with the group name as a heading and members listed.
- `wb-empty`: an ungrouped class shows `No groups yet.` with `Go back to Build and press Make groups.`
- `wb-leave`: `Back to Build` or Escape with no overlay open returns to Build.
- `oracle-pick`: `Ask the oracle` opens the reveal dialog with one name, and `<n> left` drops by one.
- `oracle-again`: `Pick again` in the dialog picks the next student.
- `oracle-close`: `Done`, a click outside the card, or Escape closes the dialog and stays on the wall.
- `oracle-round`: after the last student, the dialog says `That is everyone, so the next pick starts a new round.`
- `oracle-group-pick`: `Pick a group` opens the reveal dialog with one group name in bold and its members, and `<n> left` on the button drops by one.
- `oracle-group-again`: `Pick again` in a group dialog picks the next group, not a student.
- `oracle-group-round`: after the last group, the dialog says `That is every group, so the next pick starts a new round.`
- `oracle-group-reset`: `Make groups` starts a new group round. `Shuffle again` keeps it.
- `oracle-group-absent`: a class with no groups has no `Pick a group` button.
- `wb-phone`: below 45rem the wall is one column of cards that grow to fit their members, and the wall scrolls.

## How to get to it (user POV)

- From Build, press `Whiteboard` in the top bar. It is disabled only when there are no classes.
- On the wall, `Ask the oracle` picks a student. `Pick a group` sits right after it and appears only for a class with groups.

## Driving it with agent-browser

Preconditions:

- Baseline seed, `Room 12 Morning` selected. For the wall, first make 4 groups as in [Groups](./groups.md) (`More groups`, then `Make groups`).
- Every command below is prefixed with `source target/verify/<run>/run.env &&`.

- **Enter.** Run `agent-browser --session "$SESSION" find role button click --name "Whiteboard" --exact`. The banner shows `Back to Build`, `Room 12 Morning`, `Ask the oracle 24 left`, `Pick a group 4 left` and the `Timer` group. The page has four level-2 headings equal to the saved `.grouping.names`, and each card lists its members. Run `tivan-capture <run> wb-wall`.
- **Pick.** Run `agent-browser --session "$SESSION" find role button click --name "Ask the oracle"`. Dialog `The oracle has chosen` appears. `agent-browser --session "$SESSION" find role dialog text --name "The oracle has chosen"` prints a name with `!` and `23 left`. The name is in the roster, and saved `.doc.classes[0].oracle.drawn` has one student id, which is that student's. Run `tivan-capture <run> oracle-picked`.
- **Pick again.** Run `agent-browser --session "$SESSION" find role button click --name "Pick again"`. The dialog shows a different roster name and `22 left`, and `.oracle.drawn` has two distinct ids.
- **Close with Escape.** Run `agent-browser --session "$SESSION" press Escape`. The dialog is gone, the wall is still showing, and the button reads `Ask the oracle 22 left`.
- **Pick a group.** Run `agent-browser --session "$SESSION" find role button click --name "Pick a group"`. Dialog `The oracle has chosen` appears, and `agent-browser --session "$SESSION" find role dialog text --name "The oracle has chosen"` prints one of the four group names with `!`, the names of that group's members, and `3 groups left`. The members are exactly the students whose saved `.doc.classes[0].grouping.of` value is that group's index, and saved `.doc.classes[0].grouping.oracle.drawn` is `[<that index>]`. The student round is untouched: `.doc.classes[0].oracle.drawn` still has its two ids. Run `tivan-capture <run> oracle-group-picked`.
- **Pick a group again.** Run `agent-browser --session "$SESSION" find role button click --name "Pick again"`. The dialog shows a different group and `2 groups left`, and `.grouping.oracle.drawn` has two distinct indices. No student was picked, so `.oracle.drawn` still has two ids.
- **Group round.** Press `Pick again` until the dialog says `0 groups left. That is every group, so the next pick starts a new round.` and `.grouping.oracle.drawn` holds all four indices. Press `Pick again` once more. The dialog says `3 groups left` and `.grouping.oracle.drawn` has a single index that differs from the previous last one. Run `agent-browser --session "$SESSION" press Escape`. The wall shows `Pick a group 3 left` and `Ask the oracle 22 left`.
- **Leave.** Run `agent-browser --session "$SESSION" press Escape` again. Build mode returns with heading `Tivan`.
- **Survives reload.** Reload, re-enter the whiteboard, and confirm `Ask the oracle 22 left` and `Pick a group 3 left`.
- **Make groups starts a new group round.** Press `Back to Build`, then `Make groups`, then `agent-browser --session "$SESSION" wait 1000`, then `tivan-capture <run> oracle-group-reset`. `jq '.doc.classes[0].grouping | has("oracle")' "$EVIDENCE/oracle-group-reset.state.json"` prints `false`, because an empty round is not saved. Re-enter the whiteboard. The button reads `Pick a group 4 left`, and `Ask the oracle` still reads `22 left`.
- **Shuffle again keeps the group round.** Press `Pick a group`, then Escape, so the button reads `Pick a group 3 left`. Press `Back to Build`, then `Shuffle again`, then `agent-browser --session "$SESSION" wait 1500`, then re-enter the whiteboard. The button still reads `Pick a group 3 left` and `.grouping.oracle.drawn` is unchanged.
- **Empty wall.** Switch to `Period 3 Science` in Build, then enter the whiteboard. The text `No groups yet.` and `Go back to Build and press Make groups.` show, and the oracle still works (it picks from the roster, not from groups) and there is no `Pick a group` button.
- **Phone wall.** Run `agent-browser --session "$SESSION" set viewport 402 874` (iPhone 16 Pro). In Build, on `Room 12 Morning` with 3 groups, press `Make groups` and then `Use centers`, then enter the whiteboard. Every member fits inside its card and frame: `agent-browser --session "$SESSION" eval "[...document.querySelectorAll('.wall-center')].every(f=>{const c=f.querySelector('.wall-card');return c.scrollHeight<=c.clientHeight&&f.querySelector('li:last-child').getBoundingClientRect().bottom<=f.getBoundingClientRect().bottom})"` prints `true`, and `.wall`'s `scrollHeight` exceeds its `clientHeight`, so the teacher scrolls to the third group. Run `tivan-capture <run> wb-phone`.

## Gotchas

- The pick is random. Assert it is a roster member not already in `.oracle.drawn`, never a specific name.
- `--name "Ask the oracle"` matches the full name `Ask the oracle <n> left`, which changes with every pick. The group button's name is `Pick a group <n> left`, which does not contain `Ask the oracle`, and `Pick again` does not match `Pick a group`.
- `.grouping.oracle` is absent from the saved state until the first group pick, and again after `Make groups`. Read it as `.grouping.oracle.drawn // []`.
- The group pick is random. Assert it is one of `.grouping.names` not already in `.grouping.oracle.drawn`, never a specific group. A group with no members, possible after students are removed without a shuffle, can be picked and then lists no names.
- Each pick remounts the dialog and replays its half-second pop-in, on the student and group path alike. A `Pick again` click sent before it settles can miss, so `agent-browser --session "$SESSION" wait 800` after each pick, and read the saved `drawn` length to see whether a click landed.
- The group count line is singular at one, `1 group left`, so `wait --text "groups left"` never matches the third pick of four. Wait on `1 group left`.
- `Shuffle again` is already on screen when you press `Make groups` or `Shuffle again` on a grouped class, so `wait --text "Shuffle again"` returns at once. Use a fixed `wait` before you capture or leave Build. A class with no groups has no `Shuffle again` at all.
- Escape closes only the topmost layer: reveal, then Time's up, then the whiteboard itself. One Escape too many leaves the whiteboard.
- Draining a round for `oracle-round` takes 24 picks on `Room 12 Morning`. `Pick again` 23 times after the first pick, or use `Period 3 Science`, which has 20 students, or a new 3-student class from [Classes](./classes.md).
- With centers on, the bar also shows `Round <r> of <n>` and `Rotate`, and each card sits in a frame under a plain-text center name that is not a heading, so the four level-2 headings are still the group names. See [Centers](./centers.md).
- The wall is laid out for a 16:9 projector. Screenshots at the default viewport may crop large groups. `agent-browser --session "$SESSION" set viewport 1920 1080` gives a projector-like frame.
