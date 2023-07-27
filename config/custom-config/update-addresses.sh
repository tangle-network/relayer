#!/bin/bash

for kind in "VAnchor" "SignatureBridge"
do
  opts='.evm | .[] | .contracts | .[] | select(.contract == "'$kind'") | .address'
  old_address=$(jq --raw-output "$opts" config.json | head -n1)
  read -p "New $kind Address: " new_address
  sed -i'' -e "s/$old_address/$new_address/g" config.json
  echo "Updated $kind address from $old_address to $new_address"
done