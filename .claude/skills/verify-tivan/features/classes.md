# Classes

A teacher keeps several classes, one per set of students, switches between them from a row of pills, creates a new one (optionally pasting the whole roster at once), renames it, and deletes it after a confirmation. With no classes left, the page shows a welcome screen with the new-class form.

## Sub-features

- `classes-switch`: choose a class pill to make it current.
- `classes-create`: `+ New class` opens a form, and `Create class` adds and selects the class.
- `classes-create-roster`: the form's students box adds a pasted list in the same step.
- `classes-rename`: `Rename` turns the heading into a `Class name` textbox.
- `classes-delete`: `Delete` asks for confirmation before removing the class.
- `classes-welcome`: with zero classes, `Welcome to Tivan` shows the form with no `Cancel`.

## How to get to it (user POV)

- Build mode, the `Classes` navigation under the top bar.
- The class header (heading, `Rename`, `Delete`) below it.

## Driving it with agent-browser

Preconditions:

- Baseline seed, three classes, `Room 12 Morning` selected.
- Every command below is prefixed with `source target/verify/<run>/run.env &&`.

- **Switch.** Run `agent-browser --session "$SESSION" find role button click --name "Period 3 Science"`. The level-2 heading reads `Period 3 Science`, the meta text reads `20 students`, and the pill reports `pressed=true`. `.doc.selected` in saved state is the id of `Period 3 Science`.
- **Open the form.** Run `agent-browser --session "$SESSION" find role button click --name "+ New class"`. The heading `New class` appears with `Class name`, the students box, `Create class` and `Cancel`.
- **Create with a roster.** Run `agent-browser --session "$SESSION" find label "Class name" fill "Period 5 Art"` and `agent-browser --session "$SESSION" find label "Students (optional" fill "Rin\nSol\nTeo"`, with the `\n` as real newlines (pass the text through `$'Rin\nSol\nTeo'`). The hint reads `3 names ready`. Run `agent-browser --session "$SESSION" find role button click --name "Create class"`. A fourth pill `Period 5 Art` is pressed and the heading reads `Period 5 Art` with `3 students`. Saved state has a fourth class with those three students.
- **Rename.** Run `agent-browser --session "$SESSION" find role button click --name "Rename"`, then `agent-browser --session "$SESSION" find label "Class name" fill "Period 5 Studio Art"` and `agent-browser --session "$SESSION" press Enter`. The heading and the pill both read `Period 5 Studio Art`. Clicking away, for example `find role heading click --name "Tivan"`, also saves.
- **Delete with confirmation.** Run `agent-browser --session "$SESSION" find role button click --name "Delete" --exact`. The text `Delete Period 5 Studio Art for good?` appears with `Yes, delete` and `Keep it`. Run `agent-browser --session "$SESSION" find role button click --name "Yes, delete"`. The pill is gone, another class is selected, and saved state has three classes.
- **Proof.** Reload and run `tivan-capture <run> classes-final`. The pills and saved state match the steps above.

## Gotchas

- `--name "Delete"` without `--exact` can also hit `Yes, delete` once confirmation is open.
- With Chrome for Testing 151, `press Enter` in the `Class name` box hangs the page, the same as the `Group name` box. See the [Groups](./groups.md) gotchas.
- The create form requires a name. Submitting it empty shows the alert `Type a name first.` and creates nothing.
- Roster lines over 40 characters are skipped and reported in a `status` notice (`N names added, M lines too long…`). See [Roster](./roster.md).
- The welcome screen needs all three seed classes deleted first. It has no `Cancel` button.
- Switching class resets the group stepper and the roster panel's open state for that class.
