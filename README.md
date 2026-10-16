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

## Development

Tivan is a Rust and Dioxus 0.7 web app. It runs entirely in the browser and saves classes to `localStorage`, so each browser keeps its own data. Google Classroom sync is not built yet. First run loads three sample classes.

Run the app locally with the Dioxus CLI (`cargo install dioxus-cli --version 0.7.10 --locked`):

```bash
dx serve --platform web
```

Run the domain and persistence tests natively:

```bash
cargo test
```

## Deploy

The release bundle is served as static assets by a Cloudflare Worker named `tivan`. `deploy/` holds the pinned `cf` and `wrangler` tooling with install scripts disabled. Sign in once with `cf auth login`, then:

```bash
./scripts/deploy.sh
```
