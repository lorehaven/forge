#!/usr/bin/env fish
# Security gate for forge. The gate is a test run (`forge-sectest`'s `gate` and `live` tests) - one case
# per stage, so its report is a quench-test report: the one conveyor's nightly job keeps and shows.
#
#   scripts/security.fish
#   FORGE_SECTEST_TARGETS="warehouse=http://127.0.0.1:8080 gatehouse=http://127.0.0.1:8081" \
#       scripts/security.fish              # also scans those services (see tests/forge-sectest/tests/live.rs)
#
# Other FORGE_SECTEST_* variables (TOKEN, ACTIVE, FAIL_ON, BASELINE, RPS) are documented in live.rs.
# The report lands in target/security/index.html. Exit status is non-zero if any stage fails.

cd (dirname (status filename))/..

set -lx QUENCH_TEST_REPORT_DIR $PWD/target/security
rm -rf $QUENCH_TEST_REPORT_DIR

set -l failed 0
cargo test -p forge-sectest --test scanners; or set failed 1
cargo test -p forge-sectest --features gate --test gate -- --test-threads=1; or set failed 1
if test -n "$FORGE_SECTEST_TARGETS"
    cargo test -p forge-sectest --features gate --test live -- --test-threads=1; or set failed 1
else
    echo "live scan: skipped (set FORGE_SECTEST_TARGETS to scan a running estate)"
end

echo
echo "report: $QUENCH_TEST_REPORT_DIR/index.html"
test $failed -eq 0; and echo "security gate passed"; or begin
    echo "security gate FAILED"
    exit 1
end
