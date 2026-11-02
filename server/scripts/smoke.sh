#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
PORT=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')
TOKEN=smoke-test-token
cargo build --quiet
MUSIC_TOKEN=$TOKEN MUSIC_PORT=$PORT ./target/debug/music-server > /tmp/music-smoke.log 2>&1 &
PID=$!
trap 'kill $PID 2>/dev/null || true; wait $PID 2>/dev/null || true' EXIT
BASE="http://127.0.0.1:$PORT"
for i in $(seq 1 50); do curl -sf "$BASE/api/health" >/dev/null 2>&1 && break; sleep 0.2; done
echo "health: $(curl -sf $BASE/api/health)"
CODE=$(curl -s -o /dev/null -w '%{http_code}' "$BASE/api/tracks"); echo "no-token status: $CODE"; [ "$CODE" = 401 ]
TRACKS=$(curl -sf -H "Authorization: Bearer $TOKEN" "$BASE/api/tracks"); echo "tracks: $TRACKS" | head -c 300; echo
ID=$(printf '%s' "$TRACKS" | python3 -c 'import sys,json; print(json.load(sys.stdin)[0]["id"])')
FULL=$(curl -sf -H "Authorization: Bearer $TOKEN" "$BASE/api/tracks/$ID/stream" -o /tmp/music-smoke.wav -w '%{http_code} %{size_download}'); echo "full stream: $FULL"
RANGED=$(curl -sf -H "Authorization: Bearer $TOKEN" -H 'Range: bytes=0-99' "$BASE/api/tracks/$ID/stream" -o /dev/null -w '%{http_code} %{size_download}'); echo "ranged stream: $RANGED"; [ "$RANGED" = "206 100" ]
BAD=$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $TOKEN" -H 'Range: bytes=999999999-' "$BASE/api/tracks/$ID/stream"); echo "bad range status: $BAD"; [ "$BAD" = 416 ]
echo "SMOKE OK"
