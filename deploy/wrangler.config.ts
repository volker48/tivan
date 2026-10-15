import { defineWranglerConfig } from "wrangler/experimental-config";

// The Dioxus release bundle, built by `scripts/deploy.sh`.
export default defineWranglerConfig({
  assetsDirectory: "../target/dx/tivan/release/web/public",
});
