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

if [ -f ./pr_number/pr_number ]; then
  echo "pr_number=$(cat ./pr_number/pr_number)" >> "$GITHUB_OUTPUT"
else
  echo "pr_number=" >> "$GITHUB_OUTPUT"
fi
