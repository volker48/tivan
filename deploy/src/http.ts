export type ApiError =
  | "unauthorized"
  | "invalid_doc"
  | "too_large"
  | "reauthorize"
  | "classroom_unavailable"
  | "not_found"
  | "method_not_allowed"
  | "misconfigured"
  | "internal";

const NO_STORE = { "cache-control": "no-store" } as const;

export function json(body: unknown, status = 200): Response {
  return Response.json(body, { status, headers: NO_STORE });
}

export function apiError(
  status: number,
  code: ApiError,
  headers: Record<string, string> = {},
): Response {
  return Response.json({ error: code }, { status, headers: { ...NO_STORE, ...headers } });
}

export function noContent(cookies: readonly string[] = []): Response {
  return withCookies(new Response(null, { status: 204, headers: NO_STORE }), cookies);
}

export function redirect(location: string, cookies: readonly string[] = []): Response {
  const headers = new Headers({ ...NO_STORE, location });
  return withCookies(new Response(null, { status: 302, headers }), cookies);
}

function withCookies(response: Response, cookies: readonly string[]): Response {
  for (const cookie of cookies) {
    response.headers.append("set-cookie", cookie);
  }
  return response;
}

export const LOOPBACK_HOSTS = new Set(["localhost", "127.0.0.1", "[::1]"]);

export interface CookieOptions {
  readonly path: string;
  readonly maxAgeSeconds: number;
}

/** HttpOnly and SameSite=Lax always. Secure everywhere except plain-http local development. */
export function serializeCookie(
  name: string,
  value: string,
  url: URL,
  options: CookieOptions,
): string {
  const parts = [
    `${name}=${value}`,
    `Path=${options.path}`,
    `Max-Age=${options.maxAgeSeconds}`,
    "HttpOnly",
    "SameSite=Lax",
  ];
  if (!LOOPBACK_HOSTS.has(url.hostname)) {
    parts.push("Secure");
  }
  return parts.join("; ");
}

export function clearCookie(name: string, url: URL, path: string): string {
  return serializeCookie(name, "", url, { path, maxAgeSeconds: 0 });
}

export function parseCookies(header: string | null): ReadonlyMap<string, string> {
  const cookies = new Map<string, string>();
  for (const pair of (header ?? "").split(";")) {
    const separator = pair.indexOf("=");
    if (separator > 0) {
      const name = pair.slice(0, separator).trim();
      if (!cookies.has(name)) {
        cookies.set(name, pair.slice(separator + 1).trim());
      }
    }
  }
  return cookies;
}
