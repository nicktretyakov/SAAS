#!/usr/bin/env bash
set -euo pipefail
API="${API:-http://127.0.0.1:8080}"

echo "== Register =="
REG=$(curl -s -X POST "$API/tenants" -H "Content-Type: application/json" -d '{"name":"AuthDemo"}')
echo "$REG" | head -c 400; echo
TID=$(echo "$REG" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
KEY=$(echo "$REG" | sed -n 's/.*"api_key":"\([^"]*\)".*/\1/p')
echo "TID=$TID KEY=$KEY"

echo "== Deploy with API key =="
curl -s -X POST "$API/tenants/$TID/deploy" \
  -H "Authorization: Bearer $KEY" \
  -H "Content-Type: application/json" \
  -d '{"wasm_path":"./c.wasm","constructor":"new","args":{}}'
echo

echo "== Usage =="
curl -s "$API/usage" -H "Authorization: Bearer $KEY"
echo

echo "== JWT =="
TOK=$(curl -s -X POST "$API/auth/token" -H "Content-Type: application/json" \
  -d "{\"tenant_id\":\"$TID\"}" | sed -n 's/.*"token":"\([^"]*\)".*/\1/p')
echo "token=${TOK:0:40}..."
curl -s "$API/usage" -H "Authorization: Bearer $TOK"
echo
