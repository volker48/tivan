// The phone remote's motion sensor. iOS asks the teacher before a page may read it, and only from
// a tap, so the page asks when the teacher taps "Turn on shake". Other browsers just send readings.

// "ask" when the browser wants a tap first, "granted" when readings arrive without asking, and
// "unsupported" when there is no sensor API at all.
export function motionAccess() {
  if (typeof DeviceMotionEvent === "undefined") return "unsupported";
  return typeof DeviceMotionEvent.requestPermission === "function" ? "ask" : "granted";
}

// Resolves to whether the teacher allowed it. Call from a tap.
export async function requestMotion() {
  try {
    return (await DeviceMotionEvent.requestPermission()) === "granted";
  } catch {
    return false;
  }
}

// Calls onMotion(at, x, y, z, withGravity) for every reading, preferring the one with gravity
// taken out. Returns a function that stops listening.
export function listenForMotion(onMotion) {
  const listener = (event) => {
    const clean = event.acceleration;
    const withGravity = !(clean && clean.x !== null);
    const reading = withGravity ? event.accelerationIncludingGravity : clean;
    if (!reading || reading.x === null) return;
    onMotion(event.timeStamp, reading.x, reading.y ?? 0, reading.z ?? 0, withGravity);
  };
  window.addEventListener("devicemotion", listener);
  return () => window.removeEventListener("devicemotion", listener);
}
