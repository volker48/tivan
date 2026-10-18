import { bindings, defineConfig, exports } from "cf/config";
import * as entrypoint from "./src/index.ts" with { type: "cf-worker" };
import { LOCAL_D1_ID } from "./dev/local.ts";

// Secrets are set outside the repo: `.dev.vars` for `cf dev`, `--secrets-file` or
// `wrangler secret put` for a deployed Worker.
export default defineConfig(({ mode }) => ({
  worker: {
    name: "tivan",
    compatibilityDate: "2026-10-06",
    entrypoint,
    observability: { enabled: true },
    // Only the API and the sign-in flow run Worker code. Everything else stays a static asset.
    assets: { runWorkerFirst: ["/api/*", "/auth/*"] },
    // One room per phone remote pairing. It relays between the whiteboard and the phone.
    exports: {
      RemoteRoom: exports.durableObject({ storage: "sqlite" }),
    },
    env: {
      REMOTE_ROOMS: bindings.durableObject({ worker: "tivan", exportName: "RemoteRoom" }),
      DB: bindings.d1({
        name: "tivan",
        id: mode === "local" ? LOCAL_D1_ID : undefined,
      }),
      GOOGLE_CLIENT_ID: bindings.secret(),
      GOOGLE_CLIENT_SECRET: bindings.secret(),
      TOKEN_ENC_KEY: bindings.secret(),
      // Real Google endpoints. A local `.dev.vars` overrides them so `dev/fake-google.ts`
      // can stand in and the sign-in flow runs without a Google Cloud project.
      GOOGLE_AUTHORIZE_URL: bindings.text("https://accounts.google.com/o/oauth2/v2/auth"),
      GOOGLE_TOKEN_URL: bindings.text("https://oauth2.googleapis.com/token"),
      CLASSROOM_API_URL: bindings.text("https://classroom.googleapis.com"),
    },
  },
}));
