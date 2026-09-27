#!/bin/sh
set -eu

for file in "$@"; do
  if [ -f "$file" ]; then
    echo "found: $file"
  fi
done
