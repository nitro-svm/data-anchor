#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

# (OPTIONAL) outputs JSON from the CLI so we can pipe into jq
export DATA_ANCHOR_OUTPUT="json"
export DATA_ANCHOR_PAYER="$HOME/.config/solana/id.json"
# your blob “namespace” (logical grouping, e.g. env.demo.cloudnetwork, where env: environment, demo: project name, cloudnetwork: target network)
export DATA_ANCHOR_NAMESPACE="<NAMESPACE>"
# (OPTIONAL) API key to authenticate certain requests against the indexer
export DATA_ANCHOR_INDEXER_API_TOKEN=""

# ─── 1. Initialize blober ─────────────────────────────────────────────────────
# Create the on-chain storage container (PDA) for our namespace
echo -e "\n1. Initializing Data Anchor blober"
data-anchor blober initialize

# ─── 2. Write dynamic JSON blob (can be skipped if you want to upload a static blob) ───────────────────────────────
# Build a JSON file with current rewards metrics
echo -e "\n2. Create rewards payload to tmp.json"
DATA_FILE=tmp.json
cat <<EOF >"$DATA_FILE"
{
  "epoch": 1042,
  "location": "Zug, Switzerland",
  "timestamp": "2025-07-25T17:09:00+08:00",
  "devices": [
    { "device_id": "sensor-001", "data_points": 340, "co2_ppm": 417, "reward": "0.03" },
    { "device_id": "sensor-002", "data_points": 327, "co2_ppm": 419, "reward": "0.02" }
  ],
  "total_reward": "0.05",
  "proof": "mock_zk_proof_here"
}
EOF
echo "  payload: $(cat "$DATA_FILE")"

# ─── 3. Upload blob ───────────────────────────────────────────────────────────
# Send the payload on-chain, capture its signature and ledger slot
echo -e "\n3. Uploading blob"
OUT=$(data-anchor blob upload --data-path "$DATA_FILE")
mapfile -t SIGNATURES < <(echo "$OUT" | jq -r '.signatures[]')
SLOT=$(echo "$OUT" | jq -r .slot)
echo "  signatures: ${SIGNATURES[*]}"
echo "  slot:       $SLOT"

# ─── 4. Fetch from ledger by signature ────────────────────────────────────────
# Retrieve via ledger and decode hex back to JSON
echo -e "\n4. Fetching from ledger (by signature)"
data-anchor blob fetch "${SIGNATURES[@]}" | jq -r '.[0].data' | xxd -r -p >fetched.json
echo "  fetched.json:"
cat fetched.json

if [[ $DATA_ANCHOR_INDEXER_API_TOKEN != "" ]]; then
  # ─── 5. Query via indexer by slot ─────────────────────────────────────────────
  # Query the indexer for metadata & raw data at that slot
  echo -e "\n5. Fetching blob via indexer"
  data-anchor indexer blobs "$SLOT" | jq .
else
  echo -e "\nSkipping step 5 because no DATA_ANCHOR_INDEXER_API_TOKEN is set"
fi

# ─── 6. Close blober ─────────────────────────────────────────────────────────
# Tear down the on-chain storage account to reclaim rent
echo -e "\n6. Closing Data Anchor blober"
data-anchor blober close

echo -e "\nAll done! You've successfully initialized a namespace → uploaded & verified a blob → queried the indexer → fetched a proof → closed the namespace. Try swapping in your own data and running on Mainnet!"
