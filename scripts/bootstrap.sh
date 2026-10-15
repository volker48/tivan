#!/bin/bash
set -euo pipefail

# Installs everything needed to build Tivan on a fresh Linux machine (e.g. a cloud
# environment setup script). The Rust version, components and wasm target come from
# rust-toolchain.toml.

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# Keep in sync with the dioxus crate in Cargo.toml.
dx_version="0.7.10"

export PATH="$HOME/.cargo/bin:$PATH"

if ! command -v rustup >/dev/null; then
	curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
		sh -s -- -y --profile minimal --default-toolchain none
fi

cd "$repo"
rustup toolchain install

if ! dx --version 2>/dev/null | grep -q "^dioxus $dx_version "; then
	cargo install dioxus-cli --version "$dx_version" --locked
fi

cargo fetch --locked
