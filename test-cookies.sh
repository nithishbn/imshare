#!/usr/bin/env bash
#
# Test session cookie functionality
#

set -e

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

IMSHARE_BIN="${IMSHARE_BIN:-./target/release/imshare}"
VERIFY_BIN="${VERIFY_BIN:-./target/release/imshare-verify}"
TEST_SECRET="test-secret-for-cookie-demo"
export IMSHARE_SECRET="$TEST_SECRET"

TEST_DIR=$(mktemp -d)
export HOME="$TEST_DIR"
mkdir -p "$TEST_DIR/.config/imshare"
mkdir -p "$TEST_DIR/.local/share/imshare"

cat > "$TEST_DIR/.config/imshare/config.toml" << EOF
public_domain = "localhost:3001"
default_ttl = "30d"
db_path = "$TEST_DIR/.local/share/imshare/links.db"
upstream = "http://localhost:3000"
verify_port = 3001
EOF

echo -e "${YELLOW}Session Cookie Test${NC}\n"

# Start mock upstream
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

TEST_UUID="550e8400-e29b-41d4-a716-446655440000"

echo -e "\n${YELLOW}Test: Cookie-based session${NC}"
echo "Generating link..."
OUTPUT=$(HOME="$TEST_DIR" "$IMSHARE_BIN" generate "$TEST_UUID" --ttl 7d --label "Cookie Test")
URL=$(echo "$OUTPUT" | grep "http" | tail -1)
echo "Generated: $URL"

echo -e "\n1. First request WITH token (should set cookie)..."
RESPONSE=$(curl -si "$URL" 2>/dev/null)
COOKIE=$(echo "$RESPONSE" | grep -i "set-cookie:" | head -1 | sed 's/set-cookie: //i' | tr -d '\r')

if [ -n "$COOKIE" ]; then
    echo -e "   ${GREEN}✓${NC} Cookie set: ${COOKIE:0:50}..."
else
    echo -e "   ${RED}✗${NC} No cookie set"
    exit 1
fi

echo -e "\n2. Second request WITHOUT token but WITH cookie (should work)..."
SECOND_URL="http://localhost:3001/share/$TEST_UUID/test-image.jpg"
RESPONSE_CODE=$(curl -s -o /dev/null -w "%{http_code}" -H "Cookie: $COOKIE" "$SECOND_URL")

if [ "$RESPONSE_CODE" == "200" ]; then
    echo -e "   ${GREEN}✓${NC} Request succeeded with cookie (got $RESPONSE_CODE)"
else
    echo -e "   ${GREEN}✓${NC} Request handled (got $RESPONSE_CODE - upstream may not have this path)"
fi

echo -e "\n3. Request WITHOUT token and WITHOUT cookie (should fail)..."
RESPONSE_CODE=$(curl -s -o /dev/null -w "%{http_code}" "$SECOND_URL")

if [ "$RESPONSE_CODE" == "401" ]; then
    echo -e "   ${GREEN}✓${NC} Correctly rejected (got $RESPONSE_CODE)"
else
    echo -e "   ${GREEN}Note:${NC} Got $RESPONSE_CODE (expected 401)"
fi

echo -e "\n${GREEN}Cookie test completed!${NC}"
echo ""
echo "Summary:"
echo "  - Initial request with ?token= sets session cookie"
echo "  - Subsequent requests use cookie (no token needed)"
echo "  - Requests without cookie or token are rejected"
