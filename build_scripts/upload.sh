#!/usr/bin/env bash
set -euo pipefail
if (( $# != 2 )); then
  echo "Usage: upload <bin> <file>" >&2
  exit 1
fi
bin="$1"
file="$2"
if [[ -z $bin ]]; then
  echo "Error: no bin to upload '$file' to" >&2
  exit 1
fi
if [[ ! -f $file ]]; then
  echo "Error: '$file' not found" >&2
  exit 1
fi

filename=$(basename "$file")
if [[ "$filename" =~ \.(exe|msi)$ ]]; then
  filename="${filename}_"
fi

# -f fails on an HTTP error, which -s would otherwise hide. Retries ride out filebin hiccups,
# now that a failed upload fails the build.
curl -fsS --retry 3 -o /dev/null -X POST \
  --data-binary @"$file" \
  -H "bin: $bin" \
  -H "filename: $filename" \
  https://filebin.net
