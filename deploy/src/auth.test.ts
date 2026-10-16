import { afterEach, describe, expect, it, vi } from "vitest";
import { FAKE_REFRESH_TOKEN, FAKE_TEACHER } from "../dev/fake-google-core.ts";
import { SESSION_COOKIE, SESSION_TTL_MS, STATE_COOKIE } from "./auth.ts";
import { decryptToken, parseEncryptionKey } from "./crypto.ts";
import { parseIdToken } from "./google.ts";
import { ENVELOPE, TEST_CLIENT_ID, TEST_KEY, TEST_ORIGIN, createTestApp } from "./testing/app.ts";

afterEach(() => {
  vi.restoreAllMocks();
});

function setCookies(response: Response): string[] {
  return response.headers.getSetCookie();
}

function cookieNamed(response: Response, name: string): string {
  const cookie = setCookies(response).find((candidate) => candidate.startsWith(`${name}=`));
  if (cookie === undefined) {
    throw new Error(`no ${name} cookie was set`);
  }
  return cookie;
}

describe("GET /auth/login", () => {
  it("sets a short-lived state cookie and redirects to Google with the contract's parameters", async () => {
    const app = createTestApp();
    const response = await app.request("/auth/login");

    expect(response.status).toBe(302);
    const location = new URL(response.headers.get("location") ?? "");
    expect(`${location.origin}${location.pathname}`).toBe("https://google.test/authorize");
    expect(location.searchParams.get("client_id")).toBe(TEST_CLIENT_ID);
    expect(location.searchParams.get("redirect_uri")).toBe(`${TEST_ORIGIN}/auth/callback`);
    expect(location.searchParams.get("response_type")).toBe("code");
    expect(location.searchParams.get("access_type")).toBe("offline");
    expect(location.searchParams.get("prompt")).toBe("consent");
    expect(location.searchParams.get("scope")?.split(" ")).toEqual([
      "openid",
      "email",
      "profile",
      "https://www.googleapis.com/auth/classroom.courses.readonly",
      "https://www.googleapis.com/auth/classroom.rosters.readonly",
    ]);

    const state = location.searchParams.get("state");
    const cookie = cookieNamed(response, STATE_COOKIE);
    expect(state).toMatch(/^[A-Za-z0-9_-]{43}$/);
    expect(cookie).toContain(`${STATE_COOKIE}=${state}`);
    expect(cookie).toContain("HttpOnly");
    expect(cookie).toContain("SameSite=Lax");
    expect(cookie).toContain("Path=/auth");
    expect(cookie).toContain("Max-Age=600");
    expect(cookie).toContain("Secure");
  });

  it("issues a different state every time", async () => {
    const app = createTestApp();
    const first = (await app.request("/auth/login")).headers.get("location");
    const second = (await app.request("/auth/login")).headers.get("location");
    expect(first).not.toBe(second);
  });

  it("drops Secure on localhost so the cookie works over plain http", async () => {
    const app = createTestApp();
    const response = await app.request("/auth/login", { origin: "http://localhost:8787" });
    expect(cookieNamed(response, STATE_COOKIE)).not.toContain("Secure");
    const redirectUri = new URL(response.headers.get("location") ?? "").searchParams.get(
      "redirect_uri",
    );
    expect(redirectUri).toBe("http://localhost:8787/auth/callback");
  });
});

describe("GET /auth/callback", () => {
  it("signs the teacher in, sets the session cookie and lands on /", async () => {
    const app = createTestApp();
    const response = await app.signIn();

    expect(response.status).toBe(302);
    expect(response.headers.get("location")).toBe("/");
    const session = cookieNamed(response, SESSION_COOKIE);
    expect(session).toMatch(new RegExp(`^${SESSION_COOKIE}=[A-Za-z0-9_-]{43};`));
    expect(session).toContain("HttpOnly");
    expect(session).toContain("Secure");
    expect(session).toContain("SameSite=Lax");
    expect(session).toContain("Path=/");
    expect(session).toContain(`Max-Age=${SESSION_TTL_MS / 1000}`);
    expect(cookieNamed(response, STATE_COOKIE)).toContain("Max-Age=0");

    const me = await app.request("/api/me");
    expect(me.status).toBe(200);
    expect(await me.json()).toEqual({ email: FAKE_TEACHER.email, name: FAKE_TEACHER.name });
  });

  it("stores only the SHA-256 of the session token and the refresh token encrypted", async () => {
    const app = createTestApp();
    const response = await app.signIn();
    const token = cookieNamed(response, SESSION_COOKIE).split(";")[0]?.split("=")[1] ?? "";

    const sessions = app.db.prepare("SELECT token_hash, sub, expires_at FROM sessions").all();
    expect(sessions).toHaveLength(1);
    expect(sessions[0]?.["token_hash"]).toMatch(/^[0-9a-f]{64}$/);
    expect(JSON.stringify(sessions)).not.toContain(token);
    expect(sessions[0]?.["sub"]).toBe(FAKE_TEACHER.sub);
    expect(sessions[0]?.["expires_at"]).toBe(app.clock.now + SESSION_TTL_MS);

    const teacher = app.db.prepare("SELECT * FROM teachers").get();
    expect(teacher?.["email"]).toBe(FAKE_TEACHER.email);
    const sealed = String(teacher?.["refresh_token_enc"]);
    expect(sealed).not.toContain(FAKE_REFRESH_TOKEN);
    const key = parseEncryptionKey(TEST_KEY);
    expect(key && (await decryptToken(key, sealed))).toBe(FAKE_REFRESH_TOKEN);
  });

  it.each<[string, (state: string) => { state: string; cookie?: string }, boolean]>([
    ["a forged state", () => ({ state: "forged" }), true],
    ["a callback with no state cookie", (state) => ({ state, cookie: "unrelated=1" }), false],
    [
      "a state one character off",
      (state) => ({ state: `${state.slice(0, -1)}${state.endsWith("A") ? "B" : "A"}` }),
      false,
    ],
  ])("rejects %s, before talking to Google", async (_label, forge, leavesNoTrace) => {
    const app = createTestApp();
    const { state, cookie } = forge(await app.startLogin());

    const response = await app.request(
      `/auth/callback?code=fake-code-1&state=${state}`,
      cookie === undefined ? {} : { headers: { cookie } },
    );

    expect(response.headers.get("location")).toBe("/?auth_error=state");
    expect(app.google.requests).toEqual([]);
    if (leavesNoTrace) {
      expect(response.status).toBe(302);
      expect(
        setCookies(response).some((candidate) => candidate.startsWith(`${SESSION_COOKIE}=`)),
      ).toBe(false);
      expect(cookieNamed(response, STATE_COOKIE)).toContain("Max-Age=0");
      expect(app.db.prepare("SELECT 1 FROM teachers").all()).toEqual([]);
      expect((await app.request("/api/me")).status).toBe(401);
    }
  });

  it("reports a denied consent screen", async () => {
    const app = createTestApp();
    await app.request("/auth/login");
    const response = await app.request("/auth/callback?error=access_denied&state=x");
    expect(response.headers.get("location")).toBe("/?auth_error=denied");
  });

  it("reports a callback with no code", async () => {
    const app = createTestApp();
    await app.request("/auth/login");
    const response = await app.request("/auth/callback");
    expect(response.headers.get("location")).toBe("/?auth_error=bad_request");
  });

  it("reports a failed code exchange without leaking anything", async () => {
    const app = createTestApp();
    const state = await app.startLogin();
    vi.spyOn(console, "error").mockImplementation(() => {});

    const response = await app.request(`/auth/callback?code=never-issued&state=${state}`);

    expect(response.headers.get("location")).toBe("/?auth_error=exchange");
    expect(app.db.prepare("SELECT 1 FROM sessions").all()).toEqual([]);
  });

  it("reports an unreachable Google as an exchange failure", async () => {
    const app = createTestApp({
      wrapFetch: () => () => Promise.reject(new TypeError("network down")),
    });
    vi.spyOn(console, "error").mockImplementation(() => {});
    const state = await app.startLogin();
    const response = await app.request(`/auth/callback?code=fake-code-1&state=${state}`);
    expect(response.headers.get("location")).toBe("/?auth_error=exchange");
  });

  it("reports an ID token for another client", async () => {
    const app = createTestApp({
      wrapFetch: (inner) => async (input, init) => {
        const response = await inner(input, init);
        if (!new Request(input, init).url.endsWith("/token")) {
          return response;
        }
        const body = (await response.json()) as Record<string, string>;
        const [header = "", payload = "", signature = ""] = (body["id_token"] ?? "").split(".");
        const claims = JSON.parse(atob(payload.replaceAll("-", "+").replaceAll("_", "/")));
        const forged = btoa(JSON.stringify({ ...claims, aud: "someone-else" }));
        return Response.json({ ...body, id_token: [header, forged, signature].join(".") });
      },
    });
    const response = await app.signIn();
    expect(response.headers.get("location")).toBe("/?auth_error=id_token");
    expect(app.db.prepare("SELECT 1 FROM teachers").all()).toEqual([]);
  });

  it("reports a token response with no refresh token", async () => {
    const app = createTestApp({
      wrapFetch: (inner) => async (input, init) => {
        const response = await inner(input, init);
        if (!new Request(input, init).url.endsWith("/token")) {
          return response;
        }
        const { refresh_token: _dropped, ...rest } = (await response.json()) as Record<
          string,
          string
        >;
        return Response.json(rest);
      },
    });
    const response = await app.signIn();
    expect(response.headers.get("location")).toBe("/?auth_error=no_refresh_token");
    expect(app.db.prepare("SELECT 1 FROM teachers").all()).toEqual([]);
  });

  it("turns a database failure into an auth_error redirect, not a 500", async () => {
    const app = createTestApp();
    app.db.exec("DROP TABLE sessions");
    vi.spyOn(console, "error").mockImplementation(() => {});
    const response = await app.signIn();
    expect(response.status).toBe(302);
    expect(response.headers.get("location")).toBe("/?auth_error=server_error");
  });

  it("never logs tokens, the code or the cookie when a sign-in fails", async () => {
    const app = createTestApp();
    const logged = vi.spyOn(console, "error").mockImplementation(() => {});
    const state = await app.startLogin();
    await app.request(`/auth/callback?code=secret-code&state=${state}`);
    const output = JSON.stringify(logged.mock.calls);
    expect(output).not.toContain("secret-code");
    expect(output).not.toContain(state);
    expect(output).not.toContain(FAKE_REFRESH_TOKEN);
  });

  it("keeps the stored doc when the teacher signs in again", async () => {
    const app = createTestApp();
    await app.signIn();
    const envelope = '{"version":5,"doc":{"classes":[]}}';
    expect((await app.request("/api/doc", { method: "PUT", body: envelope })).status).toBe(204);

    await app.signIn();

    const doc = await app.request("/api/doc");
    expect(await doc.text()).toBe(envelope);
    expect(app.db.prepare("SELECT 1 FROM teachers").all()).toHaveLength(1);
  });
});

describe("sessions", () => {
  it("expire 30 days after sign-in and are removed when they are found expired", async () => {
    const app = createTestApp();
    await app.signIn();

    app.clock.advance(SESSION_TTL_MS - 1);
    expect((await app.request("/api/me")).status).toBe(200);

    app.clock.advance(1);
    const expired = await app.request("/api/me");
    expect(expired.status).toBe(401);
    expect(await expired.json()).toEqual({ error: "unauthorized" });
    expect(app.db.prepare("SELECT 1 FROM sessions").all()).toEqual([]);
  });

  it("do not authenticate with a forged or malformed cookie", async () => {
    const app = createTestApp();
    await app.signIn();
    for (const cookie of [
      `${SESSION_COOKIE}=${"A".repeat(43)}`,
      `${SESSION_COOKIE}=garbage`,
      `${SESSION_COOKIE}=`,
      "tivan_session",
    ]) {
      const response = await app.request("/api/me", { headers: { cookie } });
      expect(response.status).toBe(401);
    }
  });
});

describe("POST /auth/logout", () => {
  it("deletes the session row, clears the cookie and answers 204", async () => {
    const app = createTestApp();
    await app.signIn();

    const response = await app.request("/auth/logout", { method: "POST" });

    expect(response.status).toBe(204);
    expect(cookieNamed(response, SESSION_COOKIE)).toContain("Max-Age=0");
    expect(app.db.prepare("SELECT 1 FROM sessions").all()).toEqual([]);
    expect((await app.request("/api/me")).status).toBe(401);
  });

  it("answers 204 when nobody is signed in", async () => {
    const app = createTestApp();
    expect((await app.request("/auth/logout", { method: "POST" })).status).toBe(204);
  });

  it("keeps the teacher row and doc so signing in later returns the same setup", async () => {
    const app = createTestApp();
    await app.signIn();
    await app.request("/api/doc", { method: "PUT", body: ENVELOPE });
    await app.request("/auth/logout", { method: "POST" });
    await app.signIn();
    expect(await (await app.request("/api/doc")).text()).toBe(ENVELOPE);
  });
});

describe("parseIdToken", () => {
  const NOW = Date.UTC(2026, 9, 16);
  const valid = {
    iss: "https://accounts.google.com",
    aud: TEST_CLIENT_ID,
    sub: "sub-1",
    email: "t@example.test",
    name: "Tess",
    exp: NOW / 1000 + 3600,
  };

  function token(claims: Record<string, unknown>): string {
    const payload = btoa(JSON.stringify(claims))
      .replaceAll("+", "-")
      .replaceAll("/", "_")
      .replaceAll("=", "");
    return `e30.${payload}.sig`;
  }

  it("reads the teacher from a current token for this client", () => {
    expect(parseIdToken(token(valid), TEST_CLIENT_ID, NOW)).toEqual({
      sub: "sub-1",
      email: "t@example.test",
      name: "Tess",
    });
  });

  it("accepts the issuer without a scheme", () => {
    const claims = { ...valid, iss: "accounts.google.com" };
    expect(parseIdToken(token(claims), TEST_CLIENT_ID, NOW)).not.toBeNull();
  });

  it.each([
    ["another issuer", { iss: "https://evil.example" }],
    ["no issuer", { iss: undefined }],
    ["another audience", { aud: "someone-else" }],
    ["an expired token", { exp: NOW / 1000 - 1 }],
    ["a token expiring right now", { exp: NOW / 1000 }],
    ["no expiry", { exp: undefined }],
    ["an expiry that is not a number", { exp: "9999999999" }],
  ])("rejects %s", (_label, override) => {
    expect(parseIdToken(token({ ...valid, ...override }), TEST_CLIENT_ID, NOW)).toBeNull();
  });
});
