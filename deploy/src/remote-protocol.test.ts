import { describe, expect, it } from "vitest";
import {
  MAX_MESSAGE_CHARS,
  parseFromBoard,
  parseFromRemote,
  parseJoin,
  presence,
} from "./remote-protocol.ts";

const ROOM = "AbCdEfGhIjKlMnOpQrSt_-";
const KEY = "0123456789abcdefghij-_";

function join(path: string): ReturnType<typeof parseJoin> {
  return parseJoin(new URL(`https://tivan.test${path}`));
}

describe("joining a room", () => {
  it("reads the room from the path and the seat and key from the query", () => {
    expect(join(`/api/remote/${ROOM}?role=board&key=${KEY}`)).toEqual({
      room: ROOM,
      role: "board",
      key: KEY,
    });
    expect(join(`/api/remote/${ROOM}?role=remote&key=${KEY}`)?.role).toBe("remote");
  });

  it.each([
    ["an unknown seat", `/api/remote/${ROOM}?role=teacher&key=${KEY}`],
    ["no seat", `/api/remote/${ROOM}?key=${KEY}`],
    ["no key", `/api/remote/${ROOM}?role=board`],
    ["a short key", `/api/remote/${ROOM}?role=board&key=abc`],
    ["a short room", `/api/remote/abc?role=board&key=${KEY}`],
    ["a room with a slash", `/api/remote/${ROOM}/x?role=board&key=${KEY}`],
    ["a room outside base64url", `/api/remote/${ROOM.slice(1)}+?role=board&key=${KEY}`],
    ["another path", `/api/doc?role=board&key=${KEY}`],
  ])("refuses %s", (_name, path) => {
    expect(join(path)).toBeNull();
  });
});

describe("messages from the phone", () => {
  it.each(["shuffle", "oracle", "group", "rotate"])("forwards the %s command", (command) => {
    expect(parseFromRemote(JSON.stringify({ type: "command", command }))).toEqual({
      type: "command",
      command,
    });
  });

  it("drops extra fields, so the board only ever sees the command", () => {
    const text = JSON.stringify({ type: "command", command: "shuffle", name: "Zander" });
    expect(parseFromRemote(text)).toEqual({ type: "command", command: "shuffle" });
  });

  it.each([
    ["an unknown command", '{"type":"command","command":"delete"}'],
    ["a board message", '{"type":"status","students":true,"grouped":true,"centers":false}'],
    ["an unpair", '{"type":"unpair"}'],
    ["an array", "[]"],
    ["null", "null"],
    ["text that is not JSON", "shuffle"],
  ])("ignores %s", (_name, text) => {
    expect(parseFromRemote(text)).toBeNull();
  });

  it("ignores anything longer than the cap without reading it", () => {
    const padded = `{"type":"command","command":"shuffle","pad":"${"x".repeat(MAX_MESSAGE_CHARS)}"}`;
    expect(parseFromRemote(padded)).toBeNull();
  });
});

describe("messages from the board", () => {
  it("forwards a status of three flags and nothing else", () => {
    const text = JSON.stringify({
      type: "status",
      students: true,
      grouped: false,
      centers: false,
      roster: ["Zander"],
    });
    expect(parseFromBoard(text)).toEqual({
      type: "status",
      students: true,
      grouped: false,
      centers: false,
    });
  });

  it("forwards done for a known command, and unpair", () => {
    expect(parseFromBoard('{"type":"done","command":"rotate"}')).toEqual({
      type: "done",
      command: "rotate",
    });
    expect(parseFromBoard('{"type":"unpair"}')).toEqual({ type: "unpair" });
  });

  it.each([
    ["a status missing a flag", '{"type":"status","students":true,"grouped":true}'],
    ["a status with a string flag", '{"type":"status","students":"yes","grouped":1,"centers":0}'],
    ["done for an unknown command", '{"type":"done","command":"explode"}'],
    ["a phone command", '{"type":"command","command":"shuffle"}'],
    ["text that is not JSON", "{"],
  ])("ignores %s", (_name, text) => {
    expect(parseFromBoard(text)).toBeNull();
  });
});

describe("presence", () => {
  it("says whether the other seat is taken", () => {
    expect(JSON.parse(presence(true))).toEqual({ type: "presence", peer: true });
    expect(JSON.parse(presence(false))).toEqual({ type: "presence", peer: false });
  });
});
