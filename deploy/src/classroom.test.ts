import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  FAKE_ACCESS_TOKEN,
  FAKE_COURSES,
  FAKE_REFRESH_TOKEN,
  type FakeCourse,
  type FakeGoogleOptions,
} from "../dev/fake-google-core.ts";
import { type TestApp, createTestApp } from "./testing/app.ts";

let logged: ReturnType<typeof vi.spyOn>;

beforeEach(() => {
  logged = vi.spyOn(console, "error").mockImplementation(() => {});
});

afterEach(() => {
  vi.restoreAllMocks();
});

interface SignedInOptions {
  readonly google?: FakeGoogleOptions;
  /** Wraps the network after sign-in, so only the Classroom calls see the failure. */
  readonly wrap?: (inner: typeof fetch) => typeof fetch;
}

async function signedIn(options: SignedInOptions = {}): Promise<TestApp> {
  const app = createTestApp({ google: options.google });
  await app.signIn();
  if (options.wrap !== undefined) {
    app.interceptNetwork(options.wrap);
  }
  app.google.requests.length = 0;
  return app;
}

const courses = (app: TestApp) => app.request("/api/classroom/courses");

describe("GET /api/classroom/courses", () => {
  it("returns every active course with its full roster, following nextPageToken", async () => {
    const app = await signedIn();

    const response = await courses(app);

    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({
      courses: FAKE_COURSES.map(({ id, name, students }) => ({ id, name, students })),
    });
    expect(app.google.requests.filter((r) => r === "GET /v1/courses")).toHaveLength(2);
    expect(
      app.google.requests.filter((r) => r === "GET /v1/courses/course-1/students"),
    ).toHaveLength(3);
    expect(
      app.google.requests.filter((r) => r === "GET /v1/courses/course-2/students"),
    ).toHaveLength(1);
    expect(
      app.google.requests.filter((r) => r === "GET /v1/courses/course-3/students"),
    ).toHaveLength(1);
    expect(app.google.requests.filter((r) => r === "POST /token")).toHaveLength(1);
  });

  it("asks Google only for the teacher's ACTIVE courses and sends a bearer token", async () => {
    const seen: Request[] = [];
    const app = await signedIn({
      wrap: (inner) => (input, init) => {
        seen.push(new Request(input, init));
        return inner(input, init);
      },
    });

    await courses(app);

    const first = seen.find((request) => new URL(request.url).pathname === "/v1/courses");
    const url = new URL(first?.url ?? "");
    expect(url.searchParams.get("teacherId")).toBe("me");
    expect(url.searchParams.get("courseStates")).toBe("ACTIVE");
    expect(first?.headers.get("authorization")).toBe(`Bearer ${FAKE_ACCESS_TOKEN}`);
  });

  it("returns an empty list for a teacher with no courses", async () => {
    const app = await signedIn({ google: { courses: [] } });
    const response = await courses(app);
    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({ courses: [] });
  });

  it("returns an empty roster for a course without students", async () => {
    const app = await signedIn({ google: { courses: [{ id: "c", name: "Empty", students: [] }] } });
    expect(await (await courses(app)).json()).toEqual({
      courses: [{ id: "c", name: "Empty", students: [] }],
    });
  });

  it("keeps course order and every student across many pages", async () => {
    const many: FakeCourse[] = Array.from({ length: 7 }, (_, c) => ({
      id: `c${c}`,
      name: `Course ${c}`,
      students: Array.from({ length: 11 }, (_, s) => ({
        id: `c${c}-s${s}`,
        name: `Student ${c}.${s}`,
      })),
    }));
    const app = await signedIn({ google: { courses: many, pageSize: 3 } });

    const body = (await (await courses(app)).json()) as { courses: FakeCourse[] };

    expect(body.courses.map((course) => course.id)).toEqual(many.map((course) => course.id));
    for (const [index, course] of body.courses.entries()) {
      expect(course.students).toEqual(many[index]?.students);
    }
  });

  it("uses userId as the student id and profile.name.fullName as the name", async () => {
    const app = await signedIn({
      wrap: (inner) => async (input, init) => {
        const { pathname } = new URL(new Request(input, init).url);
        if (pathname === "/v1/courses") {
          return Response.json({ courses: [{ id: "c1", name: "Art" }] });
        }
        if (pathname === "/v1/courses/c1/students") {
          return Response.json({
            students: [
              {
                userId: "u1",
                profile: { id: "p1", name: { givenName: "A", fullName: "Ann Álvarez" } },
              },
              { userId: "u2", profile: { id: "p2" } },
            ],
          });
        }
        return inner(input, init);
      },
    });

    expect(await (await courses(app)).json()).toEqual({
      courses: [
        {
          id: "c1",
          name: "Art",
          students: [
            { id: "u1", name: "Ann Álvarez" },
            { id: "u2", name: "" },
          ],
        },
      ],
    });
  });

  it("fetches rosters with bounded concurrency", async () => {
    const many: FakeCourse[] = Array.from({ length: 12 }, (_, c) => ({
      id: `c${c}`,
      name: `Course ${c}`,
      students: [{ id: `s${c}`, name: `Student ${c}` }],
    }));
    let inFlight = 0;
    let peak = 0;
    const app = await signedIn({
      google: { courses: many, pageSize: 100 },
      wrap: (inner) => async (input, init) => {
        if (!new URL(new Request(input, init).url).pathname.endsWith("/students")) {
          return inner(input, init);
        }
        inFlight += 1;
        peak = Math.max(peak, inFlight);
        await new Promise((resolve) => setTimeout(resolve, 5));
        try {
          return await inner(input, init);
        } finally {
          inFlight -= 1;
        }
      },
    });

    const body = (await (await courses(app)).json()) as { courses: unknown[] };

    expect(body.courses).toHaveLength(12);
    expect(peak).toBeGreaterThan(1);
    expect(peak).toBeLessThanOrEqual(4);
  });
});

describe("when Google rejects the teacher", () => {
  it("answers 403 reauthorize on invalid_grant, without calling Classroom", async () => {
    const app = await signedIn();
    app.google.revokeRefreshToken();

    const response = await courses(app);

    expect(response.status).toBe(403);
    expect(await response.json()).toEqual({ error: "reauthorize" });
    expect(app.google.requests).toEqual(["POST /token"]);
  });

  it("answers 403 reauthorize when Classroom returns 401 for the course list", async () => {
    const app = await signedIn({
      wrap: (inner) => (input, init) =>
        new URL(new Request(input, init).url).pathname === "/v1/courses"
          ? Promise.resolve(
              Response.json({ error: { status: "UNAUTHENTICATED" } }, { status: 401 }),
            )
          : inner(input, init),
    });
    const response = await courses(app);
    expect(response.status).toBe(403);
    expect(await response.json()).toEqual({ error: "reauthorize" });
  });

  it("answers 403 reauthorize when scopes are missing and a roster call returns 403", async () => {
    const app = await signedIn({
      wrap: (inner) => (input, init) =>
        new URL(new Request(input, init).url).pathname.endsWith("/course-2/students")
          ? Promise.resolve(
              Response.json({ error: { status: "PERMISSION_DENIED" } }, { status: 403 }),
            )
          : inner(input, init),
    });
    const response = await courses(app);
    expect(response.status).toBe(403);
    expect(await response.json()).toEqual({ error: "reauthorize" });
  });

  it("answers 403 reauthorize when the stored refresh token can no longer be decrypted", async () => {
    const app = await signedIn();
    app.db.exec("UPDATE teachers SET refresh_token_enc = 'tampered'");
    const response = await courses(app);
    expect(response.status).toBe(403);
    expect(await response.json()).toEqual({ error: "reauthorize" });
    expect(app.google.requests).toEqual([]);
  });

  it("answers 502 instead of a partial roster when a student entry cannot be read", async () => {
    const app = await signedIn({
      wrap: (inner) => async (input, init) => {
        const response = await inner(input, init);
        if (!new URL(new Request(input, init).url).pathname.endsWith("/course-1/students")) {
          return response;
        }
        const body = (await response.json()) as { students?: unknown[] };
        return Response.json({ ...body, students: [...(body.students ?? []), { profile: {} }] });
      },
    });
    const response = await courses(app);
    expect(response.status).toBe(502);
    expect(await response.json()).toEqual({ error: "classroom_unavailable" });
  });

  it("answers 502 when a course entry has no id", async () => {
    const app = await signedIn({
      wrap: (inner) => async (input, init) =>
        new URL(new Request(input, init).url).pathname === "/v1/courses"
          ? Response.json({ courses: [{ name: "no id" }] })
          : inner(input, init),
    });
    expect((await courses(app)).status).toBe(502);
  });

  it("shows the rosters set later with setCourses", async () => {
    const app = await signedIn();
    app.google.setCourses([{ id: "only", name: "Only", students: [{ id: "u1", name: "Una" }] }]);
    const response = await courses(app);
    expect(await response.json()).toEqual({
      courses: [{ id: "only", name: "Only", students: [{ id: "u1", name: "Una" }] }],
    });
  });

  it("recovers after the teacher signs in again", async () => {
    const app = await signedIn();
    app.google.revokeRefreshToken();
    expect((await courses(app)).status).toBe(403);

    app.google.restoreRefreshToken();
    await app.signIn();

    expect((await courses(app)).status).toBe(200);
  });
});

describe("when Google is unavailable", () => {
  function failing(
    match: (pathname: string) => boolean,
    respond: () => Promise<Response>,
  ): SignedInOptions {
    return {
      wrap: (inner) => (input, init) =>
        match(new URL(new Request(input, init).url).pathname) ? respond() : inner(input, init),
    };
  }

  it.each([
    [
      "a 500 from the course list",
      failing(
        (p) => p === "/v1/courses",
        async () => new Response("boom", { status: 500 }),
      ),
    ],
    [
      "a 503 from a roster",
      failing(
        (p) => p.endsWith("/students"),
        async () => new Response("down", { status: 503 }),
      ),
    ],
    [
      "a 404 from a roster",
      failing(
        (p) => p.endsWith("/students"),
        async () => new Response("gone", { status: 404 }),
      ),
    ],
    [
      "a non-JSON course list",
      failing(
        (p) => p === "/v1/courses",
        async () => new Response("<html>", { status: 200 }),
      ),
    ],
    [
      "a network error",
      failing(
        (p) => p === "/v1/courses",
        () => Promise.reject(new TypeError("offline")),
      ),
    ],
    [
      "a 500 from the token endpoint",
      failing(
        (p) => p === "/token",
        async () => new Response("boom", { status: 500 }),
      ),
    ],
    [
      "a token endpoint that rejects the client",
      failing(
        (p) => p === "/token",
        async () => Response.json({ error: "invalid_client" }, { status: 401 }),
      ),
    ],
  ])("answers 502 classroom_unavailable on %s", async (_label, options) => {
    const app = await signedIn(options);

    const response = await courses(app);

    expect(response.status).toBe(502);
    expect(await response.json()).toEqual({ error: "classroom_unavailable" });
  });

  it("gives up on an upstream that never stops paginating", async () => {
    const app = await signedIn(
      failing(
        (p) => p === "/v1/courses",
        async () => Response.json({ courses: [{ id: "c", name: "Loop" }], nextPageToken: "again" }),
      ),
    );
    const response = await courses(app);
    expect(response.status).toBe(502);
  });

  it("logs codes only, never tokens or roster data", async () => {
    const app = await signedIn(
      failing(
        (p) => p.endsWith("/students"),
        async () => new Response("x", { status: 500 }),
      ),
    );

    await courses(app);

    const output = JSON.stringify(logged.mock.calls);
    expect(output).toContain("classroom_request_failed");
    for (const secret of [
      FAKE_ACCESS_TOKEN,
      FAKE_REFRESH_TOKEN,
      "Ada Lovelace",
      "Period 1 Science",
    ]) {
      expect(output).not.toContain(secret);
    }
  });
});
