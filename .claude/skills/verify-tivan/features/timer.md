# Timer

On the whiteboard a teacher starts a 1, 3 or 5 minute countdown, or one of any whole number of minutes up to 180 typed into a field that starts at 10, that shows as large digits, pauses and resumes it, cancels it, and gets a full-screen `Time's up!` when it ends. The timer keeps running when the teacher goes back to Build, where a chip in the top bar shows it. The timer is not saved, so a reload clears it.

## Sub-features

- `timer-start`: `Start a <n> minute timer` starts the countdown.
- `timer-custom`: one pill holds spinbutton `Minutes` (starts at `10`) and a `min ▶` button named after it, `Start a <n> minute timer`. Enter in the field starts it too. Anything but a whole number from 1 to 180 disables the button, which is then named `Start timer`. The field keeps the last entry while the whiteboard stays open.
- `timer-pause`: `Pause` freezes the digits and shows `Paused`. `Resume` continues.
- `timer-cancel`: `Cancel` clears the countdown and brings the presets and the minutes field back.
- `timer-done`: at zero, alertdialog `Time's up` with `Dismiss`. Escape also dismisses it.
- `timer-chip`: in Build, the top bar shows the remaining time with `Timer` or `Paused`, or `Time's up!` with `Dismiss`. Clicking the running chip opens the whiteboard.

## How to get to it (user POV)

- Whiteboard mode, the `Timer` group at the right of the top bar.
- Build mode top bar, only while a timer exists.

## Driving it with agent-browser

Preconditions:

- Baseline seed, `McCurdy HR` selected, on the whiteboard (`find role button click --name "Whiteboard" --exact`), then `press Escape` once to leave fullscreen. See the fullscreen gotcha in [Whiteboard](./whiteboard.md).
- Every command below is prefixed with `source target/verify/<run>/run.env &&`.

- **Start.** Run `agent-browser --session "$SESSION" find role button click --name "Start a 1 minute timer"`. The `Timer` group now holds `Pause` and `Cancel`. `agent-browser --session "$SESSION" get text ".timer-strip"` prints `1:00` or `0:59`, and a second read a few seconds later prints a smaller value.
- **Custom.** Run `agent-browser --session "$SESSION" fill 'input[aria-label="Minutes"]' 7`, then `agent-browser --session "$SESSION" press Enter`. The strip prints `7:00` or `6:59`. Cancel it, and the field still reads `7`. Fill `0`, `181` or nothing, and `Start timer` is disabled.
- **Pause and resume.** Run `agent-browser --session "$SESSION" find role button click --name "Pause"`. The strip shows `Paused`, and two reads 3 s apart print the same digits. Run `agent-browser --session "$SESSION" find role button click --name "Resume"`. The digits move again.
- **Chip in Build.** Run `agent-browser --session "$SESSION" find role button click --name "Back to Build"`. The Build top bar has a button with the remaining `m:ss` and `Timer`. Clicking it returns to the whiteboard in fullscreen, so run `agent-browser --session "$SESSION" press Escape` once to leave fullscreen.
- **Time's up.** Run `agent-browser --session "$SESSION" wait --text "Time's up!" --timeout 75000`. Alertdialog `Time's up` covers the wall. Run `tivan-capture <run> timer-done`. Then run `agent-browser --session "$SESSION" press Escape`. The dialog closes, the presets return, and the page is still on the whiteboard.
- **Cancel.** Start a 3-minute timer, then run `agent-browser --session "$SESSION" find role button click --name "Cancel"`. The strip disappears and the three presets and the minutes pill return.
- **Not saved.** Start a timer and reload. The page opens in Build with no chip. That is expected behavior, not a failure.

## Gotchas

- The shortest preset is one minute, so `timer-done` costs at least 60 s. Use `wait --text … --timeout`, not repeated sleeps.
- The digits update four times a second. Assert ranges or "decreased", never an exact value.
- `--name "Cancel"` is unique on the whiteboard but not in Build, where the move bar and paste box also have `Cancel`.
- Time's up while in Build shows only in the chip (`Time's up!` / `Dismiss`). The full-screen dialog belongs to the whiteboard.
- A finished timer absorbs the first Escape on a windowed whiteboard. A second Escape leaves the whiteboard. In fullscreen, one more Escape comes first, to leave fullscreen.
- The countdown is in memory only, and `state.json` never contains it. Prove timer behavior with screen captures alone.
