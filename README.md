# Tivan

This is a tool for teachers to use to assist in creating dynamic collections during the day in the classroom. The problem being solved is taking N students and putting them into M groups and then shuffling those groups.

## Design Goals

* Of utmost importance is keeping the UI simple and easy to use for busy teachers who are always juggling multiple tasks at once, but it shouldn't be a sterile and boring product either.
* Connects to Google Classroom to sync student names.
* Teacher can create multiple "classes" since they may not have the same students throughout the entire day.
* Multiple modes
    - Primary mode is choosing the N groups and being able to assign students to groups or shuffle them. This mode will primarly be used "out of band" before instructional time.
    - Secondary mode is designed for use during instructional time while students are in class and the UI should be suitable for projecting onto the white board.

## Group creation mode

In group creation mode the primary task is to create the classes and groups. The reason for classes is for high school and elementary school where the teacher may not have the same students throughout the entire day. Classes define the total population that is possible to go into the N groups. Once the teacher has selected a class then the UI is structured around creating the groups. There must be options to select how many groups the teacher desires. If the teacher selects say 3 groups then the students should be sorted into 3 groups randomly and the groups should default to having random names from a G rated safe word list of adjective combined with animal in an alliteration like "Majestic Monkeys" for example or "Fantastic Flamingoes".


## Whiteboard mode

During whiteboard mode most of the UI should drop away and the focus is on the defined groups. There should be some additional options available while in whiteboard mode. There should be an Oracle button that randomly selects a student to answer a question. Once the student is chosen the UI should show "The oracle has chosen Tommy!" for example. A smaller button next to it picks one of the groups instead, showing the group's name and its members. Before the answer is shown the oracle should spin for about two seconds, flashing through the students (or groups) fast and then slowing down to land on the chosen one, unless the system asks for reduced motion. There should also be a button in this UI to create a timer that will be displayed on screen.

## Phone remote

On the whiteboard, **Phone remote** shows a QR code. The teacher scans it with their phone, and from then on shaking the phone shuffles the groups on the board: the cards wobble, every name is thrown up across the wall, and each one drifts down into its new group. The phone also has buttons for the oracle, picking a group and rotating centers, and a command sent while the board is in Build switches it to the whiteboard first.

The phone opens the same app at `/#remote=<room>`. It shows only the remote and never loads the teacher's classes; the board tells it only whether the class has students, groups and centers. The board and the phone meet in a Durable Object (`RemoteRoom` in `deploy/src/remote-room.ts`) named by a random room code. The first board and the first phone to join each claim their seat with a random device key, so once the teacher's phone has joined, a student who photographs the QR code cannot take over. If a student's phone gets there first, the teacher's phone says the board is paired with another phone, and **Turn off remote** followed by **Phone remote** gives a fresh code. **Turn off remote** closes the room for good and tells the phone. It is offered only while the board is connected, so the relay always hears it. The board keeps its pairing in `localStorage["tivan.remote"]`, apart from the saved classes, so the same phone keeps working the next morning. On iPhone the teacher taps **Turn on shake** once, because iOS asks before a page may read the motion sensor. Every browser can also tap **Shuffle**.

## Development

Tivan is a Rust and Dioxus 0.7 web app. Signed out, it runs entirely in the browser and saves classes to `localStorage`, so each browser keeps its own data, and a first run loads three sample classes. A teacher can also sign in with Google. Their classes and groups are then stored in the Worker's D1 database, so they come back on any browser, and **Sync Classroom** pulls their active Google Classroom courses and rosters into classes. Students never sign in. Sync links a class to its course and a student to their Classroom user, so syncing again refreshes rosters without touching groups, hand-added students, or names the teacher edited. Saves are last write wins, so editing from two browsers at once can overwrite earlier edits.

Run the app locally with the Dioxus CLI (`cargo install dioxus-cli --version 0.7.10 --locked`):

```bash
dx serve --platform web
```

`dx serve` has no Worker, so there the phone remote never connects: the board's panel stays on "Connecting to Tivan…" and a phone on "Connecting…". Run the bundle through the Worker (below) to try it.

Run the domain and persistence tests natively:

```bash
cargo test
```

### Sign-in and sync locally

`dx serve` serves only the static app. Sign-in, the saved document and the Classroom calls come from the Worker in `deploy/`, so run the bundle through it. A local fake Google stands in for the real one, so no Google project is needed. Needs Node 22.18 or newer.

```bash
dx bundle --platform web --release
pnpm --dir deploy install --frozen-lockfile
printf 'GOOGLE_CLIENT_ID=fake-client-id\nGOOGLE_CLIENT_SECRET=fake-client-secret\nTOKEN_ENC_KEY=%s\nGOOGLE_AUTHORIZE_URL=http://localhost:8788/authorize\nGOOGLE_TOKEN_URL=http://localhost:8788/token\nCLASSROOM_API_URL=http://localhost:8788\n' "$(openssl rand -base64 32)" > deploy/.dev.vars
pnpm --dir deploy run db:migrate:local
pnpm --dir deploy run fake-google   # terminal 1, port 8788
pnpm --dir deploy run dev           # terminal 2, port 8787
```

Open `http://localhost:8787`. Rebuild the bundle after Rust changes. To try real Google locally, put the real client id and secret in `deploy/.dev.vars`, drop the three URL lines, and add `http://localhost:8787/auth/callback` as an authorized redirect URI. Worker checks run from `deploy/` with `pnpm run typecheck`, `lint`, `format:check` and `test`.

## Deploy

The release bundle is served as static assets by a Cloudflare Worker named `tivan`. `deploy/` holds the pinned `cf` and `wrangler` tooling with install scripts disabled. Sign in once with `cf auth login`, then:

```bash
./scripts/deploy.sh
```

The first deploy of the account features needs one-time setup:

1. In Google Cloud, enable the Classroom API and create a Web OAuth client. Add the scopes `classroom.courses.readonly` and `classroom.rosters.readonly`, and register `https://<your domain>/auth/callback` as an authorized redirect URI. While the consent screen is in Testing, only listed test users can sign in and refresh tokens expire after 7 days. Public use needs Google's app verification.
2. Create the database and apply `deploy/migrations/`: `pnpm --dir deploy exec cf d1 create --name tivan`, then `pnpm --dir deploy exec cf d1 migrations apply <uuid>`. Run the migrations before deploying a Worker version that needs them.
3. Deploy with the secrets `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET` and `TOKEN_ENC_KEY` (base64 of 32 random bytes): `./scripts/deploy.sh --secrets-file /absolute/path/secrets.env`. Keep that file outside the repo and delete it afterwards. Changing `TOKEN_ENC_KEY` later makes teachers sign in again.

The Worker stores student names, so check what your school or district requires before teachers other than you use it.
