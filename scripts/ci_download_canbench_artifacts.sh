#!/usr/bin/env bash
set -Eeuo pipefail

# Collects benchmark result artifacts into a JSON array for a GitHub Actions
# matrix that posts them as PR comments. Adapted from dfinity/dex.

matrix_json=$(
  python3 - <<'PY'
import glob
import json
import os

benchmarks = []

for directory in sorted(glob.glob("canbench_result_*")):
    if os.path.isdir(directory):
        result_path = os.path.join(directory, f"{directory}.md")
        if os.path.exists(result_path):
            with open(result_path, encoding="utf-8") as fh:
                benchmarks.append({
                    "title": directory,
                    "result": fh.read(),
                })

print(json.dumps({"benchmark": benchmarks}))
PY
)

# Output the benchmark matrix and PR number to be used by the next job.
echo "matrix=$matrix_json" >> "$GITHUB_OUTPUT"

# The pr_number artifact is written by the untrusted PR run, but this workflow
# posts comments with a write token. Only forward the value if it is a plain
# integer, so a tampered artifact cannot redirect the comment to an arbitrary PR.
if [ -f ./pr_number/pr_number ]; then
  pr_number=$(cat ./pr_number/pr_number)
  if [[ "$pr_number" =~ ^[0-9]+$ ]]; then
    echo "pr_number=$pr_number" >> "$GITHUB_OUTPUT"
  else
    echo "pr_number=" >> "$GITHUB_OUTPUT"
  fi
else
  echo "pr_number=" >> "$GITHUB_OUTPUT"
fi
