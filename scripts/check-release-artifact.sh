#!/usr/bin/env bash
# Inspect a built ezmark binary for test-harness pieces (plan section 11).
#
#   scripts/check-release-artifact.sh <binary> absent    # production build: none may appear
#   scripts/check-release-artifact.sh <binary> present   # harness build: all must appear
#
# The `present` run is the positive control: it proves these markers are
# visible in a binary that does contain the harness, so an `absent` pass means
# something. Markers cover the injection command and its siblings, their ACL
# entries, and the injected script (the only test asset compiled in).
set -euo pipefail

binary="${1:?usage: $0 <binary> absent|present}"
expect="${2:?usage: $0 <binary> absent|present}"

markers=(
  harness_inject
  harness_report
  harness_denied_probe
  allow-harness-inject
  allow-harness-report
  __EZMARK_HARNESS_CONFIG__
  securitypolicyviolation
  EZMARK_HARNESS_RESULT
)

failed=0
for marker in "${markers[@]}"; do
  if LC_ALL=C grep -a -F -q -- "$marker" "$binary"; then found=yes; else found=no; fi
  case "$expect:$found" in
    absent:no | present:yes) echo "ok      $marker ($found)" ;;
    *) echo "FAILED  $marker (found: $found, expected: $expect)"; failed=1 ;;
  esac
done

if [[ "$failed" -ne 0 ]]; then
  echo "artifact check failed for $binary" >&2
  exit 1
fi
echo "artifact check passed: harness markers $expect in $binary"
