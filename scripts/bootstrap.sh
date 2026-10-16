#!/bin/bash
set -euo pipefail

# Installs everything needed to build Tivan and verify it in a headless browser on a fresh
# Linux machine (e.g. a cloud environment setup script). The Rust version, components and wasm
# target come from rust-toolchain.toml. Needs Node.js with npm on the machine already, for
# agent-browser.

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# Keep in sync with the dioxus crate in Cargo.toml. Bumping it means replacing the sha256s below
# with the ones in that release's dx-<target>.sha256 assets.
dx_version="0.7.10"
# Keep in sync with the version the verify-tivan skill was proven with.
agent_browser_version="0.39.1"

export PATH="$HOME/.cargo/bin:$PATH"

if ! command -v rustup >/dev/null; then
	curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
		sh -s -- -y --profile minimal --default-toolchain none
fi

cd "$repo"
rustup toolchain install

if ! dx --version 2>/dev/null | grep -q "^dioxus $dx_version "; then
	# The release binary takes seconds; building dioxus-cli from source takes many minutes. The
	# Linux release binaries need glibc 2.39 or newer (Ubuntu 24.04).
	dx_target=""
	if [[ $(uname -s) == Linux ]] &&
		printf '2.39\n%s\n' "$(getconf GNU_LIBC_VERSION | cut -d' ' -f2)" | sort --version-sort --check=quiet; then
		case "$(uname -m)" in
		x86_64)
			dx_target="x86_64-unknown-linux-gnu"
			dx_sha256="4363e4ed2a3f1eb7f4d38d2d59aed59ce43271c44c16b425e92c89a64761fbe7"
			;;
		aarch64)
			dx_target="aarch64-unknown-linux-gnu"
			dx_sha256="8f1a17d3218700ffbe15e6540d936a178b2556fc801121a31082e3ba4ab9ef55"
			;;
		esac
	fi
	if [[ -n $dx_target ]]; then
		dx_tarball="$(mktemp)"
		curl --proto '=https' --tlsv1.2 -fsSL -o "$dx_tarball" \
			"https://github.com/DioxusLabs/dioxus/releases/download/v$dx_version/dx-$dx_target.tar.gz"
		echo "$dx_sha256  $dx_tarball" | sha256sum --check --quiet -
		tar -xzf "$dx_tarball" -C "$HOME/.cargo/bin" dx
		rm "$dx_tarball"
	else
		cargo install dioxus-cli --version "$dx_version" --locked
	fi
fi

if ! command -v npm >/dev/null; then
	echo "bootstrap.sh: npm is required to install agent-browser" >&2
	exit 1
fi
if ! agent-browser --version 2>/dev/null | grep -qx "agent-browser $agent_browser_version"; then
	# The postinstall script only marks the bundled native binary executable and points the
	# global bin at it, so it stays enabled.
	npm install --global "agent-browser@$agent_browser_version"
fi
# Downloads Chrome for Testing, and is a no-op once it is installed.
if [[ $(uname -s) == Linux ]]; then
	# --with-deps always runs apt-get through sudo, which root-only containers often lack.
	if [[ $EUID -eq 0 ]] && ! command -v sudo >/dev/null; then
		# The same order agent-browser tries its package managers in.
		if command -v apt-get >/dev/null; then
			apt-get update
			apt-get install --yes sudo
		elif command -v dnf >/dev/null; then
			dnf install --assumeyes sudo
		elif command -v yum >/dev/null; then
			yum install --assumeyes sudo
		else
			echo "bootstrap.sh: sudo is missing and none of apt-get, dnf or yum is available" >&2
			exit 1
		fi
	fi
	agent-browser install --with-deps
else
	agent-browser install
fi

cargo fetch --locked
