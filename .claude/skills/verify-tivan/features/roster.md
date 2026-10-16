# Roster

A teacher adds students to the current class one at a time or by pasting a column of names, sees a live count of what a paste will add, gets a notice for lines that are too long to keep, and removes students. Removing or adding a student updates any existing groups.

## Sub-features

- `roster-add`: type a name in `Add a student` and press `Add` or Enter.
- `roster-remove`: the `Remove <name>` button removes one student.
- `roster-paste`: `Paste a list` opens a box, and `Add these names` adds every line.
- `roster-too-long`: names over 40 characters are skipped with a dismissible notice.
- `roster-toggle`: the `Students <n>` button collapses and expands the panel.

## How to get to it (user POV)

- Build mode, the `Students` panel under the class header. It starts expanded for an ungrouped class.

## Driving it with agent-browser

Preconditions:

- Baseline seed, `McCurdy HR` selected (18 students, panel expanded).
- Every command below is prefixed with `source target/verify/<run>/run.env &&`.

- **Add one.** Run `agent-browser --session "$SESSION" find role textbox fill "Rin" --name "Add a student"` and `agent-browser --session "$SESSION" press Enter`. The roster ends with `Rin`, the toggle reads `Students 19`, the header meta reads `19 students`, and the textbox is empty again.
- **Remove one.** Run `agent-browser --session "$SESSION" find role button click --name "Remove Rin"`. Rin is gone and the counts return to 18.
- **Paste a list.** Run `agent-browser --session "$SESSION" find role button click --name "Paste a list"`, then `agent-browser --session "$SESSION" find placeholder "One name per line. A column copied from a spreadsheet works." fill $'Rin\nSol\n'"$(printf 'x%.0s' {1..41})"`. The hint reads `2 names ready, 1 line too long and will be skipped`. Run `agent-browser --session "$SESSION" find role button click --name "Add these names"`. `Rin` and `Sol` are added (20 students), and a `status` notice reads `2 names added, 1 line too long. Names can be up to 40 characters.` with `Dismiss`.
- **Dismiss the notice.** Run `agent-browser --session "$SESSION" find role button click --name "Dismiss"`. The notice is gone.
- **Proof.** Reload and run `tivan-capture <run> roster-final`. `jq '[.doc.classes[0].students[].name] | length' "$EVIDENCE/roster-final.state.json"` gives `20` and the list ends `"Rin","Sol"`.

## Gotchas

- `find role button --name "Rin"` also matches `Remove Rin`. Prefer the `Remove <name>` handle, or add `--exact`.
- An empty `Add a student` submit does nothing and shows no error. A 41-character name shows the alert `Keep names to 40 characters or fewer.`
- Duplicate names are allowed. Each student has its own id, so assert on counts and ids, not on name uniqueness.
- Once a class is grouped, the panel starts collapsed on the next visit. Click `Students <n>` to open it.
- Adding a student to a grouped class places them in a group. Removing one takes them out of their group. Check `.grouping.of` when the class is grouped.
