#!/usr/local/bin/bash
# Headless CLAP plugin smoke test suite for maolan-engine OOP hosting.
# Uses maolan-test (no GUI, no TUI) against a CLAP plugin bundle.
#
# Usage: test-clap-plugins.sh <bundle.clap> [options]
#   DURATION_SECS=2    Duration per plugin (default: 2)
#   VERBOSE=1          Enable verbose output
#   SHOW_LOGS=1        Show logs for failed tests

set -euo pipefail

PLUGIN_BUNDLE="${1:-}"
TEST_BIN="/home/meka/repos/maolan/daw/target/debug/maolan-test"
DEVICE="/dev/dsp6"
DURATION_SECS="${DURATION_SECS:-2}"
VERBOSE="${VERBOSE:-}"
PASS=0
FAIL=0
SKIPPED=0

if [[ -z "$PLUGIN_BUNDLE" ]]; then
    echo "Usage: $0 <bundle.clap>"
    echo ""
    echo "Environment variables:"
    echo "  DURATION_SECS=N   Seconds to run each plugin (default: 2)"
    echo "  VERBOSE=1         Enable verbose maolan-test output"
    echo "  SHOW_LOGS=1       Show logs for failed tests"
    echo ""
    echo "Example:"
    echo "  $0 /path/to/clap-plugins.clap"
    exit 1
fi

if [[ ! -f "$PLUGIN_BUNDLE" ]]; then
    echo "ERROR: Plugin bundle not found: $PLUGIN_BUNDLE"
    exit 1
fi

if [[ ! -f "$TEST_BIN" ]]; then
    echo "ERROR: maolan-test binary not found: $TEST_BIN"
    echo "Build it first:"
    echo "  cd /home/meka/repos/maolan/daw"
    echo "  cargo build --bin maolan-test"
    exit 1
fi

# Plugin IDs from free-audio/clap-plugins source (headless build)
PLUGINS=(
    "com.github.free-audio.clap.gain"
    "com.github.free-audio.clap.synth"
    "com.github.free-audio.clap.latency"
    "com.github.free-audio.clap.dc-offset"
    "com.github.free-audio.clap.dc-offset-with-latency"
    "com.github.free-audio.clap.adsr"
    "com.github.free-audio.clap.svf"
    "com.github.free-audio.clap.char-check"
    "com.github.free-audio.clap.realtime-requirement"
    "com.github.free-audio.clap.transport-info"
    "com.github.free-audio.clap.track-info"
    "com.github.free-audio.clap.offline-latency"
    "com.github.free-audio.clap.gain-adjustment-metering"
    "com.github.free-audio.clap.mini-curve-display"
)

echo "========================================"
echo "Maolan CLAP Plugin Smoke Test Suite"
echo "========================================"
echo "Bundle:   $PLUGIN_BUNDLE"
echo "Device:   $DEVICE"
echo "Duration: ${DURATION_SECS}s per plugin"
echo "Plugins:  ${#PLUGINS[@]}"
echo ""

for PLUGIN_ID in "${PLUGINS[@]}"; do
    PLUGIN_PATH="${PLUGIN_BUNDLE}::${PLUGIN_ID}"
    PLUGIN_NAME="${PLUGIN_ID##*.}"
    printf "%-30s ... " "$PLUGIN_NAME"

    if $TEST_BIN \
        --plugin-path "$PLUGIN_PATH" \
        --device "$DEVICE" \
        --input-device "$DEVICE" \
        --duration-secs "$DURATION_SECS" \
        --sample-rate 48000 \
        --period-frames 1024 \
        --track-name "test_${PLUGIN_NAME}" \
        ${VERBOSE:+--verbose} \
        > "/tmp/maolan-test-${PLUGIN_NAME}.log" 2>&1; then
        echo "PASS"
        PASS=$((PASS + 1))
    else
        EXIT_CODE=$?
        echo "FAIL (exit $EXIT_CODE)"
        FAIL=$((FAIL + 1))
        if [[ -n "${SHOW_LOGS:-}" ]]; then
            echo "--- log begin ---"
            cat "/tmp/maolan-test-${PLUGIN_NAME}.log"
            echo "--- log end ---"
        fi
    fi
done

echo ""
echo "========================================"
echo "Results: $PASS passed, $FAIL failed, $SKIPPED skipped"
echo "========================================"

if (( FAIL > 0 )); then
    exit 1
fi
exit 0
