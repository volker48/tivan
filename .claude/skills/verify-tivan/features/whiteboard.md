# Whiteboard and oracle

Whiteboard mode drops the editing UI and projects the current class's groups as a wall of colored cards. The oracle button picks a random student who has not been picked yet this round, shows `The oracle has chosen <name>!` in a dialog with how many are left, and starts a new round once everyone has been picked. The pick history is saved.

## Sub-features

- `wb-enter`: the `Whiteboard` button in the Build top bar opens the wall for the selected class.
- `wb-wall`: one card per group, with the group name as a heading and members listed.
- `wb-empty`: an ungrouped class shows `No groups yet.` with `Go back to Build and press Make groups.`
- `wb-leave`: `Back to Build` or Escape with no overlay open returns to Build.
- `oracle-pick`: `Ask the oracle` opens the reveal dialog with one name, and `<n> left` drops by one.
- `oracle-again`: `Pick again` in the dialog picks the next student.
- `oracle-close`: `Done`, a click outside the card, or Escape closes the dialog and stays on the wall.
- `oracle-round`: after the last student, the dialog says `That is everyone, so the next pick starts a new round.`

## How to get to it (user POV)

- From Build, press `Whiteboard` in the top bar. It is disabled only when there are no classes.

## Driving it with agent-browser

Preconditions:

- Baseline seed, `Room 12 Morning` selected. For the wall, first make 4 groups as in [Groups](./groups.md) (`More groups`, then `Make groups`).
- Every command below is prefixed with `source target/verify/<run>/run.env &&`.

- **Enter.** Run `agent-browser --session "$SESSION" find role button click --name "Whiteboard" --exact`. The banner shows `Back to Build`, `Room 12 Morning`, `Ask the oracle 24 left` and the `Timer` group. The page has four level-2 headings equal to the saved `.grouping.names`, and each card lists its members. Run `tivan-capture <run> wb-wall`.
- **Pick.** Run `agent-browser --session "$SESSION" find role button click --name "Ask the oracle"`. Dialog `The oracle has chosen` appears. `agent-browser --session "$SESSION" find role dialog text --name "The oracle has chosen"` prints a name with `!` and `23 left`. The name is in the roster, and saved `.doc.classes[0].oracle.drawn` has one student id, which is that student's. Run `tivan-capture <run> oracle-picked`.
- **Pick again.** Run `agent-browser --session "$SESSION" find role button click --name "Pick again"`. The dialog shows a different roster name and `22 left`, and `.oracle.drawn` has two distinct ids.
- **Close with Escape.** Run `agent-browser --session "$SESSION" press Escape`. The dialog is gone, the wall is still showing, and the button reads `Ask the oracle 22 left`.
- **Leave.** Run `agent-browser --session "$SESSION" press Escape` again. Build mode returns with heading `Tivan`.
- **Survives reload.** Reload, re-enter the whiteboard, and confirm `Ask the oracle 22 left`.
- **Empty wall.** Switch to `Period 3 Science` in Build, then enter the whiteboard. The text `No groups yet.` and `Go back to Build and press Make groups.` show, and the oracle still works (it picks from the roster, not from groups).

## Gotchas

- The pick is random. Assert it is a roster member not already in `.oracle.drawn`, never a specific name.
- `--name "Ask the oracle"` matches the full name `Ask the oracle <n> left`, which changes with every pick.
- Escape closes only the topmost layer: reveal, then Time's up, then the whiteboard itself. One Escape too many leaves the whiteboard.
- Draining a round for `oracle-round` takes 24 picks on `Room 12 Morning`. `Pick again` 23 times after the first pick, or use a new 3-student class from [Classes](./classes.md).
- The wall is laid out for a 16:9 projector. Screenshots at the default viewport may crop large groups. `agent-browser --session "$SESSION" set viewport 1920 1080` gives a projector-like frame.
