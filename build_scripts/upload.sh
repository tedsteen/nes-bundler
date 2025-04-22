#!/usr/bin/env bash
set -euo pipefail
if (( $# != 2 )); then
  echo "Usage: upload <bin> <file>" >&2
  exit 1
fi
bin="$1"
file="$2"
if [[ ! -f $file ]]; then
  echo "Error: '$file' not found" >&2
  exit 1
fi

filename=$(basename "$file")
resp=$(curl -s -X POST \
  --data-binary @"$file" \
  -H "bin: $bin" \
  -H "filename: $filename" \
  https://filebin.net)
