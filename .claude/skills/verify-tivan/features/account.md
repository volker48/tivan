# Account and Classroom sync

A teacher can sign in with Google from the Build top bar. Signed in, their classes are kept on the server instead of in the browser, so the same classes come back on any browser. They can also pull their active Google Classroom classes and rosters into Tivan with `Sync Classroom`, and sync again later to refresh them. Signed out, Tivan works as it always did and saves to the browser.

## Sub-features

- `account-signed-out`: a visitor sees `Sign in with Google` in the top bar and the page saves to `localStorage` as before.
- `account-sign-in`: the link goes through Google and back. The top bar then shows the teacher's first name, `Sync Classroom` and `Sign out`. With only the sample classes in the browser the account starts empty. With other classes in the browser the account takes them over. A doc already on the server wins, and the browser's copy is left alone.
- `account-save`: signed in, a change reaches the server about a second after the last edit, in one request at a time, and `localStorage` is not written.
- `account-return`: a fresh browser session that signs in gets the same classes back.
- `account-sync`: `Sync Classroom` lists the classes the teacher teaches. New ones start checked, ones already in Tivan say `already in Tivan, refreshed`. `Apply` brings the checked ones in and shows a line such as `Added 2 classes, 7 new students.`
- `account-resync`: syncing again changes nothing (`No changes made.`). Groups, group names, centers and students added by hand survive. A student who left Classroom disappears from the roster and the groups. A new student joins the smallest group.
- `account-unchecked`: an unchecked new class is not created. A class the teacher deleted shows up as new again and comes back only if checked.
- `account-sign-out`: `Sign out` sends any change still waiting, ends the session and reloads signed out.
- `account-session-ended`: when the server stops recognizing the session, the next save falls back to the browser, the doc stays on screen, and a dismissible `status` says so.
- `account-reauthorize`: when Google no longer honors the grant, `Sync Classroom` sends the teacher through Google again.
- `account-errors`: `?auth_error=<code>` shows a sign-in notice. A broken account service, an unreadable or newer server doc, and a refused save each show their own notice, and a server doc that could not be read is never overwritten.

## How to get to it (user POV)

- Build mode top bar, to the right of the `Tivan` heading: `Sign in with Google` for a visitor, or the first name, `Sync Classroom` and `Sign out` for a teacher. On a phone this group is its own row under the heading and `Whiteboard`.
- The sync panel opens at the top of the page under the top bar. The whiteboard has no account controls.

## Driving it with agent-browser

Preconditions:

- The local Worker is running with fake Google, serving the built app, and the app is not the `tivan-up` server. `tivan-up` serves only the static build, where `/api/me` answers with the app's own page and the page reads that as signed out. That makes it the right place for `account-signed-out` and the wrong place for everything else.
- A fresh `agent-browser` session with nothing saved, so the page starts from the seed. `APP` is the Worker's address with a trailing slash.
- The fake Google account has never signed in to this Worker, so the server holds no doc for it. A second sign-in finds the doc the first one stored.
- The bundled fake Google teaches three courses (`Period 1 Science` with five students, `Period 3 Math` with two, and an empty `Homeroom`). Nameless and over-long Classroom names are covered by `cargo test`, not here.
- The README's "Sign-in and sync locally" section lists the one-time setup (`.dev.vars`, local D1 migration). The next block starts the two servers and the session. Stop them with `pkill -f fake-google; pkill -f "cf dev"; pkill -f workerd`.

```bash
dx bundle --platform web --release
pnpm --dir deploy run fake-google > target/fake-google.log 2>&1 &
pnpm --dir deploy run dev > target/worker.log 2>&1 &
APP=http://localhost:8787/
SESSION=acct-verify
```

- **Signed out.** Run `agent-browser --session "$SESSION" open "$APP"`, then `agent-browser --session "$SESSION" wait --text "Sign in with Google"`. `snapshot -i -c` shows the link `Sign in with Google`, `Whiteboard` and the three sample classes, and no `Sync Classroom`. `agent-browser --session "$SESSION" storage local get tivan.state` prints the saved doc. Keep that output as `before`.
- **Sign in.** Run `agent-browser --session "$SESSION" find role link click --name "Sign in with Google"` and finish the fake Google consent. Then `agent-browser --session "$SESSION" wait --text "Sign out"`. The top bar shows the first name, `Sync Classroom` and `Sign out`, the link is gone, and the page shows `Welcome to Tivan` because the sample classes were not uploaded. `storage local get tivan.state` is byte for byte the same as `before`.
- **Cookie.** Run `agent-browser --session "$SESSION" cookies get`. `tivan_session` is `httpOnly` and `sameSite` Lax, and is `secure` unless the host is localhost.
- **Open the picker.** Run `agent-browser --session "$SESSION" find role button click --name "Sync Classroom"`, then `agent-browser --session "$SESSION" wait --text "Apply"`. The heading `Sync Classroom` (level 2) sits above one checkbox per course, each checked and named `<course> <n> students`, with `Apply` and `Cancel`.
- **Apply.** Run `agent-browser --session "$SESSION" uncheck <ref of one checkbox>` using a ref from a fresh `snapshot -i -c`, then `agent-browser --session "$SESSION" find role button click --name "Apply"` and `agent-browser --session "$SESSION" wait --text "Added"`. A `status` reads `Added <a> classes, <s> new students.` with `a` one less than the number of courses, and the class bar has a pill per created class. Run `agent-browser --session "$SESSION" find role button click --name "Done"`.
- **Saved to the server.** Wait 2 s. Run `agent-browser --session "$SESSION" open "${APP}api/doc"` and `agent-browser --session "$SESSION" get text body`. It is JSON with `"version":5`, and each created class has a `classroom_course_id` and each student a `classroom_user_id`. Run `agent-browser --session "$SESSION" open "$APP"` and `wait --text "Sign out"` to come back.
- **Edits are debounced.** Click `Make groups`, then `Shuffle again` three times in quick succession, and wait 2 s. `agent-browser --session "$SESSION" network requests --filter /api/doc` lists one `PUT` for the burst, not four.
- **Resync is a no-op.** Run `Sync Classroom` again. Every course now reads `already in Tivan, refreshed` and has no checkbox, except a course that was unchecked, which still has one. Run `Apply`. For all-linked courses the `status` reads `No changes made.`, and `network requests --filter /api/doc` shows no new `PUT`.
- **Resync after a change in Classroom.** Change the fake's rosters while it runs, dropping Margaret Hamilton from `course-1` and adding Dorothy Vaughan:

  ```bash
  curl -s -X POST localhost:8788/__courses -d '[{"id":"course-1","name":"Period 1 Science","students":[{"id":"s-101","name":"Ada Lovelace"},{"id":"s-102","name":"Grace Hopper"},{"id":"s-103","name":"Katherine Johnson"},{"id":"s-104","name":"Alan Turing"},{"id":"s-106","name":"Dorothy Vaughan"}]},{"id":"course-2","name":"Period 3 Math","students":[{"id":"s-201","name":"Emmy Noether"},{"id":"s-202","name":"Srinivasa Ramanujan"}]},{"id":"course-3","name":"Homeroom","students":[]}]'
  ```

  Then run `Sync Classroom` and `Apply`. The `status` reads `Added 1 new student, removed 1 student.` The removed student's `Remove <name>` button is gone, the new one is present, existing group cards keep their members and names, and a student added by hand beforehand is still there.
- **Unchecked and deleted classes.** Delete a synced class with `Delete` then `Yes, delete`, run `Sync Classroom`, and see its course back as a checked checkbox. Uncheck it, run `Apply`, and the class is not created.
- **Return on a fresh session.** Run `agent-browser --session "$SESSION" close`, open a new session, sign in again with the same account, and wait for `Sign out`. The same classes, groups and names are there, and `network requests --filter /api/doc` shows no `PUT` for this load.
- **Session ended.** Run `agent-browser --session "$SESSION" cookies clear`, then add a student (`find role textbox fill "Probe" --name "Add a student"` and `press Enter`). A `status` reading `You were signed out.` appears, the top bar returns to `Sign in with Google`, and the student is still on screen. `storage local get tivan.state` now contains `Probe`.
- **Sign out.** Sign in again, make an edit, and run `agent-browser --session "$SESSION" find role button click --name "Sign out"` at once. The page reloads signed out, and signing in again shows the edit, which proves the waiting change was sent before the session ended.
- **Sign-in error.** Run `agent-browser --session "$SESSION" open "${APP}?auth_error=access_denied"` and `wait --text "did not finish"`. A `status` reads `Google sign-in did not finish (access_denied). Please try again.` and `agent-browser --session "$SESSION" get url` no longer has the query. Open `${APP}?auth_error=Call%20555` and no notice appears.
- **Reauthorize.** Have the fake Google revoke the refresh token, then run `Sync Classroom`. The page leaves for Google again, and after consent returns signed in.
- **Cleanup.** Run `agent-browser --session "$SESSION" close` and stop the Worker and fake Google the way the block above started them.

## Gotchas

- While the page is still asking who is signed in, the top bar has no account controls at all. Always `wait --text "Sign in with Google"` or `wait --text "Sign out"` before a `find`, or the `find` reports no element.
- `tivan-capture` reads `localStorage` only. Signed in, its `state.json` shows the browser's copy from before sign-in and never the server's. Read the server's doc with `open "${APP}api/doc"` and `get text body`.
- The session cookie is `httpOnly`, so `eval` cannot see or change it. Use `cookies get` and `cookies clear`.
- Saves wait about 0.8 s after the last change. Wait 2 s before reading the server's doc or counting requests.
- The class name and the student count run together in a checkbox's name (`Period 1 Math 4 students`). Match on the class name and avoid `--exact`.
- A student's name is Classroom's full name, cut to 40 characters, and `Student` when Classroom sent none. A sync never renames a student or a class that is already linked.
- A class made from Classroom is linked to its course, not to its name. Two courses with the same name give two classes.
- After `account-session-ended`, signing in again loads the server's doc, so changes made while signed out are not in it. They stay in the browser's copy.
- Two browsers signed in at once overwrite each other: the last save wins.
- A server doc that is newer than the page, or that cannot be read, gives an `alert` that cannot be dismissed and turns saving off until reload. That is the safe behavior, not a failure.
- Google's consent screen is the fake's. Never put a real Google account, password or token into a run.
