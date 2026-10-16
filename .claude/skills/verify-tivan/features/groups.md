# Groups

A teacher picks how many groups a class needs, has Tivan split the students randomly into that many groups with alliterative names, reshuffles, moves single students between groups, and renames groups by typing or by rolling a new random name. Groups are saved and survive a reload.

## Sub-features

- `groups-count`: the `Number of groups` stepper, bounded 1 to min(students, 40), with a size hint.
- `groups-make`: `Make groups` splits the roster into the chosen number of groups.
- `groups-shuffle`: `Shuffle again` re-deals students between the existing groups. Every group keeps its name, hand-typed ones included, and the saved group count. Chips fly to their new groups.
- `groups-move`: tap a student, then a group, to move them. Escape or `Cancel` abandons the move.
- `groups-rename`: click a group name to type a new one.
- `groups-reroll`: the dice button picks a new random name for one group.

## How to get to it (user POV)

- Build mode, with a class that has at least one student selected. The stepper and `Make groups` sit below the `Students` panel.
- A class with no groups yet shows `No groups yet` with the hint `Choose how many groups you want, then press Make groups.`

## Driving it with agent-browser

Preconditions:

- Baseline seed, `McCurdy HR` selected with 18 students and no groups.
- Every command below is prefixed with `source target/verify/<run>/run.env &&`.

- **Before.** Capture the ungrouped class. Run `tivan-capture <run> groups-before`. The `aria.txt` has heading `No groups yet` and the stepper reads `3 groups` with `6 each`. In `state.json`, `.doc.classes[0].grouping` is `null`.
- **Choose a count.** Press `More groups` once. Run `agent-browser --session "$SESSION" find role button click --name "More groups"`. A full `agent-browser --session "$SESSION" snapshot` shows the stepper reading `4` `groups` and the hint `4 to 5 each`.
- **Make groups.** Run `agent-browser --session "$SESSION" find role button click --name "Make groups"`. Four `article` cards appear, each with a group-name button and `Pick a new name for <name>`. `Shuffle again` appears. Run `tivan-capture <run> groups-made`.
- **Saved structure.** Run `jq '.doc.classes[0].grouping | {groups: (.names|length), placed: (.of|length), used: ([.of[]]|unique)}' "$EVIDENCE/groups-made.state.json"`. It gives `groups: 4`, `placed: 18`, `used: [0,1,2,3]`. The four `names` equal the four card names in `groups-made.aria.txt`, in order.
- **Survives reload.** Run `agent-browser --session "$SESSION" reload`, then `agent-browser --session "$SESSION" wait --text "Shuffle again"`, then `tivan-capture <run> groups-after-reload`. The card names and members in the `aria.txt` match `groups-made`, and `state.json` is identical: `diff "$EVIDENCE"/groups-made.state.json "$EVIDENCE"/groups-after-reload.state.json` prints nothing.
- **Shuffle.** Run `agent-browser --session "$SESSION" find role button click --name "Shuffle again"`, then `tivan-capture <run> groups-shuffled`. There are still 4 cards and 4 `.grouping.names`, `.of` still places all 18 students in groups 0-3, the names are identical to `groups-made`, and the membership differs, on screen and in `state.json`. An identical deal is possible but vanishingly rare, so retry once before calling it a failure.
- **Move a student.** Pick a member of the first card from the snapshot or from `state.json` (a student id whose `.grouping.of` value is `0`), for example `Abigail`. Run `agent-browser --session "$SESSION" find role button click --name "Abigail" --exact`. A status reads `Moving Abigail. Tap a group to place them.` and the other three cards show `Move Abigail here`. Run `agent-browser --session "$SESSION" find role button click --name "Move Abigail here"`. That clicks the first such card. The status disappears, Abigail is listed in that card, and `.grouping.of` maps Abigail's student id to that card's index.
- **Cancel a move.** Click a member again, then run `agent-browser --session "$SESSION" press Escape`. The `Moving …` status disappears and nothing moved.
- **Rename by typing.** Click the first card's name button with `find role button click --name "<group name>" --exact`. A textbox `Group name` appears, focused, holding the old name. Run `agent-browser --session "$SESSION" find label "Group name" fill "Table One"`, `agent-browser --session "$SESSION" press Enter` and `agent-browser --session "$SESSION" wait --text "Table One"`. The textbox is gone, the card's name button reads `Table One`, and `.grouping.names[0]` is `"Table One"`. Clicking away, for example `find role heading click --name "McCurdy HR"`, also saves.
- **Reroll a name.** Run `agent-browser --session "$SESSION" find role button click --name "Pick a new name for Table One"`. The card gets a different alliterative name that no other card uses, and `.grouping.names[0]` matches it.
- **Proof.** Finish with a reload and `tivan-capture <run> groups-final`. The screen and saved state show the move and the names from the steps above.

## Gotchas

- Group names and membership are random. Read them from the snapshot or `state.json` for each run. Never hard-code them.
- `--name "Abigail"` without `--exact` also matches `Remove Abigail` in the open roster panel.
- While a student is selected, every card is a drop target, including its name button. Clicking a name then moves the student instead of renaming the group.
- The stepper's count is per screen visit. It resets to the saved group count (or 3) when you switch classes.
- `Make groups` uses the stepper's count, re-deals everyone, and draws all-new names, so renamed groups lose their names. `Shuffle again` keeps the saved count and every group name, and only moves students. When centers are on, `Shuffle again` also keeps them and the round, while `Make groups` keeps the center names and restarts at `Round 1`. See [Centers](./centers.md).
- With Chrome for Testing 151, `press Enter` in the `Group name` box hung the page. Enter saves and removes the field, so the Enter reached the page body, and that Chrome answered with a storm of synthetic key events. The same happens on a plain HTML page, so the app is not at fault. Chrome for Testing 155 is clean. On an older Chrome, save by clicking away. See the stuck-page notes in `SKILL.md`.
- An empty or unchanged name in the `Group name` box cancels the rename silently. A name over 40 characters shows the alert `Keep names to 40 characters or fewer.`
- `McCurdy HR` has 18 students, so the stepper allows up to 18 groups. A class with fewer students caps lower.
