#!/bin/bash
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

(cd "$repo" && dx bundle --platform web --release >/dev/null)
pnpm --dir "$repo/deploy" install --frozen-lockfile
pnpm --dir "$repo/deploy" exec cf deploy "$@"
