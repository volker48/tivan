/**
 * A deterministic stand-in for the Google endpoints the Worker calls: authorize, token, and the
 * Classroom courses and students lists. `fake-google.ts` serves it over HTTP for local
 * development and the tests call it in process, so one fake backs both.
 *
 * It checks the shape of every request the way Google would (scopes, `teacherId=me`,
 * `courseStates=ACTIVE`, single-use codes, matching redirect URI), so a Worker that sends the
 * wrong request fails here instead of in production.
 */
import { encodeBase64Url } from "../src/crypto.ts";

export interface FakeStudent {
  readonly id: string;
  readonly givenName: string;
  readonly familyName: string;
}

export interface FakeCourse {
  readonly id: string;
  readonly name: string;
  readonly students: readonly FakeStudent[];
}

export const FAKE_TEACHER = {
  sub: "fake-teacher-1",
  email: "ms.frizzle@example.test",
  name: "Ms. Frizzle",
} as const;

export const FAKE_REFRESH_TOKEN = "fake-refresh-token";
export const FAKE_ACCESS_TOKEN = "fake-access-token";

const REQUIRED_SCOPES = [
  "openid",
  "email",
  "profile",
  "https://www.googleapis.com/auth/classroom.courses.readonly",
  "https://www.googleapis.com/auth/classroom.rosters.readonly",
];

function student(id: string, givenName: string, familyName: string): FakeStudent {
  return { id, givenName, familyName };
}

export const FAKE_COURSES: readonly FakeCourse[] = [
  {
    id: "course-1",
    name: "Period 1 Science",
    students: [
      student("s-101", "Ada", "Lovelace"),
      student("s-102", "Grace", "Hopper"),
      student("s-103", "Katherine", "Johnson"),
      student("s-104", "Alan", "Turing"),
      student("s-105", "Ada", "Yonath"),
    ],
  },
  {
    id: "course-2",
    name: "Period 3 Math",
    students: [student("s-201", "Emmy", "Noether"), student("s-202", "Srinivasa", "Ramanujan")],
  },
  { id: "course-3", name: "Homeroom", students: [] },
];

export interface FakeGoogleOptions {
  readonly courses?: readonly FakeCourse[];
  /** Items per page. Small by default so pagination is always exercised. */
  readonly pageSize?: number;
  /** Milliseconds since the epoch. The ID token's `iat` and `exp` follow it. */
  readonly now?: () => number;
}

export interface FakeGoogle {
  fetch(request: Request): Promise<Response>;
  /** Makes the refresh token fail with `invalid_grant`, as after the teacher revokes access. */
  revokeRefreshToken(): void;
  restoreRefreshToken(): void;
  /** `METHOD /path` of every request served, in order. Tests may empty it between steps. */
  readonly requests: string[];
}

function base64UrlJson(value: unknown): string {
  return encodeBase64Url(new TextEncoder().encode(JSON.stringify(value)));
}

function idToken(audience: string, nowMs: number): string {
  const iat = Math.floor(nowMs / 1000);
  const claims = {
    iss: "https://accounts.google.com",
    aud: audience,
    sub: FAKE_TEACHER.sub,
    email: FAKE_TEACHER.email,
    name: FAKE_TEACHER.name,
    iat,
    exp: iat + 3600,
  };
  return [base64UrlJson({ alg: "none", typ: "JWT" }), base64UrlJson(claims), "fake-signature"].join(
    ".",
  );
}

function jsonResponse(body: unknown, status = 200): Response {
  return Response.json(body, { status });
}

function badRequest(message: string): Response {
  return new Response(`fake-google: ${message}\n`, { status: 400 });
}

function oauthError(error: string): Response {
  return jsonResponse({ error }, 400);
}

export function createFakeGoogle(options: FakeGoogleOptions = {}): FakeGoogle {
  const courses = options.courses ?? FAKE_COURSES;
  const now = options.now ?? Date.now;
  const pageSize = options.pageSize ?? 2;
  const requests: string[] = [];
  const issuedCodes = new Map<string, string>();
  let codesIssued = 0;
  let revoked = false;

  function authorize(url: URL): Response {
    const query = url.searchParams;
    const redirectUri = query.get("redirect_uri");
    const state = query.get("state");
    const scopes = (query.get("scope") ?? "").split(" ");
    if (!query.get("client_id")) return badRequest("client_id is missing");
    if (redirectUri === null) return badRequest("redirect_uri is missing");
    if (!state) return badRequest("state is missing");
    if (query.get("response_type") !== "code") return badRequest("response_type must be code");
    if (query.get("access_type") !== "offline") return badRequest("access_type must be offline");
    if (query.get("prompt") !== "consent") return badRequest("prompt must be consent");
    const missing = REQUIRED_SCOPES.filter((scope) => !scopes.includes(scope));
    if (missing.length > 0) return badRequest(`scopes missing: ${missing.join(", ")}`);

    codesIssued += 1;
    const code = `fake-code-${codesIssued}`;
    issuedCodes.set(code, redirectUri);
    const target = new URL(redirectUri);
    target.searchParams.set("code", code);
    target.searchParams.set("state", state);
    return new Response(null, { status: 302, headers: { location: target.toString() } });
  }

  async function token(request: Request): Promise<Response> {
    const form = new URLSearchParams(await request.text());
    const clientId = form.get("client_id");
    if (!clientId || !form.get("client_secret")) return oauthError("invalid_client");

    switch (form.get("grant_type")) {
      case "authorization_code": {
        const code = form.get("code") ?? "";
        const redirectUri = issuedCodes.get(code);
        if (redirectUri === undefined || redirectUri !== form.get("redirect_uri")) {
          return oauthError("invalid_grant");
        }
        issuedCodes.delete(code);
        return jsonResponse({
          access_token: FAKE_ACCESS_TOKEN,
          refresh_token: FAKE_REFRESH_TOKEN,
          id_token: idToken(clientId, now()),
          expires_in: 3600,
          scope: REQUIRED_SCOPES.join(" "),
          token_type: "Bearer",
        });
      }
      case "refresh_token":
        if (revoked || form.get("refresh_token") !== FAKE_REFRESH_TOKEN) {
          return oauthError("invalid_grant");
        }
        return jsonResponse({
          access_token: FAKE_ACCESS_TOKEN,
          expires_in: 3600,
          scope: REQUIRED_SCOPES.join(" "),
          token_type: "Bearer",
        });
      default:
        return oauthError("unsupported_grant_type");
    }
  }

  function page<T>(items: readonly T[], pageToken: string | null, key: string): Response {
    const start = Number(pageToken ?? 0);
    const end = start + pageSize;
    const body: Record<string, unknown> = {};
    if (items.length > 0) body[key] = items.slice(start, end);
    if (end < items.length) body["nextPageToken"] = String(end);
    return jsonResponse(body);
  }

  function classroom(url: URL, request: Request): Response {
    if (request.headers.get("authorization") !== `Bearer ${FAKE_ACCESS_TOKEN}`) {
      return jsonResponse({ error: { code: 401, status: "UNAUTHENTICATED" } }, 401);
    }
    const pageToken = url.searchParams.get("pageToken");

    if (url.pathname === "/v1/courses") {
      if (url.searchParams.get("teacherId") !== "me") return badRequest("teacherId must be me");
      if (url.searchParams.get("courseStates") !== "ACTIVE") {
        return badRequest("courseStates must be ACTIVE");
      }
      const listed = courses.map(({ id, name }) => ({ id, name, courseState: "ACTIVE" }));
      return page(listed, pageToken, "courses");
    }

    const roster = /^\/v1\/courses\/([^/]+)\/students$/.exec(url.pathname);
    const course = courses.find(
      (candidate) => candidate.id === decodeURIComponent(roster?.[1] ?? ""),
    );
    if (roster === null || course === undefined) {
      return jsonResponse({ error: { code: 404, status: "NOT_FOUND" } }, 404);
    }
    const students = course.students.map(({ id, givenName, familyName }) => ({
      courseId: course.id,
      userId: id,
      profile: { id, name: { givenName, familyName, fullName: `${givenName} ${familyName}` } },
    }));
    return page(students, pageToken, "students");
  }

  return {
    requests,
    revokeRefreshToken: () => {
      revoked = true;
    },
    restoreRefreshToken: () => {
      revoked = false;
    },
    async fetch(request) {
      const url = new URL(request.url);
      requests.push(`${request.method} ${url.pathname}`);

      if (request.method === "GET" && url.pathname === "/authorize") return authorize(url);
      if (request.method === "POST" && url.pathname === "/token") return token(request);
      if (request.method === "GET" && url.pathname.startsWith("/v1/courses")) {
        return classroom(url, request);
      }
      return new Response("fake-google: not found\n", { status: 404 });
    },
  };
}
