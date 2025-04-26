#!/usr/bin/env bash
# Script to create llm.txt in the current directory
set -euo pipefail

OUTPUT="llm.txt"
echo "# LLM input file" > "$OUTPUT"
echo "" >> "$OUTPUT"
find . -maxdepth 1 -type f \( -name "*.py" -o -name "*.html" -o -name "*.js" \) | sort | while read -r file; do
  printf "\n===== %s =====\n" "$file" >> "$OUTPUT"
  cat "$file" >> "$OUTPUT"
done
echo "Created $OUTPUT at $(pwd)/$OUTPUT"
