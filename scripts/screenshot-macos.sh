#!/usr/bin/env bash
# Real-app screenshot on macOS: launch ezmark on a file, wait for it to render,
# capture the screen. Used by CI and by hand before each release (plan
# section 11). With a test-harness build the window stays open and on top
# (hold mode) and the script waits for the harness result line; with a
# production build it waits a fixed time.
#
#   scripts/screenshot-macos.sh target/release/ezmark doc.md shot.png
set -euo pipefail

binary="${1:?usage: $0 <binary> <file.md> <out.png>}"
file="${2:?usage: $0 <binary> <file.md> <out.png>}"
out="${3:?usage: $0 <binary> <file.md> <out.png>}"

log="$(mktemp)"
EZMARK_HARNESS_MODE=hold EZMARK_HARNESS_ID=screenshot "$binary" "$file" >"$log" 2>&1 &
pid=$!
trap 'kill "$pid" 2>/dev/null || true; rm -f "$log"' EXIT

for _ in $(seq 1 60); do
  if grep -q EZMARK_HARNESS_RESULT "$log"; then break; fi
  if ! kill -0 "$pid" 2>/dev/null; then
    echo "ezmark exited early:" >&2
    cat "$log" >&2
    exit 1
  fi
  sleep 0.5
done
sleep 2

screencapture -x "$out"
echo "saved $out"
