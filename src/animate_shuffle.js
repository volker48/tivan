// Chips change parent when a student switches group, so Dioxus unmounts and remounts them and
// no DOM node survives the shuffle. Chips are matched by their data-student id instead (FLIP).

const FLIGHT_MS = 650;
const MAX_STAGGER_MS = 180;
// The re-render lands within a frame or two of the click. If nothing mutates by then there is
// nothing to animate.
const GIVE_UP_MS = 500;

let pending = null;

function stopWaiting() {
  if (!pending) return;
  pending.observer.disconnect();
  clearTimeout(pending.timer);
  pending = null;
}

function chipsIn(root) {
  return [...root.querySelectorAll("[data-student]")];
}

function chipPositions(root) {
  const positions = new Map();
  for (const chip of chipsIn(root)) {
    const { left, top } = chip.getBoundingClientRect();
    positions.set(chip.dataset.student, { left, top });
  }
  return positions;
}

function fly({ chip, dx, dy, background }) {
  const distance = Math.hypot(dx, dy);
  const lift = Math.min(140, 30 + distance * 0.18);
  const spin = (Math.random() < 0.5 ? -1 : 1) * (8 + Math.random() * 14);
  const rest = "translate(0px, 0px) rotate(0deg) scale(1)";
  const edge = { boxShadow: "0 0 0 rgba(0, 0, 0, 0)", backgroundColor: background, zIndex: 10 };
  chip.animate(
    [
      { ...edge, transform: `translate(${dx}px, ${dy}px) rotate(0deg) scale(1)` },
      {
        transform: `translate(${dx / 2}px, ${dy / 2 - lift}px) rotate(${spin}deg) scale(1.2)`,
        boxShadow: "0 14px 20px rgba(0, 0, 0, 0.3)",
        // Opaque so a chip crossing another group's text hides it instead of overprinting it.
        backgroundColor: "rgb(255, 255, 255)",
        zIndex: 10,
        offset: 0.5,
      },
      { ...edge, transform: rest },
    ],
    {
      duration: FLIGHT_MS,
      delay: Math.random() * MAX_STAGGER_MS,
      easing: "cubic-bezier(.2, .8, .2, 1)",
      fill: "backwards",
    },
  );
}

function flyMovedChips(root, before) {
  const chips = chipsIn(root);
  // A chip still mid-flight from an earlier shuffle reports its transformed position; cancel
  // first so its layout position is what gets measured.
  for (const chip of chips) {
    for (const animation of chip.getAnimations()) animation.cancel();
  }
  const moves = [];
  for (const chip of chips) {
    const from = before.get(chip.dataset.student);
    if (!from) continue;
    const to = chip.getBoundingClientRect();
    const dx = from.left - to.left;
    const dy = from.top - to.top;
    if (Math.abs(dx) >= 1 || Math.abs(dy) >= 1) {
      moves.push({ chip, dx, dy, background: getComputedStyle(chip).backgroundColor });
    }
  }
  for (const move of moves) fly(move);
}

// Call right before the state change that re-deals the chips. Records where every chip is now,
// then animates each chip that moved once Dioxus has patched the DOM.
export function animateShuffle() {
  if (matchMedia("(prefers-reduced-motion: reduce)").matches) return;
  const cards = document.querySelector(".cards");
  if (!cards) return;
  const before = chipPositions(cards);
  if (before.size === 0) return;

  stopWaiting();
  const observer = new MutationObserver(() => {
    stopWaiting();
    flyMovedChips(cards, before);
  });
  pending = { observer, timer: setTimeout(stopWaiting, GIVE_UP_MS) };
  observer.observe(cards, { childList: true, subtree: true });
}
