#!/usr/bin/env bash
set -euo pipefail

API="${API:-http://localhost:8080}"

echo "==> Health"
curl -s "$API/health" | jq .

echo ""
echo "==> Register tenant"
TENANT=$(curl -s -X POST "$API/tenants" \
  -H "Content-Type: application/json" \
  -d '{"name":"Demo Tenant"}')
echo "$TENANT" | jq .
TID=$(echo "$TENANT" | jq -r '.data.id')

echo ""
echo "==> Deploy contract"
DEPLOY=$(curl -s -X POST "$API/tenants/$TID/deploy" \
  -H "Content-Type: application/json" \
  -d '{"wasm_path":"./simple_storage.wasm","constructor":"new","args":{"initial_value":42}}')
echo "$DEPLOY" | jq .
ADDR=$(echo "$DEPLOY" | jq -r '.data.contract_address')

echo ""
echo "==> Call get"
curl -s -X POST "$API/tenants/$TID/call" \
  -H "Content-Type: application/json" \
  -d "{\"contract_address\":\"$ADDR\",\"method\":\"get\",\"args\":{}}" | jq .

echo ""
echo "==> Call set"
curl -s -X POST "$API/tenants/$TID/call" \
  -H "Content-Type: application/json" \
  -d "{\"contract_address\":\"$ADDR\",\"method\":\"set\",\"args\":{\"new_value\":777}}" | jq .

echo ""
echo "==> Call get again"
curl -s -X POST "$API/tenants/$TID/call" \
  -H "Content-Type: application/json" \
  -d "{\"contract_address\":\"$ADDR\",\"method\":\"get\",\"args\":{}}" | jq .

echo ""
echo "Demo finished."
