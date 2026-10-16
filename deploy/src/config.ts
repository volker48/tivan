import { parseEncryptionKey } from "./crypto.ts";
import type { EncryptionKey } from "./types.ts";

export interface Config {
  readonly clientId: string;
  readonly clientSecret: string;
  readonly tokenKey: EncryptionKey;
  readonly authorizeUrl: URL;
  readonly tokenUrl: URL;
  readonly classroomUrl: URL;
}

/** The slice of the Worker `Env` this module reads. Values are untrusted until parsed. */
export interface RawEnv {
  readonly GOOGLE_CLIENT_ID?: unknown;
  readonly GOOGLE_CLIENT_SECRET?: unknown;
  readonly TOKEN_ENC_KEY?: unknown;
  readonly GOOGLE_AUTHORIZE_URL?: unknown;
  readonly GOOGLE_TOKEN_URL?: unknown;
  readonly CLASSROOM_API_URL?: unknown;
}

export type ConfigResult =
  | { readonly kind: "ok"; readonly config: Config }
  | { readonly kind: "invalid"; readonly problems: readonly string[] };

function textOf(value: unknown): string {
  return typeof value === "string" ? value.trim() : "";
}

const LOOPBACK_HOSTS = new Set(["localhost", "127.0.0.1", "[::1]"]);

/** The client secret and refresh tokens travel to these URLs, so plain HTTP is for loopback only. */
function endpointUrl(value: string): URL | null {
  try {
    const url = new URL(value);
    const secure = url.protocol === "https:";
    return secure || (url.protocol === "http:" && LOOPBACK_HOSTS.has(url.hostname)) ? url : null;
  } catch {
    return null;
  }
}

/** Problems name the variable and never echo its value, so they are safe to log. */
export function parseConfig(env: RawEnv): ConfigResult {
  const clientId = textOf(env.GOOGLE_CLIENT_ID);
  const clientSecret = textOf(env.GOOGLE_CLIENT_SECRET);
  const tokenKey = parseEncryptionKey(textOf(env.TOKEN_ENC_KEY));
  const authorizeUrl = endpointUrl(textOf(env.GOOGLE_AUTHORIZE_URL));
  const tokenUrl = endpointUrl(textOf(env.GOOGLE_TOKEN_URL));
  const classroomUrl = endpointUrl(textOf(env.CLASSROOM_API_URL));

  if (
    clientId === "" ||
    clientSecret === "" ||
    tokenKey === null ||
    authorizeUrl === null ||
    tokenUrl === null ||
    classroomUrl === null
  ) {
    const problems = [
      clientId === "" && "GOOGLE_CLIENT_ID is missing",
      clientSecret === "" && "GOOGLE_CLIENT_SECRET is missing",
      tokenKey === null && "TOKEN_ENC_KEY is missing or not base64 of 32 bytes",
      authorizeUrl === null &&
        "GOOGLE_AUTHORIZE_URL is missing, or not an https URL (http only for localhost)",
      tokenUrl === null &&
        "GOOGLE_TOKEN_URL is missing, or not an https URL (http only for localhost)",
      classroomUrl === null &&
        "CLASSROOM_API_URL is missing, or not an https URL (http only for localhost)",
    ].filter((problem) => problem !== false);
    return { kind: "invalid", problems };
  }
  return {
    kind: "ok",
    config: { clientId, clientSecret, tokenKey, authorizeUrl, tokenUrl, classroomUrl },
  };
}
