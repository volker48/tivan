#!/bin/bash
# Sourced by the tivan-* helpers. Not run on its own.

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
runs="$repo/target/verify"

die() {
	echo "error: $*" >&2
	exit 1
}

# Loads RUN, PORT, URL, SESSION, EVIDENCE, STARTED and GIT_REV for the run named in $1.
load_run() {
	local run="${1:-}"
	[[ -n "$run" ]] || die "pass the run id that tivan-up printed (ls $runs to list runs)"
	local env="$runs/$run/run.env"
	[[ -f "$env" ]] || die "no run $run: $env is missing"
	# shellcheck source=/dev/null
	source "$env"
}

# The pid of the dx server this run started, or nothing once tivan-down has run.
server_pid() {
	cat "$EVIDENCE/server.pid" 2>/dev/null || true
}

# True when $1 is a live `dx serve` on this run's port. Guards every kill against a reused pid.
is_our_server() {
	local pid="$1"
	[[ -n "$pid" ]] || return 1
	ps -p "$pid" -o command= 2>/dev/null | grep -q -- "dx serve .*--port $PORT"
}

# Runs a command, killing it after $1 seconds (exit 142). macOS has no timeout(1). A page stuck in
# a loop makes every agent-browser call to its session block forever.
with_timeout() {
	perl -e 'alarm shift; exec @ARGV or die "exec $ARGV[0]: $!"' "$@"
}

# Calls agent-browser on this run's session with a 30 s limit.
ab() {
	with_timeout 30 agent-browser --session "$SESSION" "$@"
}

# Lists sessions without starting a browser. Any other `agent-browser --session X` command creates X.
session_listed() {
	with_timeout 10 agent-browser session list 2>/dev/null | grep -q -x "  $SESSION"
}

# The session's current page URL, or nothing when the page does not answer. Call only after
# session_listed, or this creates the session.
session_url() {
	with_timeout 10 agent-browser --session "$SESSION" get url 2>/dev/null || true
}

# The pid of the agent-browser daemon that owns this session. Killing it also stops its Chrome.
session_daemon_pid() {
	local pid
	pid="$(cat "$HOME/.agent-browser/$SESSION.pid" 2>/dev/null || true)"
	if [[ -n "$pid" ]] && ps -p "$pid" -o command= 2>/dev/null | grep -q agent-browser; then
		echo "$pid"
	fi
}
