// Chips change parent when a student switches group, so Dioxus remounts them, and a center keeps
// its DOM node while a different group card is patched into it. Either way the node on screen is
// not the one that moved, so chips are matched by their data-student id and cards by their
// data-group id instead (FLIP).

const FLIGHT_MS = 650;
const MAX_STAGGER_MS = 180;
const CARD_FLIGHT_MS = 800;
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

function flyChip({ el: chip, dx, dy, style }) {
  const background = style.backgroundColor;
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

// A card is already opaque, so it keeps its own background and shadow at both ends of the flight.
function flyCard({ el: card, dx, dy, style }) {
  const shadow = style.boxShadow;
  const distance = Math.hypot(dx, dy);
  const lift = Math.min(80, 24 + distance * 0.1);
  const spin = (Math.random() < 0.5 ? -1 : 1) * (1 + Math.random() * 2);
  const rest = "translate(0px, 0px) rotate(0deg) scale(1)";
  const edge = { boxShadow: shadow, zIndex: 10 };
  card.animate(
    [
      { ...edge, transform: `translate(${dx}px, ${dy}px) rotate(0deg) scale(1)` },
      {
        transform: `translate(${dx / 2}px, ${dy / 2 - lift}px) rotate(${spin}deg) scale(1.04)`,
        boxShadow: "0 18px 28px rgba(0, 0, 0, 0.3)",
        zIndex: 10,
        offset: 0.5,
      },
      { ...edge, transform: rest },
    ],
    {
      duration: CARD_FLIGHT_MS,
      easing: "cubic-bezier(.2, .8, .2, 1)",
      fill: "backwards",
    },
  );
}

const CHIPS = { key: "student", fly: flyChip };
const CARDS = { key: "group", fly: flyCard };

function elementsIn(root, { key }) {
  return [...root.querySelectorAll(`[data-${key}]`)];
}

function positionsIn(root, kind) {
  const positions = new Map();
  for (const el of elementsIn(root, kind)) {
    const { left, top } = el.getBoundingClientRect();
    positions.set(el.dataset[kind.key], { left, top });
  }
  return positions;
}

function flyMoved(root, kind, before) {
  const els = elementsIn(root, kind);
  // An element still mid-flight from an earlier change reports its transformed position; cancel
  // first so its layout position is what gets measured.
  for (const el of els) {
    for (const animation of el.getAnimations()) animation.cancel();
  }
  const moves = [];
  for (const el of els) {
    const from = before.get(el.dataset[kind.key]);
    if (!from) continue;
    const to = el.getBoundingClientRect();
    const dx = from.left - to.left;
    const dy = from.top - to.top;
    if (Math.abs(dx) >= 1 || Math.abs(dy) >= 1) {
      // Read every style before the first animate() call so the writes do not force a restyle each.
      const { backgroundColor, boxShadow } = getComputedStyle(el);
      moves.push({ el, dx, dy, style: { backgroundColor, boxShadow } });
    }
  }
  for (const move of moves) kind.fly(move);
}

// Records where every element of `kind` is now, then animates each one that moved once Dioxus
// has patched the DOM. Build keeps its groups in .cards and the whiteboard in .wall.
function animateMoves(kind) {
  if (matchMedia("(prefers-reduced-motion: reduce)").matches) return;
  const root = document.querySelector(".cards, .wall");
  if (!root) return;
  const before = positionsIn(root, kind);
  if (before.size === 0) return;

  stopWaiting();
  const observer = new MutationObserver(() => {
    stopWaiting();
    flyMoved(root, kind, before);
  });
  pending = { observer, timer: setTimeout(stopWaiting, GIVE_UP_MS) };
  // A rotation only relabels each center's card in place, which the attribute filter catches.
  observer.observe(root, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: [`data-${kind.key}`],
  });
}

// Call right before the state change that re-deals the chips.
export function animateShuffle() {
  animateMoves(CHIPS);
}

// Call right before the state change that rotates the groups to their next centers.
export function animateRotation() {
  animateMoves(CARDS);
}
