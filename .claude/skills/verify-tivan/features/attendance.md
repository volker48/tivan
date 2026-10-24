# Attendance

A teacher marks a student absent for the day. An absent student drops out of the group cards, the whiteboard wall and the oracle, but keeps their group, and everyone is back the next day without the teacher doing anything. Making groups or shuffling while someone is away splits only the students present, and files the absent into the smallest groups so the groups stay even when they return.

## Sub-features

- `absent-roster`: the `<name> absent today` toggle (visible text `Absent`) on each roster entry marks or clears an absence.
- `absent-board`: tapping a member on a group card shows the moving bar, whose `Absent today` button marks them absent.
- `absent-tray`: the region `Absent today` lists absent students. `Mark <name> here` brings one back to their group.
- `absent-oracle`: on a student pick, the whiteboard reveal's `Not here today` marks the pick absent, gives back their turn, and picks again.
- `absent-lapse`: absences hold for the day they were marked on and clear on any later day.

## How to get to it (user POV)

- Build mode: the `Students` panel toggles, a member chip on a group card, and the `Absent today` tray between the roster and the group controls. The class header meta reads `<n> students, <k> absent today`.
- Whiteboard mode: `Ask the oracle`, then `Not here today` in the reveal.

## Driving it with agent-browser

Preconditions:

- Baseline seed, `McCurdy HR` selected (18 students, panel expanded).
- Every command below is prefixed with `source target/verify/<run>/run.env &&`.

- **Mark from the roster.** Run `agent-browser --session "$SESSION" find role button click --name "Abigail absent today"`. The meta reads `18 students, 1 absent today`, the tray shows `Mark Abigail here`, and `.doc.classes[0].absent` is `{"on":{"year":…,"month":…,"day":…},"students":[0]}`.
- **Groups skip the absent.** Click `Make groups` with 3 groups. The cards show 6, 6 and 5 members with no Abigail, and `.grouping.of["0"]` names the 5-member group.
- **Mark from the board.** Click a member chip (with `--exact`), then `agent-browser --session "$SESSION" find role button click --name "Absent today" --exact`. They move from their card to the tray and the meta counts 2 absent.
- **Oracle.** Enter `Whiteboard`. `Ask the oracle` reads `16 left` and the wall has no absent names. Pick, then click `Not here today`. A new pick lands with `14 left`, and the earlier pick is in `.absent.students` and not in `.oracle.drawn`.
- **Bring one back.** In Build, click `Mark Abigail here`. She is back on the card she was filed in.
- **Proof.** Reload: the tray and meta are unchanged.
- **Next day.** Shift the page clock with `eval` (`window.Date` subclass adding 24 h to `Date.now()` and to `new Date()`), wait a second for the tick, and the tray is gone, the meta reads `18 students`, and `.doc.classes[0].absent` is absent from the saved state. This only moves the clock, never the stored state.

## Gotchas

- `--name "Absent today"` also matches every roster toggle (`… absent today`) and the tray region. Use `--exact` for the moving bar's button.
- A member chip's name is a substring of `Mark <name> here` and `Remove <name>`. Use `--exact`.
- The absence date is the browser's local date, so a run near midnight can see absences lapse on their own.
