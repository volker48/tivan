// Avatars on the whiteboard blink one at a time, at random, so a wall of them looks alive. A blink
// shuts an avatar's eyes for a moment with the class the stylesheet swaps its eye layers on.
// Nothing runs between blinks. A CSS animation on every avatar would restyle and repaint all of
// them every frame for as long as the wall is up. Build's avatars are too small for a blink to
// show, so they are left alone.

// About how long one avatar goes between blinks, and the shortest gap between any two, so a wall
// full of avatars does not blink faster than a few times a second.
const GAP_PER_AVATAR_MS = 7000;
const MIN_GAP_MS = 250;
// How soon to look again while no whiteboard is showing.
const IDLE_MS = 1000;
const SHUT_MS = 110;
const BETWEEN_TWO_BLINKS_MS = 140;
const TWO_BLINKS_CHANCE = 0.2;

const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)");
let started = false;

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

function tick() {
  const avatars = document.querySelectorAll(".board-screen svg.avatar");
  if (avatars.length > 0 && !document.hidden && !reducedMotion.matches) {
    const avatar = avatars[Math.floor(Math.random() * avatars.length)];
    blink(avatar, Math.random() < TWO_BLINKS_CHANCE ? 2 : 1);
  }
  if (avatars.length === 0) {
    setTimeout(tick, IDLE_MS);
    return;
  }
  const gap = Math.max(MIN_GAP_MS, GAP_PER_AVATAR_MS / avatars.length);
  // Jittered so the blinks never settle into a rhythm.
  setTimeout(tick, gap * (0.4 + Math.random() * 1.2));
}

// Keeps the whiteboard's avatars blinking for the life of the page. Calling it again does nothing.
export function startBlinking() {
  if (started) return;
  started = true;
  tick();
}
