import { DurableObject } from "cloudflare:workers";
import {
  CLOSE_CODES,
  type Join,
  type Role,
  otherRole,
  parseFromBoard,
  parseFromRemote,
  parseJoin,
  presence,
} from "./remote-protocol.ts";
import { apiError } from "./http.ts";

/** A room nobody has opened for this long forgets its claims, so abandoned pairings do not pile up. */
const IDLE_MS = 30 * 24 * 60 * 60 * 1000;

const RETIRED = "retired";

function claimKey(role: Role): string {
  return `key:${role}`;
}

type Admission = "ok" | "claimed" | "unpaired";

/**
 * One pairing of a whiteboard and a phone. Sockets hibernate between messages, so a room that is
 * open all day but quiet costs nothing. The claims live in storage, so they survive eviction.
 */
export class RemoteRoom extends DurableObject<Env> {
  override async fetch(request: Request): Promise<Response> {
    const join = parseJoin(new URL(request.url));
    if (join === null) {
      return apiError(404, "not_found");
    }
    const admission = await this.admit(join);
    const { 0: client, 1: server } = new WebSocketPair();
    // A refused socket carries no role, so its close never reads as the seat emptying.
    this.ctx.acceptWebSocket(server, admission === "ok" ? [join.role] : []);
    if (admission === "ok") {
      this.seat(server, join.role);
      await this.ctx.storage.setAlarm(Date.now() + IDLE_MS);
    } else {
      server.close(CLOSE_CODES[admission], admission);
    }
    return new Response(null, { status: 101, webSocket: client });
  }

  override async webSocketMessage(ws: WebSocket, message: string | ArrayBuffer): Promise<void> {
    if (typeof message !== "string") {
      return;
    }
    const role = this.roleOf(ws);
    if (role === "remote") {
      const parsed = parseFromRemote(message);
      if (parsed !== null) {
        this.sendTo("board", JSON.stringify(parsed));
      }
    } else if (role === "board") {
      const parsed = parseFromBoard(message);
      if (parsed?.type === "unpair") {
        await this.unpair();
      } else if (parsed !== null) {
        this.sendTo("remote", JSON.stringify(parsed));
      }
    }
  }

  override async webSocketClose(ws: WebSocket): Promise<void> {
    this.left(ws);
  }

  override async webSocketError(ws: WebSocket): Promise<void> {
    this.left(ws);
  }

  override async alarm(): Promise<void> {
    if (this.ctx.getWebSockets().length > 0) {
      await this.ctx.storage.setAlarm(Date.now() + IDLE_MS);
    } else {
      await this.ctx.storage.deleteAll();
    }
  }

  /** The first key to arrive for a seat claims it; later ones must match. */
  private async admit(join: Join): Promise<Admission> {
    if ((await this.ctx.storage.get<boolean>(RETIRED)) === true) {
      return "unpaired";
    }
    const claimed = await this.ctx.storage.get<string>(claimKey(join.role));
    if (claimed === undefined) {
      await this.ctx.storage.put(claimKey(join.role), join.key);
      return "ok";
    }
    return claimed === join.key ? "ok" : "claimed";
  }

  /** The newest socket for a seat wins, so a reloaded page takes over from its stale self. */
  private seat(ws: WebSocket, role: Role): void {
    for (const stale of this.ctx.getWebSockets(role)) {
      if (stale !== ws) {
        stale.close(CLOSE_CODES.replaced, "replaced");
      }
    }
    ws.send(presence(this.open(otherRole(role)).length > 0));
    this.sendTo(otherRole(role), presence(true));
  }

  private left(ws: WebSocket): void {
    const role = this.roleOf(ws);
    if (role !== null && this.open(role).every((other) => other === ws)) {
      this.sendTo(otherRole(role), presence(false));
    }
  }

  /** Closes the room for good. Its code is never reused, because the board makes a new one. */
  private async unpair(): Promise<void> {
    await this.ctx.storage.deleteAll();
    await this.ctx.storage.put(RETIRED, true);
    for (const ws of this.ctx.getWebSockets()) {
      ws.close(CLOSE_CODES.unpaired, "unpaired");
    }
  }

  private roleOf(ws: WebSocket): Role | null {
    const [tag] = this.ctx.getTags(ws);
    return tag === "board" || tag === "remote" ? tag : null;
  }

  private open(role: Role): WebSocket[] {
    return this.ctx.getWebSockets(role).filter((ws) => ws.readyState === WebSocket.OPEN);
  }

  private sendTo(role: Role, text: string): void {
    for (const ws of this.open(role)) {
      ws.send(text);
    }
  }
}

/** Hands a WebSocket upgrade for `/api/remote/<room>` to that room. */
export async function connectRemote(
  request: Request,
  url: URL,
  rooms: DurableObjectNamespace,
): Promise<Response> {
  if (request.method !== "GET") {
    return apiError(405, "method_not_allowed", { allow: "GET" });
  }
  const join = parseJoin(url);
  if (join === null) {
    return apiError(404, "not_found");
  }
  if (request.headers.get("upgrade")?.toLowerCase() !== "websocket") {
    return apiError(426, "upgrade_required");
  }
  return rooms.getByName(join.room).fetch(request);
}
