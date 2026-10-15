#!/bin/bash
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# dx bundle leaves earlier hashed assets behind, and the deploy would upload them all.
rm -rf "$repo/target/dx/tivan/release/web/public"
(cd "$repo" && dx bundle --platform web --release >/dev/null)
pnpm --dir "$repo/deploy" install --frozen-lockfile
pnpm --dir "$repo/deploy" exec cf deploy "$@"
