#!/bin/sh

# Ensure that curl is installed
if ! command -v curl >/dev/null 2>&1; then
  echo 'Error: curl is not installed.' >&2
  exit 1
fi

GIT_ROOT=$(git rev-parse --show-toplevel)
REMOTE_URL="https://dapp-fixtures.s3.amazonaws.com/develop"
DEST_DIR="${GIT_ROOT}/tests/solidity-fixtures"

download_file() {
  local url=$1
  local dest=$2
  curl -L -o "$dest" "$url"
}

make_directory() {
  local dir=$1
  mkdir -p "$dir"
}

files=(
  # 2x2
  "vanchor_2/2/witness_calculator.cjs"
  "vanchor_2/2/poseidon_vanchor_2_2.wasm"
  "vanchor_2/2/circuit_final.zkey"
  # 2x8
  "vanchor_2/8/witness_calculator.cjs"
  "vanchor_2/8/poseidon_vanchor_2_8.wasm"
  "vanchor_2/8/circuit_final.zkey"
  # 16x2
  "vanchor_16/2/witness_calculator.cjs"
  "vanchor_16/2/poseidon_vanchor_16_2.wasm"
  "vanchor_16/2/circuit_final.zkey"
  # 16x8
  "vanchor_16/8/witness_calculator.cjs"
  "vanchor_16/8/poseidon_vanchor_16_8.wasm"
  "vanchor_16/8/circuit_final.zkey"
)

for file in "${files[@]}"; do
  dest="${DEST_DIR}/${file}"
  url="${REMOTE_URL}/${file}"
  make_directory "$(dirname "$dest")"
  echo "Downloading ${url} to ${dest}"
  download_file "$url" "$dest"
done