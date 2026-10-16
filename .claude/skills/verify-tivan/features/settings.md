# Settings

A teacher opens Settings from the gear on the Build top bar and picks how student avatars are dressed. `Seasonal (automatic)` puts a witch hat on every avatar in October and a Santa hat from Dec 1 to Dec 25. `Halloween` and `Christmas` force one skin, and `Plain` turns skins off. The choice is saved and applies to every avatar on the whiteboard.

## Sub-features

- `settings-open`: the gear opens Settings, and `Back to Build` or Escape returns.
- `settings-skin`: choose Plain, Seasonal, Halloween or Christmas.
- `settings-skin-wall`: the chosen skin shows on every avatar on the whiteboard.

## How to get to it (user POV)

- Build mode, the gear button named `Settings` at the right of the top bar.

## Driving it with agent-browser

Preconditions:

- Baseline seed, `McCurdy HR` selected. Run `Make groups` once so the whiteboard has members.
- Every command below is prefixed with `source target/verify/<run>/run.env &&`.

- **Open.** Run `agent-browser --session "$SESSION" find role button click --name "Settings"`. The heading `Settings` and a group `Avatar skins` with four radios (`Plain`, `Seasonal (automatic)`, `Halloween`, `Christmas`) show. A fresh profile has `Seasonal (automatic)` checked, and its hint ends `Today: <skin or no skin>.`
- **Choose.** Run `agent-browser --session "$SESSION" find text "Christmas" click --exact`. `eval "JSON.parse(localStorage.getItem('tivan.state')).doc.settings"` gives `{"skin":{"fixed":"christmas"}}`. `plain` and `auto` are stored as strings.
- **Wall.** Click `Back to Build`, then `Whiteboard`. `eval "[...new Set([...document.querySelectorAll('svg.avatar use')].map(u=>u.getAttribute('href')).filter(h=>h.startsWith('#skin-')))]"` lists `#skin-christmas`. With `Plain` it lists nothing.
- **Proof.** Reload, open Settings, and the chosen radio is still checked. Run `tivan-capture <run> settings-final`.

## Gotchas

- `find role radio click --name …` did not change the selection in the run that recorded this recipe, while a real mouse click, the label text and `element.click()` all did. Click the label text.
- `Seasonal (automatic)` follows the machine's date, so assert it against the date, not a fixed skin.
