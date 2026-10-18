/**
 * The phone remote's wire format. A board (the whiteboard page) and a remote (the teacher's phone)
 * meet in one room named by a random code the board made. Each side proves it is the device that
 * first claimed its seat with a random key. The relay forwards only the messages below, re-encoded,
 * so it never carries anything a peer did not mean to send, and never a student's name.
 */

export type Role = "board" | "remote";

export const COMMANDS = ["shuffle", "oracle", "group", "rotate"] as const;
export type Command = (typeof COMMANDS)[number];

/** Room codes and device keys are 16 random bytes in unpadded base64url. */
const TOKEN = /^[A-Za-z0-9_-]{22}$/;

export const REMOTE_PREFIX = "/api/remote/";

/** Longer than any message below. Anything bigger is dropped unread. */
export const MAX_MESSAGE_CHARS = 512;

/**
 * Why the relay closed a socket. The page stops reconnecting on any of them, because trying again
 * cannot help: another device holds the seat, a newer tab took it, or the board turned the room off.
 */
export const CLOSE_CODES = {
  claimed: 4001,
  replaced: 4002,
  unpaired: 4003,
} as const;

export interface Join {
  readonly room: string;
  readonly role: Role;
  readonly key: string;
}

/** `/api/remote/<room>?role=board|remote&key=<key>`, or null for anything else. */
export function parseJoin(url: URL): Join | null {
  if (!url.pathname.startsWith(REMOTE_PREFIX)) {
    return null;
  }
  const room = url.pathname.slice(REMOTE_PREFIX.length);
  const role = url.searchParams.get("role");
  const key = url.searchParams.get("key") ?? "";
  if (!TOKEN.test(room) || !TOKEN.test(key) || (role !== "board" && role !== "remote")) {
    return null;
  }
  return { room, role, key };
}

export function otherRole(role: Role): Role {
  return role === "board" ? "remote" : "board";
}

export type FromRemote = { readonly type: "command"; readonly command: Command };

export type FromBoard =
  | {
      readonly type: "status";
      readonly students: boolean;
      readonly grouped: boolean;
      readonly centers: boolean;
    }
  | { readonly type: "done"; readonly command: Command }
  | { readonly type: "unpair" };

function parseObject(text: string): Record<string, unknown> | null {
  if (text.length > MAX_MESSAGE_CHARS) {
    return null;
  }
  try {
    const value: unknown = JSON.parse(text);
    return typeof value === "object" && value !== null && !Array.isArray(value)
      ? (value as Record<string, unknown>)
      : null;
  } catch {
    return null;
  }
}

function isCommand(value: unknown): value is Command {
  return COMMANDS.some((command) => command === value);
}

export function parseFromRemote(text: string): FromRemote | null {
  const message = parseObject(text);
  if (message?.type === "command" && isCommand(message.command)) {
    return { type: "command", command: message.command };
  }
  return null;
}

export function parseFromBoard(text: string): FromBoard | null {
  const message = parseObject(text);
  switch (message?.type) {
    case "status": {
      const { students, grouped, centers } = message;
      if (
        typeof students === "boolean" &&
        typeof grouped === "boolean" &&
        typeof centers === "boolean"
      ) {
        return { type: "status", students, grouped, centers };
      }
      return null;
    }
    case "done":
      return isCommand(message.command) ? { type: "done", command: message.command } : null;
    case "unpair":
      return { type: "unpair" };
    default:
      return null;
  }
}

/** Sent by the relay itself: whether the other seat has a device on it right now. */
export function presence(peer: boolean): string {
  return JSON.stringify({ type: "presence", peer });
}
