# Phone remote

On the whiteboard, `Phone remote` shows a QR code that opens Tivan on the teacher's phone as a remote. Shaking the phone (or tapping `Shuffle`) reshuffles the groups on the board: the cards wobble, every name flies up across the wall and drifts down into its new group. The phone also asks the oracle, picks a group and rotates centers. The phone never loads the teacher's classes. The first phone to join holds the remote; another phone is refused. `Turn off remote` retires the room and tells the phone.

## Sub-features

- `remote-pair`: `Phone remote` makes a pairing (saved in `localStorage["tivan.remote"]`) and opens dialog `Phone remote` with a QR code, the link as text, and `Waiting for your phone…`.
- `remote-connect`: the link on a phone shows `Connected to the whiteboard`. The board's button reads `Remote on` and the dialog says `Phone connected. Shake it to shuffle the groups!`.
- `remote-shake`: a shake (three hard jolts within a second) or `Shuffle` reshuffles the board's groups with the burst animation, and the phone shows `Shuffled!`.
- `remote-commands`: `Ask the oracle` and `Pick a group` on the phone open the board's oracle dialog; `Rotate centers` appears only for a class with centers.
- `remote-from-build`: a command while the board is in Build switches it to the whiteboard first.
- `remote-claimed`: a second phone opening the same link reads `This whiteboard is paired with another phone…`.
- `remote-off`: `Turn off remote` stores `null` in `tivan.remote`, and the phone reads `The remote was turned off on the whiteboard…`. The next `Phone remote` makes a new room code.
- `remote-persist`: reloading the board keeps the pairing, and the phone reconnects on its own.

## How to get to it (user POV)

- On the whiteboard, `Phone remote` at the end of the top bar. After pairing it reads `Remote on` while a phone is connected.
- On the phone, scan the QR code, or open the link printed under `No camera? Open`. On iPhone, tap `Turn on shake` once.

## Driving it with agent-browser

Preconditions:

- The relay lives in the Worker, so `tivan-up` (`dx serve`) cannot drive it: the board stays on `Connecting to Tivan…`. Build the bundle and start the Worker as in the README's "Sign-in and sync locally" (no fake Google needed): `dx bundle --platform web --release`, `pnpm --dir deploy install --frozen-lockfile`, a `deploy/.dev.vars`, `pnpm --dir deploy run db:migrate:local`, `pnpm --dir deploy run dev`. Open `http://localhost:8787/`.
- Two sessions with separate profiles: `tv-board` at 1280×800 and `tv-phone` at 390×844 (`agent-browser --session tv-phone set viewport 390 844`).
- On the board, `McCurdy HR` with groups made (`Make groups`), then `Whiteboard`.

- **Pair.** On `tv-board`, `find role button click --name "Phone remote"`, then `wait --text "Waiting for your phone"`. `eval "document.querySelector('.remote-link code').textContent"` prints `http://localhost:8787/#remote=<22 chars>`, and `eval "JSON.parse(localStorage.getItem('tivan.remote')).room"` prints the same room.
- **Connect.** On `tv-phone`, `open "<link>"`, then `wait --text "Connected to the whiteboard"`. `eval "Object.keys(localStorage)"` is `["tivan.remote.device"]` only. On `tv-board`, the button reads `Remote on`.
- **Shake.** Save the board's `.doc.classes[0].grouping.of`. On `tv-phone`, `find role button click --name "Turn on shake"` if present (headless Chrome asks, like iOS), then dispatch a shake with `eval`:

  ```js
  (() => { let i = 0;
    const tick = () => {
      const x = i % 10 < 3 ? (Math.floor(i / 10) % 2 ? -22 : 22) : 0;
      window.dispatchEvent(new DeviceMotionEvent("devicemotion", {
        acceleration: { x, y: 0, z: 0 },
        accelerationIncludingGravity: { x, y: 0, z: 9.81 },
        interval: 16,
      }));
      if (++i < 40) setTimeout(tick, 16);
    };
    tick(); })()
  ```
 Within about 0.4 s the board's cards wobble and the names fly; a screenshot 1 s after the shake shows names airborne. After 2.5 s, `.grouping.of` differs and the phone shows `Shuffled!`.
- **Commands.** On `tv-phone`, `Ask the oracle`: the board shows dialog `The oracle has chosen` and `17 left`. `Pick a group`: the dialog names a group.
- **From Build.** Press Escape on the board until Build shows, then tap `Shuffle` on the phone. The board is back on the whiteboard (`Back to Build` visible).
- **Claimed.** A third session opening the same link reads `paired with another phone`, and the board still reads `Remote on`.
- **Turn off.** On `tv-board`, open the dialog, wait for its pop-in (`wait 600`), then `Turn off remote`. It is absent while the board is connecting or retrying, and in a tab another tab replaced. The phone reads `turned off on the whiteboard`, `tivan.remote` is `"null"`, and pressing `Phone remote` again gives a different room code.
- **QR.** Screenshot the dialog and decode it (for example with `jsqr` and `pngjs` in a scratch directory). It decodes to the same link as the text.

## Gotchas

- Restarting or rebuilding while `cf dev` runs: deleting `target/dx/tivan/release/web/public` makes the running Worker serve `{"error":"not_found"}` for `/`. Restart `pnpm --dir deploy run dev` after a rebundle.
- `dx bundle --release` downloads binaryen and esbuild from GitHub through its own HTTP client, which a proxied sandbox may refuse. `curl` the same release and unpack it into `~/.local/share/.dx/tools/binaryen-129/` (and esbuild into `esbuild-0.27.3/`).
- The dialog pops in with a scale and tilt, so a click in its first half second can miss. `wait 600` first.
- After the board reloads, the phone reconnects on its own backoff (up to 15 s), so the board reads `Phone remote` until it does.
- A synthetic shake only counts when each jolt rises from below 14 m/s². Keep still frames between the pushes.
