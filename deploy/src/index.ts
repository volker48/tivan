import { parseConfig } from "./config.ts";
import { apiError } from "./http.ts";
import { logEvent } from "./log.ts";
import { REMOTE_PREFIX } from "./remote-protocol.ts";
import { connectRemote } from "./remote-room.ts";
import { handle } from "./routes.ts";
import { d1Sql } from "./store.ts";

export { RemoteRoom } from "./remote-room.ts";

export default {
  async fetch(request, env): Promise<Response> {
    // The phone remote needs no Google configuration, so it works even where sign-in does not.
    const url = new URL(request.url);
    if (url.pathname.startsWith(REMOTE_PREFIX)) {
      return connectRemote(request, url, env.REMOTE_ROOMS);
    }
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
