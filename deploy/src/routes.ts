import {
  SESSION_COOKIE,
  SESSION_TTL_MS,
  STATE_COOKIE,
  STATE_TTL_SECONDS,
  type Authn,
  type LoginOutcome,
  authenticate,
  completeLogin,
  endSession,
  parseCallbackParams,
} from "./auth.ts";
import { fetchClassroomCourses } from "./classroom.ts";
import { newOauthState } from "./crypto.ts";
import type { Deps } from "./deps.ts";
import { readDocBody } from "./doc.ts";
import { authorizeUrl } from "./google.ts";
import {
  apiError,
  clearCookie,
  json,
  noContent,
  parseCookies,
  redirect,
  serializeCookie,
} from "./http.ts";
import { errorName, logEvent } from "./log.ts";
import { getDoc, putDoc } from "./store.ts";
import type { Teacher } from "./types.ts";

type Method = "GET" | "POST" | "PUT";

interface Context {
  readonly request: Request;
  readonly url: URL;
  readonly deps: Deps;
}

type Route = { readonly method: Method; readonly path: string } & (
  | { readonly access: "public"; readonly handle: (context: Context) => Promise<Response> }
  | {
      readonly access: "teacher";
      readonly handle: (context: Context, teacher: Teacher) => Promise<Response>;
    }
);

const SESSION_MAX_AGE_SECONDS = SESSION_TTL_MS / 1000;

function redirectUri(url: URL): string {
  return `${url.origin}/auth/callback`;
}

async function login({ url, deps }: Context): Promise<Response> {
  const state = newOauthState();
  const cookie = serializeCookie(STATE_COOKIE, state, url, {
    path: "/auth",
    maxAgeSeconds: STATE_TTL_SECONDS,
  });
  return redirect(authorizeUrl(deps.config, redirectUri(url), state), [cookie]);
}

async function callback({ request, url, deps }: Context): Promise<Response> {
  const cookieState = parseCookies(request.headers.get("cookie")).get(STATE_COOKIE) ?? null;
  let outcome: LoginOutcome;
  try {
    outcome = await completeLogin(
      deps,
      parseCallbackParams(url.searchParams),
      cookieState,
      redirectUri(url),
    );
  } catch (error) {
    logEvent("login_failed", { error: errorName(error) });
    outcome = { kind: "failed", code: "server_error" };
  }

  const clearState = clearCookie(STATE_COOKIE, url, "/auth");
  switch (outcome.kind) {
    case "signed-in": {
      const session = serializeCookie(SESSION_COOKIE, outcome.token, url, {
        path: "/",
        maxAgeSeconds: SESSION_MAX_AGE_SECONDS,
      });
      return redirect("/", [clearState, session]);
    }
    case "failed":
      return redirect(`/?auth_error=${outcome.code}`, [clearState]);
    default:
      return outcome satisfies never;
  }
}

async function logout({ request, url, deps }: Context): Promise<Response> {
  await endSession(deps, request.headers.get("cookie"));
  return noContent([clearCookie(SESSION_COOKIE, url, "/")]);
}

async function me(_context: Context, teacher: Teacher): Promise<Response> {
  return json({ email: teacher.email, name: teacher.name });
}

async function readDoc({ deps }: Context, teacher: Teacher): Promise<Response> {
  const doc = await getDoc(deps.sql, teacher.sub);
  if (doc === null) {
    return noContent();
  }
  return new Response(doc, {
    headers: { "content-type": "application/json", "cache-control": "no-store" },
  });
}

async function writeDoc({ request, deps }: Context, teacher: Teacher): Promise<Response> {
  const body = await readDocBody(request);
  switch (body.kind) {
    case "ok":
      await putDoc(deps.sql, teacher.sub, body.text, deps.now());
      return noContent();
    case "too_large":
      return apiError(413, "too_large");
    case "invalid":
      return apiError(400, "invalid_doc");
    default:
      return body satisfies never;
  }
}

async function classroomCourses({ deps }: Context, teacher: Teacher): Promise<Response> {
  const courses = await fetchClassroomCourses(deps, teacher);
  switch (courses.kind) {
    case "ok":
      return json({ courses: courses.value });
    case "reauthorize":
      return apiError(403, "reauthorize");
    case "unavailable":
      return apiError(502, "classroom_unavailable");
    default:
      return courses satisfies never;
  }
}

export const routes: readonly Route[] = [
  { method: "GET", path: "/auth/login", access: "public", handle: login },
  { method: "GET", path: "/auth/callback", access: "public", handle: callback },
  { method: "POST", path: "/auth/logout", access: "public", handle: logout },
  { method: "GET", path: "/api/me", access: "teacher", handle: me },
  { method: "GET", path: "/api/doc", access: "teacher", handle: readDoc },
  { method: "PUT", path: "/api/doc", access: "teacher", handle: writeDoc },
  { method: "GET", path: "/api/classroom/courses", access: "teacher", handle: classroomCourses },
];

async function run(route: Route, context: Context): Promise<Response> {
  switch (route.access) {
    case "public":
      return route.handle(context);
    case "teacher": {
      const authn: Authn = await authenticate(context.deps, context.request.headers.get("cookie"));
      return authn.kind === "teacher"
        ? route.handle(context, authn.teacher)
        : apiError(401, "unauthorized");
    }
    default:
      return route satisfies never;
  }
}

export async function handle(request: Request, deps: Deps): Promise<Response> {
  const url = new URL(request.url);
  const sameRoute = routes.filter((route) => route.path === url.pathname);
  if (sameRoute.length === 0) {
    return apiError(404, "not_found");
  }
  const route = sameRoute.find((candidate) => candidate.method === request.method);
  if (route === undefined) {
    return apiError(405, "method_not_allowed", {
      allow: sameRoute.map((candidate) => candidate.method).join(", "),
    });
  }
  try {
    return await run(route, { request, url, deps });
  } catch (error) {
    logEvent("unhandled_error", { error: errorName(error) });
    return apiError(500, "internal");
  }
}
