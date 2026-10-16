# Centers

A teacher who runs learning centers turns them on for a grouped class. Each group then stands in its own square frame, one per classroom station, and `Rotate` sends every group to the next station in a fixed cycle. After one fewer press than there are groups, every group has visited every center. The place in the cycle is saved, so a teacher can stop after round 3 of 5 and press `Rotate` again another day. The group card flies to its new frame, in Build and on the whiteboard. Centers are optional per class.

## Sub-features

- `centers-on`: `Use centers` gives each group a center named `Center 1`, `Center 2`, and so on, at `Round 1 of <n>`.
- `centers-rotate`: `Rotate` moves every group one center on and raises the round by one.
- `centers-cycle`: `n` presses of `Rotate` bring every group home, and the round returns to 1.
- `centers-rename`: click a center's name to type a new one. Center names stay put while groups move past.
- `centers-keep`: `Shuffle again` keeps the centers, their names and the round. `Make groups` keeps the center names and restarts at round 1.
- `centers-off`: `Remove centers` asks first, then drops the centers and their names, and the groups stay.
- `centers-wall`: the whiteboard draws the same frames, with the center name above each card, and has its own `Rotate` and round label.
- `centers-single`: with one group, `Rotate` is disabled.

## How to get to it (user POV)

- Build mode, a class with groups made. `Use centers` or `Remove centers` sits after `Shuffle again` in the group controls. While centers are on, `Round <r> of <n>` and `Rotate` follow it.
- Whiteboard mode, for a class that uses centers. The top bar shows `Round <r> of <n>` and `Rotate` between the oracle and the timer.
- A class with no groups has no centers controls at all.

## Driving it with agent-browser

Preconditions:

- Baseline seed, `Room 12 Morning` selected with 24 students. Make 3 groups first with `agent-browser --session "$SESSION" find role button click --name "Make groups"`, then `agent-browser --session "$SESSION" wait --text "Shuffle again"`.
- Every command below is prefixed with `source target/verify/<run>/run.env &&`.
- Group names and membership are random. Read them from the snapshot or `state.json`.

- **Before.** Run `tivan-capture <run> centers-before`. `jq '.doc.classes[0].grouping | has("centers")' "$EVIDENCE/centers-before.state.json"` prints `false`. The `aria.txt` has the button `Use centers` and no `Center 1`.
- **Turn on.** Run `agent-browser --session "$SESSION" find role button click --name "Use centers" --exact`, then `agent-browser --session "$SESSION" wait --text "Round 1 of 3"`. Buttons `Remove centers` and `Rotate` appear, and so do three center-name buttons `Center 1`, `Center 2`, `Center 3`, each above one group card. The group cards read in the same order as before. Run `tivan-capture <run> centers-on`. `jq -c '.doc.classes[0].grouping.centers' "$EVIDENCE/centers-on.state.json"` prints `{"names":["Center 1","Center 2","Center 3"],"turn":0}`, and `.version` is `3`.
- **Rotate.** Run `agent-browser --session "$SESSION" find role button click --name "Rotate" --exact`, then `agent-browser --session "$SESSION" wait --text "Round 2 of 3"`, then `agent-browser --session "$SESSION" wait 1200` so the flight has landed, then `tivan-capture <run> centers-rotated`. The saved `.centers.turn` is `1`. The group at each center, in screen order, is `names[(c - turn) mod n]`. Check it with `jq -c '.doc.classes[0].grouping | . as $g | ($g.names|length) as $n | [range(0; $n) | $g.names[(. - $g.centers.turn + $n) % $n]]' "$EVIDENCE/centers-rotated.state.json"`, which must list the group names in the order their cards appear in `centers-rotated.aria.txt`. In `centers-on` that order was the saved `names` order, and now it is that list shifted by one: the last group is first. Membership and group names are unchanged.
- **Full cycle.** Press `Rotate` twice more, waiting for `Round 3 of 3` and then `Round 1 of 3`. After the second press the card order and `.centers.turn` (`0`) equal `centers-on`. While doing it, record each group's center after every press: each group appears under each of `Center 1`, `Center 2`, `Center 3` exactly once over the three rounds.
- **Rename a center.** Run `agent-browser --session "$SESSION" find role button click --name "Center 2" --exact`. A focused textbox `Center name` holds `Center 2`. Run `agent-browser --session "$SESSION" find label "Center name" fill "Reading Nook"`, `agent-browser --session "$SESSION" press Enter` and `agent-browser --session "$SESSION" wait --text "Reading Nook"`. `.centers.names[1]` is `"Reading Nook"`. Enter saves and removes the field, which hangs Chrome for Testing 151 (see the Groups gotchas). On that Chrome, save by clicking away instead, for example `find role heading click --name "Room 12 Morning"`. Press `Rotate` once and the name stays second on screen while a different group sits under it.
- **Survives reload.** Run `tivan-capture <run> centers-before-reload`, then `agent-browser --session "$SESSION" reload`, `agent-browser --session "$SESSION" wait --text "Shuffle again"` and `tivan-capture <run> centers-after-reload`. The round label, the center names and the card order in the `aria.txt` match, and `diff "$EVIDENCE"/centers-before-reload.state.json "$EVIDENCE"/centers-after-reload.state.json` prints nothing.
- **Shuffle keeps centers.** With centers on and the round above 1, run `agent-browser --session "$SESSION" find role button click --name "Shuffle again"`, then `tivan-capture <run> centers-shuffled`. `.grouping.centers` and `.grouping.names` are identical to the capture just before, the round label is unchanged, and `.grouping.of` differs. An identical deal is possible but rare, so press `Shuffle again` once more before calling it a failure.
- **Make groups restarts.** Run `agent-browser --session "$SESSION" find role button click --name "More groups"`, then `agent-browser --session "$SESSION" find role button click --name "Make groups"`, then `agent-browser --session "$SESSION" wait --text "Round 1 of 4"`. The saved centers have four names: the first three as they were, including `Reading Nook`, then `Center 4`, with `turn` `0`. Group names are all new. Pressing `Make groups` again at the same count also returns to `Round 1`.
- **Turn off.** Run `agent-browser --session "$SESSION" find role button click --name "Remove centers"`. The text `Remove the centers and their names?` appears with `Yes, remove` and `Keep them`, and nothing is removed yet. `Keep them` brings back `Remove centers` with the centers intact. Click `Remove centers` again, then run `agent-browser --session "$SESSION" find role button click --name "Yes, remove" --exact`. The button `Use centers` returns, the `Center <n>` buttons, `Rotate` and the round label are gone, and the cards sit in group order. `jq '.doc.classes[0].grouping | has("centers")'` prints `false`, and `.names` and `.of` are unchanged. Pressing `Use centers` again gives `Center 1` to `Center 4` at `Round 1 of 4`, so the old names did not survive.
- **On the whiteboard.** With centers on, run `agent-browser --session "$SESSION" set viewport 1920 1080` for a projector-like frame, then `agent-browser --session "$SESSION" find role button click --name "Whiteboard" --exact`. The top bar shows `Round <r> of <n>` and `Rotate`. The wall has one level-2 heading per group and the center names as plain text above the cards, in center order. Run `tivan-capture <run> centers-wall`. Press `Rotate`, wait for the next round text, run `agent-browser --session "$SESSION" wait 1200` so the cards have landed, and capture again: the same center names stay in place and the cards have moved on. Escape returns to Build, where the round label matches the saved `.centers.turn + 1`.
- **One group.** Run `agent-browser --session "$SESSION" find role button click --name "Fewer groups"` until the stepper reads `1`, `Make groups`, then `Use centers`. `agent-browser --session "$SESSION" snapshot -i -c` lists `button "Rotate" [disabled]` and `Round 1 of 1`.

## Gotchas

- With centers on, the cards on screen are in center order, not group order. After a rotation the first card is no longer group 0. Compare against the saved `.names` and `.centers.turn`, not against what the first card was.
- The flight takes about 0.8 s. `wait --text "Round 2 of 3"` returns as soon as the text changes, before the cards land. Wait another 1200 ms before a screenshot, or the PNG shows cards mid-air. The text, the aria tree and `state.json` are exact at once. With a reduced-motion preference there is no flight.
- `--name "Center 1"` also matches `Center 10`, `Center 11` and `Center 12` once there are that many groups. Add `--exact`.
- No button name other than `Rotate` contains "Rotate", in Build or on the whiteboard.
- The center name is outside the group card, so unlike a group name it is never a drop target while a student is selected for a move.
- `Remove centers` forgets the center names. `Make groups` keeps them, so `Make groups` with centers on is the way to deal fresh groups without retyping center names.
- A roster that shrinks below the group count does not change the groups until `Shuffle again`, which drops the last groups and their centers together. The round number wraps into the smaller cycle.
- Centers belong to a class. Switching class and back keeps each class's centers and round.
- The saved shape is `.doc.classes[n].grouping.centers = {names, turn}`, and the key is absent while centers are off. The round on screen is `turn + 1`.
