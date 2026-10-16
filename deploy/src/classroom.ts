import { decryptToken } from "./crypto.ts";
import type { Deps } from "./deps.ts";
import { refreshAccessToken } from "./google.ts";
import { isRecord, readJson } from "./json.ts";
import { errorName, logEvent } from "./log.ts";
import { getRefreshTokenEnc } from "./store.ts";
import type { Fetched, Teacher } from "./types.ts";

export interface ClassroomStudent {
  readonly id: string;
  readonly name: string;
}

export interface ClassroomCourse {
  readonly id: string;
  readonly name: string;
  readonly students: readonly ClassroomStudent[];
}

type CourseHeader = Pick<ClassroomCourse, "id" | "name">;
type Failure = Exclude<Fetched<never>, { kind: "ok" }>;

const ROSTER_CONCURRENCY = 4;
// Guards against an upstream that keeps returning a next page token.
const MAX_PAGES = 100;

function parseCourse(raw: unknown): CourseHeader | null {
  if (!isRecord(raw) || typeof raw["id"] !== "string") {
    return null;
  }
  return { id: raw["id"], name: typeof raw["name"] === "string" ? raw["name"] : "" };
}

function parseStudent(raw: unknown): ClassroomStudent | null {
  if (!isRecord(raw) || typeof raw["userId"] !== "string") {
    return null;
  }
  const profile = raw["profile"];
  const name = isRecord(profile) ? profile["name"] : undefined;
  const fullName = isRecord(name) ? name["fullName"] : undefined;
  return { id: raw["userId"], name: typeof fullName === "string" ? fullName : "" };
}

/**
 * A 403 that signing in again can fix: the token lacks a scope. Other 403s, such as Classroom
 * being switched off for the account, would only send the teacher around in a loop.
 */
function isMissingScope(body: unknown): boolean {
  const error = isRecord(body) ? body["error"] : undefined;
  if (!isRecord(error)) {
    return false;
  }
  const reasonIn = (key: string, wanted: string): boolean => {
    const list = error[key];
    return Array.isArray(list) && list.some((item) => isRecord(item) && item["reason"] === wanted);
  };
  return (
    reasonIn("details", "ACCESS_TOKEN_SCOPE_INSUFFICIENT") ||
    reasonIn("errors", "insufficientPermissions")
  );
}

/** Follows `nextPageToken` until Google stops returning one. */
async function listAll<T>(
  deps: Deps,
  accessToken: string,
  path: string,
  query: Record<string, string>,
  itemsKey: string,
  parseItem: (raw: unknown) => T | null,
): Promise<Fetched<T[]>> {
  const base = deps.config.classroomUrl.href.replace(/\/$/, "");
  const items: T[] = [];
  let pageToken: string | null = null;

  for (let page = 0; page < MAX_PAGES; page += 1) {
    const url = new URL(base + path);
    for (const [name, value] of Object.entries(query)) {
      url.searchParams.set(name, value);
    }
    if (pageToken !== null) {
      url.searchParams.set("pageToken", pageToken);
    }

    let response: Response;
    try {
      response = await deps.fetch(url, { headers: { authorization: `Bearer ${accessToken}` } });
    } catch (error) {
      logEvent("classroom_request_failed", { reason: "network", error: errorName(error) });
      return { kind: "unavailable" };
    }
    const body = await readJson(response);
    if (response.status === 401 || (response.status === 403 && isMissingScope(body))) {
      return { kind: "reauthorize" };
    }
    if (!response.ok || !isRecord(body)) {
      logEvent("classroom_request_failed", { status: response.status });
      return { kind: "unavailable" };
    }

    const raw = body[itemsKey];
    for (const item of Array.isArray(raw) ? raw : []) {
      const parsed = parseItem(item);
      if (parsed === null) {
        // A partial roster would read as students who left, and the next sync would remove them.
        logEvent("classroom_request_failed", { reason: "unreadable_item" });
        return { kind: "unavailable" };
      }
      items.push(parsed);
    }
    const next = body["nextPageToken"];
    if (typeof next !== "string" || next === "") {
      return { kind: "ok", value: items };
    }
    pageToken = next;
  }
  logEvent("classroom_request_failed", { reason: "too_many_pages" });
  return { kind: "unavailable" };
}

async function fetchRosters(
  deps: Deps,
  accessToken: string,
  courses: readonly CourseHeader[],
): Promise<Fetched<ClassroomCourse[]>> {
  const rosters: ClassroomCourse[] = [];
  let next = 0;
  let failed = false;

  const worker = async (): Promise<Failure | null> => {
    while (!failed) {
      const index = next;
      next += 1;
      const course = courses[index];
      if (course === undefined) {
        return null;
      }
      const students = await listAll(
        deps,
        accessToken,
        `/v1/courses/${encodeURIComponent(course.id)}/students`,
        {},
        "students",
        parseStudent,
      );
      if (students.kind !== "ok") {
        failed = true;
        return students;
      }
      rosters[index] = { ...course, students: students.value };
    }
    return null;
  };
  const outcomes = await Promise.all(
    Array.from({ length: Math.min(ROSTER_CONCURRENCY, courses.length) }, () => worker()),
  );

  return (
    outcomes.find((outcome): outcome is Failure => outcome !== null) ?? {
      kind: "ok",
      value: rosters,
    }
  );
}

/** The teacher's ACTIVE taught courses, each with its full roster. */
export async function fetchClassroomCourses(
  deps: Deps,
  teacher: Teacher,
): Promise<Fetched<ClassroomCourse[]>> {
  const sealed = await getRefreshTokenEnc(deps.sql, teacher.sub);
  const refreshToken = sealed === null ? null : await decryptToken(deps.config.tokenKey, sealed);
  if (refreshToken === null) {
    logEvent("refresh_token_unreadable");
    return { kind: "reauthorize" };
  }

  const access = await refreshAccessToken(deps, refreshToken);
  if (access.kind !== "ok") {
    return access;
  }
  const courses = await listAll(
    deps,
    access.value,
    "/v1/courses",
    { teacherId: "me", courseStates: "ACTIVE" },
    "courses",
    parseCourse,
  );
  if (courses.kind !== "ok") {
    return courses;
  }
  return fetchRosters(deps, access.value, courses.value);
}
