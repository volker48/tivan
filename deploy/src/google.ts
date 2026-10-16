import type { Config } from "./config.ts";
import { decodeBase64Json } from "./crypto.ts";
import type { Deps } from "./deps.ts";
import { isRecord, readJson } from "./json.ts";
import { errorName, logEvent } from "./log.ts";
import { type Fetched, type Teacher, toSub } from "./types.ts";

export const SCOPES = [
  "openid",
  "email",
  "profile",
  "https://www.googleapis.com/auth/classroom.courses.readonly",
  "https://www.googleapis.com/auth/classroom.rosters.readonly",
] as const;

/** `access_type=offline` with `prompt=consent` makes Google return a refresh token on every sign-in. */
export function authorizeUrl(config: Config, redirectUri: string, state: string): string {
  const url = new URL(config.authorizeUrl);
  const query = {
    client_id: config.clientId,
    redirect_uri: redirectUri,
    response_type: "code",
    scope: SCOPES.join(" "),
    access_type: "offline",
    prompt: "consent",
    state,
  };
  for (const [name, value] of Object.entries(query)) {
    url.searchParams.set(name, value);
  }
  return url.toString();
}

export type ExchangeFailure = "exchange" | "no_refresh_token" | "id_token";

export type ExchangeResult =
  | { readonly kind: "ok"; readonly teacher: Teacher; readonly refreshToken: string }
  | { readonly kind: "failed"; readonly code: ExchangeFailure };

async function postForm(
  deps: Deps,
  event: string,
  form: Record<string, string>,
): Promise<{ readonly status: number; readonly body: unknown } | null> {
  try {
    const response = await deps.fetch(deps.config.tokenUrl, {
      method: "POST",
      body: new URLSearchParams(form),
    });
    return { status: response.status, body: await readJson(response) };
  } catch (error) {
    logEvent(event, { reason: "network", error: errorName(error) });
    return null;
  }
}

function googleErrorCode(body: unknown): string {
  return isRecord(body) && typeof body["error"] === "string" ? body["error"] : "none";
}

export async function exchangeCode(
  deps: Deps,
  code: string,
  redirectUri: string,
): Promise<ExchangeResult> {
  const { clientId, clientSecret } = deps.config;
  const reply = await postForm(deps, "token_exchange_failed", {
    grant_type: "authorization_code",
    code,
    redirect_uri: redirectUri,
    client_id: clientId,
    client_secret: clientSecret,
  });
  if (reply === null) {
    return { kind: "failed", code: "exchange" };
  }
  if (reply.status !== 200 || !isRecord(reply.body)) {
    logEvent("token_exchange_failed", {
      status: reply.status,
      google: googleErrorCode(reply.body),
    });
    return { kind: "failed", code: "exchange" };
  }

  const refreshToken = reply.body["refresh_token"];
  if (typeof refreshToken !== "string" || refreshToken === "") {
    return { kind: "failed", code: "no_refresh_token" };
  }
  const idToken = reply.body["id_token"];
  const teacher = typeof idToken === "string" ? parseIdToken(idToken, clientId) : null;
  if (teacher === null) {
    return { kind: "failed", code: "id_token" };
  }
  return { kind: "ok", teacher, refreshToken };
}

/**
 * Reads the identity out of an ID token without checking its signature. The token arrived over
 * TLS straight from Google's token endpoint, which OpenID Connect accepts in place of a
 * signature check, so only the audience and the claims we use are verified.
 */
export function parseIdToken(idToken: string, clientId: string): Teacher | null {
  const claims = decodeBase64Json(idToken.split(".")[1] ?? "");
  if (!isRecord(claims)) {
    return null;
  }
  const audience: unknown = claims["aud"];
  if (!(Array.isArray(audience) ? audience.includes(clientId) : audience === clientId)) {
    return null;
  }
  const sub = typeof claims["sub"] === "string" ? toSub(claims["sub"]) : null;
  const email = claims["email"];
  if (sub === null || typeof email !== "string" || email === "") {
    return null;
  }
  const name = claims["name"];
  return { sub, email, name: typeof name === "string" && name !== "" ? name : email };
}

export async function refreshAccessToken(
  deps: Deps,
  refreshToken: string,
): Promise<Fetched<string>> {
  const reply = await postForm(deps, "token_refresh_failed", {
    grant_type: "refresh_token",
    refresh_token: refreshToken,
    client_id: deps.config.clientId,
    client_secret: deps.config.clientSecret,
  });
  if (reply === null) {
    return { kind: "unavailable" };
  }
  if (
    reply.status === 200 &&
    isRecord(reply.body) &&
    typeof reply.body["access_token"] === "string"
  ) {
    return { kind: "ok", value: reply.body["access_token"] };
  }
  if (googleErrorCode(reply.body) === "invalid_grant") {
    return { kind: "reauthorize" };
  }
  logEvent("token_refresh_failed", { status: reply.status, google: googleErrorCode(reply.body) });
  return { kind: "unavailable" };
}
