#!/usr/bin/env bash
#
# Test script for imshare verification scenarios
# Run this after building: cargo build --release
#

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
IMSHARE_BIN="${IMSHARE_BIN:-./target/release/imshare}"
VERIFY_BIN="${VERIFY_BIN:-./target/release/imshare-verify}"
TEST_SECRET="test-secret-for-verification-only-do-not-use-in-production"
export IMSHARE_SECRET="$TEST_SECRET"

# Test database and config
TEST_DIR=$(mktemp -d)
export HOME="$TEST_DIR"
mkdir -p "$TEST_DIR/.config/imshare"
mkdir -p "$TEST_DIR/.local/share/imshare"

# Create test config
cat > "$TEST_DIR/.config/imshare/config.toml" << EOF
public_domain = "localhost:3001"
default_ttl = "30d"
db_path = "$TEST_DIR/.local/share/imshare/links.db"
upstream = "http://localhost:3000"
verify_port = 3001
EOF

echo -e "${YELLOW}Starting test suite for imshare verification${NC}\n"

# Start a mock upstream server
echo "Starting mock upstream server on port 3000..."
python3 -m http.server 3000 --directory /tmp > /dev/null 2>&1 &
UPSTREAM_PID=$!
sleep 1

# Start imshare-verify
echo "Starting imshare-verify on port 3001..."
HOME="$TEST_DIR" "$VERIFY_BIN" > /dev/null 2>&1 &
VERIFY_PID=$!
sleep 2

cleanup() {
    echo -e "\nCleaning up..."
    kill $VERIFY_PID 2>/dev/null || true
    kill $UPSTREAM_PID 2>/dev/null || true
    rm -rf "$TEST_DIR"
}
trap cleanup EXIT

# Test helper function
run_test() {
    local test_name="$1"
    local expected_code="$2"
    local url="$3"

    echo -n "Test: $test_name... "

    response_code=$(curl -s -o /dev/null -w "%{http_code}" "$url")

    if [ "$response_code" == "$expected_code" ]; then
        echo -e "${GREEN}PASS${NC} (got $response_code)"
        return 0
    else
        echo -e "${RED}FAIL${NC} (expected $expected_code, got $response_code)"
        return 1
    fi
}

# Generate a test UUID
TEST_UUID="550e8400-e29b-41d4-a716-446655440000"

echo -e "\n${YELLOW}Test 1: Valid Token${NC}"
echo "Generating link with 7-day TTL..."
OUTPUT=$(HOME="$TEST_DIR" "$IMSHARE_BIN" generate "$TEST_UUID" --ttl 7d --label "Test Valid")
URL=$(echo "$OUTPUT" | grep "http" | tail -1)
echo "Generated: $URL"

run_test "Valid token should proxy successfully" "200" "$URL"

echo -e "\n${YELLOW}Test 2: Revoked Token${NC}"
echo "Generating link with 7-day TTL..."
OUTPUT=$(HOME="$TEST_DIR" "$IMSHARE_BIN" generate "$TEST_UUID" --ttl 7d --label "Test Revoked")
URL=$(echo "$OUTPUT" | grep "http" | tail -1)
LINK_ID=$(echo "$OUTPUT" | grep "Generated link" | grep -oP '#\K\d+')
echo "Generated link #$LINK_ID: $URL"

echo "Revoking link #$LINK_ID..."
HOME="$TEST_DIR" "$IMSHARE_BIN" revoke "$LINK_ID"

run_test "Revoked token should return 401" "401" "$URL"

echo -e "\n${YELLOW}Test 3: Database Failure (Fail Closed)${NC}"
echo "Generating valid link..."
OUTPUT=$(HOME="$TEST_DIR" "$IMSHARE_BIN" generate "$TEST_UUID" --ttl 7d --label "Test DB Failure")
URL=$(echo "$OUTPUT" | grep "http" | tail -1)
echo "Generated: $URL"

echo "Making database unreachable..."
chmod 000 "$TEST_DIR/.local/share/imshare/links.db"

run_test "Unreachable DB should return 503" "503" "$URL"

echo "Restoring database permissions..."
chmod 644 "$TEST_DIR/.local/share/imshare/links.db"

echo -e "\n${YELLOW}Test 4: Valid Token After DB Recovery${NC}"
run_test "Valid token should work after DB recovery" "200" "$URL"

echo -e "\n${GREEN}All automated tests completed!${NC}"
echo ""
echo "Note: Expired token testing requires manual steps."
echo "      See TESTING.md for comprehensive manual testing instructions,"
echo "      including testing expired tokens with short TTLs."
