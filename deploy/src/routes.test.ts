import { afterEach, describe, expect, it, vi } from "vitest";
import { SESSION_COOKIE, SESSION_TTL_MS } from "./auth.ts";
import { routes } from "./routes.ts";
import { createTestApp } from "./testing/app.ts";

afterEach(() => {
  vi.restoreAllMocks();
});

const teacherRoutes = routes
  .filter((route) => route.access === "teacher")
  .map((route) => [`${route.method} ${route.path}`, route] as const);

function requestFor(route: { method: string; path: string }): [string, RequestInit] {
  const body = route.method === "PUT" ? '{"version":5,"doc":{}}' : undefined;
  return [route.path, { method: route.method, body }];
}

describe("the route table", () => {
  it("serves exactly the contract's routes, and only the sign-in flow is public", () => {
    expect(routes.map((route) => `${route.method} ${route.path} ${route.access}`)).toEqual([
      "GET /auth/login public",
      "GET /auth/callback public",
      "POST /auth/logout public",
      "GET /api/me teacher",
      "GET /api/doc teacher",
      "PUT /api/doc teacher",
      "GET /api/classroom/courses teacher",
    ]);
  });
});

describe("every protected route", () => {
  it.each(teacherRoutes)("%s answers 401 without a session", async (_name, route) => {
    const app = createTestApp();

    const response = await app.request(...requestFor(route));

    expect(response.status).toBe(401);
    expect(await response.json()).toEqual({ error: "unauthorized" });
    expect(response.headers.get("cache-control")).toBe("no-store");
    expect(app.google.requests).toEqual([]);
  });

  it.each(teacherRoutes)(
    "%s answers 401 once the session has expired or been logged out",
    async (_name, route) => {
      const app = createTestApp();
      await app.signIn();
      const cookie = app.cookies.get(SESSION_COOKIE) ?? "";
      const [path, init] = requestFor(route);
      const withCookie = { ...init, headers: { cookie: `${SESSION_COOKIE}=${cookie}` } };

      app.clock.advance(SESSION_TTL_MS);
      expect((await app.request(path, withCookie)).status).toBe(401);

      app.clock.advance(-SESSION_TTL_MS);
      await app.signIn();
      const fresh = app.cookies.get(SESSION_COOKIE) ?? "";
      await app.request("/auth/logout", { method: "POST" });
      const afterLogout = { ...init, headers: { cookie: `${SESSION_COOKIE}=${fresh}` } };
      expect((await app.request(path, afterLogout)).status).toBe(401);
    },
  );

  it("does not store a doc that an anonymous caller sends", async () => {
    const app = createTestApp();
    await app.request("/api/doc", { method: "PUT", body: '{"version":5,"doc":{}}' });
    expect(app.db.prepare("SELECT 1 FROM teachers").all()).toEqual([]);
  });
});

describe("GET /api/me", () => {
  it("is not cacheable", async () => {
    const app = createTestApp();
    await app.signIn();
    const response = await app.request("/api/me");
    expect(response.headers.get("cache-control")).toBe("no-store");
    expect(response.headers.get("content-type")).toContain("application/json");
  });
});

describe("unknown requests", () => {
  it("answer 404 with a JSON error", async () => {
    const app = createTestApp();
    for (const path of ["/api/nope", "/api", "/auth", "/auth/other", "/api/me/extra"]) {
      const response = await app.request(path);
      expect(response.status).toBe(404);
      expect(await response.json()).toEqual({ error: "not_found" });
    }
  });

  it("answer 405 with the allowed methods when the path exists", async () => {
    const app = createTestApp();

    const logout = await app.request("/auth/logout");
    expect(logout.status).toBe(405);
    expect(logout.headers.get("allow")).toBe("POST");
    expect(await logout.json()).toEqual({ error: "method_not_allowed" });

    const doc = await app.request("/api/doc", { method: "POST", body: "{}" });
    expect(doc.status).toBe(405);
    expect(doc.headers.get("allow")).toBe("GET, PUT");

    expect((await app.request("/api/me", { method: "DELETE" })).status).toBe(405);
  });
});

describe("unexpected failures", () => {
  it("answer 500 internal and log only the error name", async () => {
    const app = createTestApp();
    await app.signIn();
    const logged = vi.spyOn(console, "error").mockImplementation(() => {});
    app.db.exec("DROP TABLE teachers");

    const response = await app.request("/api/me");

    expect(response.status).toBe(500);
    expect(await response.json()).toEqual({ error: "internal" });
    expect(JSON.stringify(logged.mock.calls)).toBe(
      '[["{\\"event\\":\\"unhandled_error\\",\\"error\\":\\"Error\\"}"]]',
    );
  });
});
