# Avatars

Every student has an animal avatar, a die-cut sticker beside or above their name in Build and on the whiteboard. There are 30 animals, so a class of up to 30 has no two alike. On the whiteboard, avatars breathe slowly, each on its own clock, blink now and then, one at a time, and every few seconds one of the wall's avatars fidgets: a head tilt, a wiggle, a bounce, a giggle, a lean, a doze, or now and then a twirl. Pointing at a student's roster row, group chip or wall tile makes their avatar smile and hop. When the whiteboard opens, each card's avatars pop up in a quick wave. When the oracle lands on a student, their avatar leaps in grinning with a burst of sparks, wiggles and settles, and a chosen group's members pop up one after another. Shuffled students grin while their chips fly to a new group. With reduced motion on, nothing blinks or moves and everything shows in its final state.

## Sub-features

- `avatars-set`: 30 different animals, saved as each student's `avatar`.
- `avatars-blink`: on the whiteboard, one avatar at a time shuts its eyes for a moment.
- `avatars-idle`: wall avatars breathe, and one at a time fidgets.
- `avatars-hover`: pointing at a roster row, a group chip or a wall tile makes the avatar smile and hop.
- `avatars-arrive`: wall avatars pop up in a wave when the whiteboard opens.
- `avatars-cheer`: the oracle's chosen student leaps in with sparks, and a chosen group's members pop up in turn.
- `avatars-shuffle`: avatars grin while their chips fly during `Shuffle again`.
- `avatars-still`: with reduced motion, none of the above moves.

## How to get to it (user POV)

- Build: the student rows in the roster panel and the member chips on each group card.
- Whiteboard: the member tiles, `Ask the oracle` and `Pick a group`.

## Driving it with agent-browser

Preconditions:

- Baseline seed, `McCurdy HR` selected, viewport 1280x720 (`agent-browser --session "$SESSION" set viewport 1280 720`).
- Every command below is prefixed with `source target/verify/<run>/run.env &&`.

- **Set.** Run `tivan-capture <run> avatars-seed`. In `avatars-seed.state.json`, `.version` is `7` and `[.doc.classes[2].students[].avatar] | unique | length` is `27`: `Reading Club`'s 27 students all have different animals.
- **Hover.** `agent-browser --session "$SESSION" hover "li.roster-item:nth-child(3)"`, `wait 250`, then `eval "getComputedStyle(document.querySelector('li.roster-item:nth-child(3) svg.avatar .av-glad')).opacity"` prints `1` (happy eyes) and a screenshot shows that student's avatar with arcs for eyes.
- **Shuffle.** Click `Make groups`, `wait 800`, click `Shuffle again`, `wait 200`, then `eval "[...new Set([...document.querySelectorAll('.chip')].filter(c => c.getAnimations().length).map(c => getComputedStyle(c.querySelector('.av-glad')).opacity))]"` prints `["1"]`: every flying chip's avatar has happy eyes. After `wait 1200`, `eval "[...new Set([...document.querySelectorAll('.chip .av-glad')].map(g => getComputedStyle(g).opacity))]"` prints `["0"]`.
- **Arrive.** Run `agent-browser --session "$SESSION" find role button click --name "Whiteboard" --exact` and take `agent-browser --session "$SESSION" screenshot "$EVIDENCE/avatars-arriving.png"` straight away: the names show with blank space above most or all of them. After `wait 1500`, `tivan-capture <run> avatars-wall` shows every avatar.
- **Blink.** On the wall, `agent-browser --session "$SESSION" eval "new Promise(r => { let seen = 0, n = 0; const t = setInterval(() => { seen += document.querySelectorAll('svg.avatar.blink').length; if (++n === 40) { clearInterval(t); r(seen); } }, 75); })"` samples for 3 s and prints a number above 0. A blinking avatar shows shut eyes in a screenshot taken at that moment. Back in Build the same sample prints `0`.
- **Idle.** On the wall, `agent-browser --session "$SESSION" eval "[...new Set([...document.querySelectorAll('.wall svg.avatar')].map(a => a.getAnimations().map(x => x.animationName + ':' + x.playState).join()))]"` prints `["av-breathe:running"]`. `eval "new Promise(r => { const seen = new Set(); let n = 0; const t = setInterval(() => { for (const b of document.querySelectorAll('.wall .wall-avatar')) for (const c of b.classList) if (c.startsWith('fidget-')) seen.add(c); if (++n === 80) { clearInterval(t); r([...seen]); } }, 100); })"` samples for 8 s and lists at least one `fidget-…` class.
- **Cheer.** Click `Ask the oracle`, then `wait --text "Pick again" --timeout 8000`, `wait 500` and screenshot: the chosen avatar is in the air or landing, with arcs for eyes and sparks bursting around it. After `wait 2500`, `eval "(() => { const a = document.querySelector('[role=dialog] svg.avatar'); return [getComputedStyle(a.querySelector('.av-glad')).opacity, a.getAnimations({ subtree: true }).filter(x => x.playState === 'running').length, [...new Set([...document.querySelectorAll('.sparks i')].map(i => getComputedStyle(i).opacity))]]; })()"` prints `["0", 0, ["0"]]`: open eyes, nothing still moving, every spark faded. `Done`, then `Pick a group` and a screenshot after `wait 400` shows the members popping up, the first ones with arcs for eyes.
- **Still.** `agent-browser --session "$SESSION" set media light reduced-motion`, then reload and open the whiteboard. The Blink sample on the wall prints `0`, the Idle fidget sample prints `[]`, and the breathing read prints `[""]`: no animation is left on any avatar. `Ask the oracle` lands with no spin, and the Cheer read prints `["0", 0, ["0"]]` after `wait --text "Pick again"` and `wait 200`. Opening the whiteboard and screenshotting after `wait 300` shows every avatar in place.

## Gotchas

- `wait --text "17 left"` matches the `Ask the oracle 17 left` button as soon as the draw starts, while the spin is still running. Wait for `Pick again` to know the card has landed.
- Blinks are random and brief (about a tenth of a second). Sample over a few seconds; never assert that a given avatar blinks.
- `hover` takes a CSS selector, and the roster rows have no accessible name of their own, so the recipe names the row by position.
- Read a reveal straight after `Pick again` appears and the first frame may not have run yet, even with reduced motion. Wait a moment first.
- Reloading returns to Build, so wait for `Whiteboard`, not a whiteboard label.
- The landed card pops up first, and its avatar is invisible for a moment before it leaps in. It is stretched or squashed for most of the next second. Judge its settled look after `wait 2500`.
