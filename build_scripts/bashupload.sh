#!/usr/bin/env bash
set -euo pipefail

if (( $# != 1 )); then
  echo "Usage: bashupload <file>" >&2
  exit 1
fi

file="$1"
if [[ ! -f $file ]]; then
  echo "Error: '$file' not found" >&2
  exit 1
fi

fname=$(basename "$file")
enc_fname=$(printf %s "$fname" | jq -sRr @uri)
response=$(curl --silent --show-error --upload-file "$file" "https://bashupload.com/$enc_fname")

url=$(awk '/wget/ { print $2; exit }' <<< "$response")

if [[ -z "$url" ]]; then
  echo "Upload failed: $response" >&2
  exit 1
fi

echo "$url"