import {
  constantTimeEqual,
  encryptToken,
  hashSessionToken,
  newSessionToken,
  parseSessionToken,
} from "./crypto.ts";
import type { Deps } from "./deps.ts";
import { type ExchangeFailure, exchangeCode } from "./google.ts";
import { parseCookies } from "./http.ts";
import { deleteSession, findSession, insertSession, upsertTeacher } from "./store.ts";
import type { SessionToken, Teacher } from "./types.ts";

export const SESSION_COOKIE = "tivan_session";
export const STATE_COOKIE = "tivan_oauth_state";
export const SESSION_TTL_MS = 30 * 24 * 60 * 60 * 1000;
export const STATE_TTL_SECONDS = 10 * 60;

export type Authn =
  | { readonly kind: "anonymous" }
  | { readonly kind: "teacher"; readonly teacher: Teacher };

function sessionTokenFrom(cookieHeader: string | null): SessionToken | null {
  const value = parseCookies(cookieHeader).get(SESSION_COOKIE);
  return value === undefined ? null : parseSessionToken(value);
}

export async function authenticate(deps: Deps, cookieHeader: string | null): Promise<Authn> {
  const token = sessionTokenFrom(cookieHeader);
  if (token === null) {
    return { kind: "anonymous" };
  }
  const tokenHash = await hashSessionToken(token);
  const session = await findSession(deps.sql, tokenHash);
  if (session === null) {
    return { kind: "anonymous" };
  }
  if (session.expiresAt <= deps.now()) {
    await deleteSession(deps.sql, tokenHash);
    return { kind: "anonymous" };
  }
  return { kind: "teacher", teacher: session.teacher };
}

export async function endSession(deps: Deps, cookieHeader: string | null): Promise<void> {
  const token = sessionTokenFrom(cookieHeader);
  if (token !== null) {
    await deleteSession(deps.sql, await hashSessionToken(token));
  }
}

/** What Google put on the callback URL, parsed once at the boundary. */
export type CallbackParams =
  | { readonly kind: "denied" }
  | { readonly kind: "authorized"; readonly code: string; readonly state: string }
  | { readonly kind: "malformed" };

export function parseCallbackParams(query: URLSearchParams): CallbackParams {
  if (query.has("error")) {
    return { kind: "denied" };
  }
  const code = query.get("code");
  const state = query.get("state");
  return code && state ? { kind: "authorized", code, state } : { kind: "malformed" };
}

export type AuthErrorCode = ExchangeFailure | "denied" | "state" | "bad_request" | "server_error";

export type LoginOutcome =
  | { readonly kind: "signed-in"; readonly token: SessionToken }
  | { readonly kind: "failed"; readonly code: AuthErrorCode };

export async function completeLogin(
  deps: Deps,
  params: CallbackParams,
  cookieState: string | null,
  redirectUri: string,
): Promise<LoginOutcome> {
  if (params.kind === "denied") {
    return { kind: "failed", code: "denied" };
  }
  if (params.kind === "malformed") {
    return { kind: "failed", code: "bad_request" };
  }
  if (cookieState === null || !(await constantTimeEqual(cookieState, params.state))) {
    return { kind: "failed", code: "state" };
  }

  const exchange = await exchangeCode(deps, params.code, redirectUri);
  if (exchange.kind === "failed") {
    return exchange;
  }

  const now = deps.now();
  const sealed = await encryptToken(deps.config.tokenKey, exchange.refreshToken);
  await upsertTeacher(deps.sql, exchange.teacher, sealed, now);
  const token = newSessionToken();
  await insertSession(
    deps.sql,
    await hashSessionToken(token),
    exchange.teacher.sub,
    now + SESSION_TTL_MS,
    now,
  );
  return { kind: "signed-in", token };
}
