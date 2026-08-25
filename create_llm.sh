#!/usr/bin/env bash
# Script to create llm.txt in the current directory
set -euo pipefail

OUTPUT="llm.txt"
echo "# LLM input file" > "$OUTPUT"
echo "" >> "$OUTPUT"
{
  find . -maxdepth 1 -type f \( -name "*.html" -o -name "*.js" -o -name "*.css" -o -name "*.toml" \)
  find crates scripts -type f \( -name "*.rs" -o -name "*.toml" -o -name "*.sh" \)
} | sort | while read -r file; do
  printf "\n===== %s =====\n" "$file" >> "$OUTPUT"
  cat "$file" >> "$OUTPUT"
done
echo "Created $OUTPUT at $(pwd)/$OUTPUT"
