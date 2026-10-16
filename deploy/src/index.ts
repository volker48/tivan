import { parseConfig } from "./config.ts";
import { apiError } from "./http.ts";
import { logEvent } from "./log.ts";
import { handle } from "./routes.ts";
import { d1Sql } from "./store.ts";

export default {
  async fetch(request, env): Promise<Response> {
    const parsed = parseConfig(env);
    if (parsed.kind === "invalid") {
      logEvent("misconfigured", { problems: parsed.problems.join("; ") });
      return apiError(500, "misconfigured");
    }
    return handle(request, {
      sql: d1Sql(env.DB),
      fetch: (input, init) => fetch(input, init),
      now: () => Date.now(),
      config: parsed.config,
    });
  },
} satisfies ExportedHandler<Env>;
