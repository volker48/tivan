import type { DatabaseSync } from "node:sqlite";
import {
  createFakeGoogle,
  type FakeGoogle,
  type FakeGoogleOptions,
} from "../../dev/fake-google-core.ts";
import { parseConfig } from "../config.ts";
import type { Deps } from "../deps.ts";
import { handle } from "../routes.ts";
import { sqliteSql } from "./sqlite.ts";

export const TEST_ORIGIN = "https://tivan.test";
const GOOGLE_ORIGIN = "https://google.test";

export const TEST_CLIENT_ID = "test-client-id";
/** The smallest body `PUT /api/doc` accepts. */
export const ENVELOPE = '{"version":6,"doc":{}}';
export const TEST_KEY = btoa(String.fromCharCode(...Array.from({ length: 32 }, (_, i) => i + 1)));

export const TEST_ENV = {
  GOOGLE_CLIENT_ID: TEST_CLIENT_ID,
  GOOGLE_CLIENT_SECRET: "test-client-secret",
  TOKEN_ENC_KEY: TEST_KEY,
  GOOGLE_AUTHORIZE_URL: `${GOOGLE_ORIGIN}/authorize`,
  GOOGLE_TOKEN_URL: `${GOOGLE_ORIGIN}/token`,
  CLASSROOM_API_URL: GOOGLE_ORIGIN,
};

export interface TestClock {
  now: number;
  advance(ms: number): void;
}

export interface TestApp {
  readonly google: FakeGoogle;
  readonly clock: TestClock;
  readonly db: DatabaseSync;
  readonly deps: Deps;
  /** Sends a request to the Worker. Cookies from earlier responses are sent back automatically. */
  request(path: string, init?: RequestInit & { origin?: string }): Promise<Response>;
  /** Cookies the browser would currently hold. */
  readonly cookies: Map<string, string>;
  /** Starts a sign-in and returns the `state` Google would echo back. The state cookie is kept. */
  startLogin(): Promise<string>;
  /** Runs the whole sign-in as the fake teacher and returns the callback response. */
  signIn(origin?: string): Promise<Response>;
  /** Wraps the network from now on, to inject failures into later calls. */
  interceptNetwork(wrap: (inner: typeof fetch) => typeof fetch): void;
}

export interface TestAppOptions {
  readonly google?: FakeGoogleOptions;
  /** Wraps the network the Worker sees, to inject failures. */
  readonly wrapFetch?: (inner: typeof fetch) => typeof fetch;
  readonly env?: Record<string, unknown>;
}

export function createTestApp(options: TestAppOptions = {}): TestApp {
  const clock: TestClock = {
    now: Date.UTC(2026, 9, 16),
    advance(ms) {
      clock.now += ms;
    },
  };
  const google = createFakeGoogle({ now: () => clock.now, ...options.google });
  const { sql, db } = sqliteSql();

  const parsed = parseConfig({ ...TEST_ENV, ...options.env });
  if (parsed.kind === "invalid") {
    throw new Error(`test config is invalid: ${parsed.problems.join("; ")}`);
  }
  const googleNetwork: typeof fetch = (input, init) => google.fetch(new Request(input, init));
  let network = options.wrapFetch?.(googleNetwork) ?? googleNetwork;
  const deps: Deps = {
    sql,
    fetch: (input, init) => network(input, init),
    now: () => clock.now,
    config: parsed.config,
  };

  const cookies = new Map<string, string>();

  async function request(
    path: string,
    init: RequestInit & { origin?: string } = {},
  ): Promise<Response> {
    const { origin = TEST_ORIGIN, ...rest } = init;
    const headers = new Headers(rest.headers);
    if (cookies.size > 0 && !headers.has("cookie")) {
      headers.set("cookie", [...cookies].map(([name, value]) => `${name}=${value}`).join("; "));
    }
    const response = await handle(new Request(new URL(path, origin), { ...rest, headers }), deps);
    for (const setCookie of response.headers.getSetCookie()) {
      const [pair = "", ...attributes] = setCookie.split(";").map((part) => part.trim());
      const separator = pair.indexOf("=");
      const name = pair.slice(0, separator);
      if (attributes.includes("Max-Age=0")) {
        cookies.delete(name);
      } else {
        cookies.set(name, pair.slice(separator + 1));
      }
    }
    return response;
  }

  async function startLogin(): Promise<string> {
    const login = await request("/auth/login");
    const state = new URL(login.headers.get("location") ?? "").searchParams.get("state");
    if (state === null) {
      throw new Error("the login redirect carried no state");
    }
    return state;
  }

  async function signIn(origin = TEST_ORIGIN): Promise<Response> {
    const login = await request("/auth/login", { origin });
    const authorized = await google.fetch(new Request(login.headers.get("location") ?? ""));
    const callback = new URL(authorized.headers.get("location") ?? "");
    return request(`${callback.pathname}${callback.search}`, { origin });
  }

  function interceptNetwork(wrap: (inner: typeof fetch) => typeof fetch): void {
    network = wrap(network);
  }

  return { google, clock, db, deps, request, cookies, startLogin, signIn, interceptNetwork };
}
