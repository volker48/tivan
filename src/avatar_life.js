// Brings the whiteboard's avatars to life between the big moments. They blink, one at a time and
// at random, and now and then one of the wall's avatars fidgets: a head tilt, a wiggle, a bounce, a
// giggle, a lean, a doze or, once in a while, a twirl. Each is a class the stylesheet animates for
// a moment, and nothing runs in between. A CSS animation on every avatar would restyle and repaint
// all of them every frame for as long as the wall is up. Build's avatars are too small for any of
// this to show, so they are left alone.

// About how long one avatar goes between blinks, and between fidgets, and the shortest gap between
// any two, so a wall full of avatars stays calm enough to teach in front of.
const BLINK = { perAvatarMs: 7000, minGapMs: 250 };
const FIDGET = { perAvatarMs: 60000, minGapMs: 2000 };
// How soon to look again while there is nothing to animate.
const IDLE_MS = 1000;
const SHUT_MS = 110;
const BETWEEN_TWO_BLINKS_MS = 140;
const TWO_BLINKS_CHANCE = 0.2;
// Each fidget with how often it comes up relative to the others. Its class is `fidget-<name>`.
const FIDGETS = [
  ["tilt-left", 2],
  ["tilt-right", 2],
  ["wiggle", 2],
  ["bounce", 2],
  ["giggle", 2],
  ["lean-left", 1],
  ["lean-right", 1],
  ["doze", 1],
  ["twirl", 1],
];
// Longer than any fidget, in case its animation never reports its end.
const FIDGET_GIVE_UP_MS = 4000;

const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)");
let started = false;

function still() {
  return document.hidden || reducedMotion.matches;
}

// A glad avatar already has its eyes closed in a smile, and shutting them too would draw both.
function isGlad(avatar) {
  const glad = avatar.querySelector(".av-glad");
  return glad !== null && getComputedStyle(glad).opacity !== "0";
}

function blink(avatar, times) {
  if (isGlad(avatar)) return;
  avatar.classList.add("blink");
  setTimeout(() => {
    avatar.classList.remove("blink");
    if (times > 1) setTimeout(() => blink(avatar, times - 1), BETWEEN_TWO_BLINKS_MS);
  }, SHUT_MS);
}

function pickFidget() {
  const total = FIDGETS.reduce((sum, [, weight]) => sum + weight, 0);
  let roll = Math.random() * total;
  for (const [name, weight] of FIDGETS) {
    roll -= weight;
    if (roll < 0) return name;
  }
  return FIDGETS[0][0];
}

function fidget(box) {
  if ([...box.classList].some((c) => c.startsWith("fidget-")) || isGlad(box)) return;
  const name = `fidget-${pickFidget()}`;
  let timer = null;
  const done = (event) => {
    if (event && (event.target !== box || !event.animationName.startsWith("av-fidget-"))) return;
    box.classList.remove(name);
    box.removeEventListener("animationend", done);
    clearTimeout(timer);
  };
  box.addEventListener("animationend", done);
  timer = setTimeout(done, FIDGET_GIVE_UP_MS);
  box.classList.add(name);
}

// Calls `act` on a random one of the elements `selector` finds, again and again, so each of them
// gets a turn about every `perAvatarMs`, jittered so the turns never settle into a rhythm.
function every(selector, { perAvatarMs, minGapMs }, act, quiet = () => false) {
  const tick = () => {
    const found = document.querySelectorAll(selector);
    if (found.length === 0) {
      setTimeout(tick, IDLE_MS);
      return;
    }
    if (!still() && !quiet()) act(found[Math.floor(Math.random() * found.length)]);
    const gap = Math.max(minGapMs, perAvatarMs / found.length);
    setTimeout(tick, gap * (0.4 + Math.random() * 1.2));
  };
  tick();
}

// Keeps the whiteboard's avatars blinking and fidgeting for the life of the page. Calling it again
// does nothing.
export function startAvatarLife() {
  if (started) return;
  started = true;
  every(".board-screen svg.avatar", BLINK, (avatar) =>
    blink(avatar, Math.random() < TWO_BLINKS_CHANCE ? 2 : 1),
  );
  // Nobody fidgets under a reveal or Time's up, where the wall is dimmed behind it.
  every(".wall .wall-avatar", FIDGET, fidget, () =>
    document.querySelector(".board-screen .overlay"),
  );
}
