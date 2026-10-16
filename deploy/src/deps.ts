import type { Config } from "./config.ts";
import type { Sql } from "./store.ts";

/** Everything with a side effect, so tests can swap the network, the database and the clock. */
export interface Deps {
  readonly sql: Sql;
  readonly fetch: typeof fetch;
  readonly now: () => number;
  readonly config: Config;
}
