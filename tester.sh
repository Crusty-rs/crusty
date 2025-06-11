#!/usr/bin/env bash
# test_krust.sh - Enhanced testing suite for krust tool

set -euo pipefail
shopt -s inherit_errexit
set -o pipefail

# Global configuration
KRUST="${KRUST:-./target/release/krust}"
TEST_HOST="${TEST_HOST:-localhost}"
TEST_USER="${TEST_USER:-$USER}"
TEMP_DIR="$(mktemp -d)"
trap 'cleanup' EXIT INT TERM

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Test counters
TESTS_RUN=0
TESTS_PASSED=0

# Cleanup
cleanup() {
    rm -rf "$TEMP_DIR"
}

# Logging
log_info() {
    printf "%b%s%b\n" "$YELLOW" "$1" "$NC"
}

log_success() {
    printf "%b%s%b\n" "$GREEN" "$1" "$NC"
}

log_error() {
    printf "%b%s%b\n" "$RED" "$1" "$NC" >&2
}

# Validate environment
validate_environment() {
    if ! command -v jq >/dev/null; then
        log_error "jq is required but not found in PATH."
        return 1
    fi
    if [[ ! -x "$KRUST" ]]; then
        log_error "krust binary not found or not executable at: $KRUST"
        return 1
    fi
}

# Core test case
run_test() {
    local name="$1"
    local command="$2"
    local expected="${3:-0}"
    local output; local exit_code

    TESTS_RUN=$((TESTS_RUN + 1))
    printf "%s" "Testing: $name... "

    if output=$(eval "$command" 2>&1); then
        exit_code=0
    else
        exit_code=$?
    fi

    if [[ $exit_code -eq $expected ]]; then
        log_success "PASS"
        TESTS_PASSED=$((TESTS_PASSED + 1))
    else
        log_error "FAIL (expected $expected, got $exit_code)"
        printf "Output: %s\n" "$output"
    fi
}

# Parse JSON test
parse_json_test() {
    local description="$1"
    local command="$2"
    local jq_filter="$3"

    TESTS_RUN=$((TESTS_RUN + 1))
    local output
    if ! output=$(eval "$command" 2>/dev/null); then
        log_error "$description: FAIL (command error)"
        return
    fi

    if echo "$output" | jq -e "$jq_filter" >/dev/null 2>&1; then
        log_success "$description: PASS"
        TESTS_PASSED=$((TESTS_PASSED + 1))
    else
        log_error "$description: FAIL (jq filter failed)"
    fi
}

# Concurrency test
concurrency_test() {
    local start_time; local end_time; local duration
    TESTS_RUN=$((TESTS_RUN + 1))

    start_time=$(date +%s)
    if ! "$KRUST" --hosts "$(printf "$TEST_HOST,%.0s" {1..10} | sed 's/,$//')" --concurrency 5 'sleep 1' >/dev/null 2>&1; then
        log_error "Parallel execution: FAIL (command error)"
        return
    fi
    end_time=$(date +%s)
    duration=$((end_time - start_time))

    if [[ $duration -lt 5 ]]; then
        log_success "Parallel execution (${duration}s): PASS"
        TESTS_PASSED=$((TESTS_PASSED + 1))
    else
        log_error "Parallel execution (${duration}s): FAIL (too slow)"
    fi
}

# Main execution
main() {
    validate_environment || return 1

    printf "=== krust Test Suite ===\n"
    printf "Binary: %s\n" "$KRUST"
    printf "Test host: %s\n\n" "$TEST_HOST"

    run_test "Version flag" "$KRUST --version"
    run_test "Help flag" "$KRUST --help"
    run_test "No hosts error" "$KRUST echo test" 2

    run_test "Simple command" "$KRUST --hosts $TEST_HOST --user $TEST_USER echo OK"
    run_test "Command with args" "$KRUST --hosts $TEST_HOST 'echo hello world'"
    run_test "Multiple hosts" "$KRUST --hosts $TEST_HOST,$TEST_HOST hostname"
    run_test "Failed command" "$KRUST --hosts $TEST_HOST 'exit 42'" 1

    printf "%s\n" "$TEST_HOST" > "$TEMP_DIR/inventory.txt"
    run_test "Inventory file" "$KRUST --inventory $TEMP_DIR/inventory.txt whoami"

    parse_json_test "JSON output" "$KRUST --hosts $TEST_HOST --json echo test" '.hostname'
    parse_json_test "Pretty JSON" "$KRUST --hosts $TEST_HOST --pretty-json echo test" '.[0].hostname'
    parse_json_test "Field selection" "$KRUST --hosts $TEST_HOST --json --fields hostname,exit_code echo test" 'has("stdout") | not'

    run_test "Quick timeout" "$KRUST --hosts $TEST_HOST --timeout 1s sleep 10" 1
    run_test "Retry on failure" "$KRUST --hosts invalid.host.local --retries 1 --timeout 2s echo test" 1

    if [[ -f ~/.ssh/id_rsa ]]; then
        run_test "Key auth" "$KRUST --hosts $TEST_HOST --private-key ~/.ssh/id_rsa whoami"
    fi

    parse_json_test "stdout_lines parsing" "$KRUST --hosts $TEST_HOST --json 'printf \"line1\nline2\nline3\"'" '.stdout_lines | length == 3'
    parse_json_test "Duration tracking" "$KRUST --hosts $TEST_HOST --json 'sleep 0.1'" '.duration_ms > 100'

    concurrency_test

    parse_json_test "Stderr capture" "$KRUST --hosts $TEST_HOST --json 'echo error >&2; exit 1' || true" '.stderr | contains("error")'

    printf "\n=== Test Summary ===\n"
    printf "Tests run: %d\n" "$TESTS_RUN"
    printf "Tests passed: %b%d%b\n" "$GREEN" "$TESTS_PASSED" "$NC"
    printf "Tests failed: %b%d%b\n" "$RED" "$((TESTS_RUN - TESTS_PASSED))" "$NC"

    if [[ $TESTS_PASSED -eq $TESTS_RUN ]]; then
        printf "\n%bAll tests passed!%b\n" "$GREEN" "$NC"
        return 0
    else
        printf "\n%bSome tests failed!%b\n" "$RED" "$NC"
        return 1
    fi
}

main "$@"
