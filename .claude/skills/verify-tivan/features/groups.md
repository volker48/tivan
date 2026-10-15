# Groups

A teacher picks how many groups a class needs, has Tivan split the students randomly into that many groups with alliterative names, reshuffles, moves single students between groups, and renames groups by typing or by rolling a new random name. Groups are saved and survive a reload.

## Sub-features

- `groups-count`: the `Number of groups` stepper, bounded 1 to min(students, 40), with a size hint.
- `groups-make`: `Make groups` splits the roster into the chosen number of groups.
- `groups-shuffle`: `Shuffle again` re-deals students and draws new names, keeping the saved group count.
- `groups-move`: tap a student, then a group, to move them. Escape or `Cancel` abandons the move.
- `groups-rename`: click a group name to type a new one.
- `groups-reroll`: the dice button picks a new random name for one group.

## How to get to it (user POV)

- Build mode, with a class that has at least one student selected. The stepper and `Make groups` sit below the `Students` panel.
- A class with no groups yet shows `No groups yet` with the hint `Choose how many groups you want, then press Make groups.`

## Driving it with agent-browser

Preconditions:

- Baseline seed, `Room 12 Morning` selected with 24 students and no groups.
- Every command below is prefixed with `source target/verify/<run>/run.env &&`.

- **Before.** Capture the ungrouped class. Run `tivan-capture <run> groups-before`. The `aria.txt` has heading `No groups yet` and the stepper reads `3 groups` with `8 each`. In `state.json`, `.doc.classes[0].grouping` is `null`.
- **Choose a count.** Press `More groups` once. Run `agent-browser --session "$SESSION" find role button click --name "More groups"`. A full `agent-browser --session "$SESSION" snapshot` shows the stepper reading `4` `groups` and the hint `6 each`.
- **Make groups.** Run `agent-browser --session "$SESSION" find role button click --name "Make groups"`. Four `article` cards appear, each with a group-name button and `Pick a new name for <name>`. `Shuffle again` appears. Run `tivan-capture <run> groups-made`.
- **Saved structure.** Run `jq '.doc.classes[0].grouping | {groups: (.names|length), placed: (.of|length), used: ([.of[]]|unique)}' "$EVIDENCE/groups-made.state.json"`. It gives `groups: 4`, `placed: 24`, `used: [0,1,2,3]`. The four `names` equal the four card names in `groups-made.aria.txt`, in order.
- **Survives reload.** Run `agent-browser --session "$SESSION" reload`, then `agent-browser --session "$SESSION" wait --text "Shuffle again"`, then `tivan-capture <run> groups-after-reload`. The card names and members in the `aria.txt` match `groups-made`, and `state.json` is identical: `diff "$EVIDENCE"/groups-made.state.json "$EVIDENCE"/groups-after-reload.state.json` prints nothing.
- **Shuffle.** Run `agent-browser --session "$SESSION" find role button click --name "Shuffle again"`, then `tivan-capture <run> groups-shuffled`. There are still 4 cards and 4 `.grouping.names`, `.of` still places all 24 students in groups 0-3, and both the membership and the names differ from `groups-made`, on screen and in `state.json`. An identical deal is possible but vanishingly rare, so retry once before calling it a failure.
- **Move a student.** Pick a member of the first card from the snapshot or from `state.json` (a student id whose `.grouping.of` value is `0`), for example `Ava`. Run `agent-browser --session "$SESSION" find role button click --name "Ava" --exact`. A status reads `Moving Ava. Tap a group to place them.` and the other three cards show `Move Ava here`. Run `agent-browser --session "$SESSION" find role button click --name "Move Ava here"`. That clicks the first such card. The status disappears, Ava is listed in that card, and `.grouping.of` maps Ava's student id to that card's index.
- **Cancel a move.** Click a member again, then run `agent-browser --session "$SESSION" press Escape`. The `Moving …` status disappears and nothing moved.
- **Rename by typing.** Click the first card's name button with `find role button click --name "<group name>" --exact`. A textbox `Group name` appears, focused, holding the old name. Run `agent-browser --session "$SESSION" find label "Group name" fill "Table One"`, `agent-browser --session "$SESSION" press Enter` and `agent-browser --session "$SESSION" wait --text "Table One"` before anything else. The card's name button reads `Table One` and `.grouping.names[0]` is `"Table One"`.
- **Reroll a name.** Run `agent-browser --session "$SESSION" find role button click --name "Pick a new name for Table One"`. The card gets a different alliterative name that no other card uses, and `.grouping.names[0]` matches it.
- **Proof.** Finish with a reload and `tivan-capture <run> groups-final`. The screen and saved state show the move and the names from the steps above.

## Gotchas

- Group names and membership are random. Read them from the snapshot or `state.json` for each run. Never hard-code them.
- `--name "Ava"` without `--exact` also matches `Remove Ava` in the open roster panel.
- While a student is selected, every card is a drop target, including its name button. Clicking a name then moves the student instead of renaming the group.
- The stepper's count is per screen visit. It resets to the saved group count (or 3) when you switch classes.
- `Make groups` uses the stepper's count and `Shuffle again` keeps the saved count. Both re-deal everyone and draw all-new names, so renamed groups lose their names.
- `press Enter` in the `Group name` box hung the page in 4 of 6 tries while this map was written. The Chrome renderer pinned at 100% CPU and every later agent-browser call blocked until agent-browser relaunched Chrome with an empty profile. One of the hangs followed `wait --text "<new name>"`, so waiting does not avoid it. The same Enter in the class `Rename` box and in `Add a student` did not hang. Until the cause is fixed, run this step last in its run, expect `groups-rename` to fail, and report it with the run's evidence path. Do not drop the step.
- An empty or unchanged name in the `Group name` box cancels the rename silently. A name over 40 characters shows the alert `Keep names to 40 characters or fewer.`
- `Room 12 Morning` has 24 students, so the stepper allows up to 24 groups. A class with fewer students caps lower.
