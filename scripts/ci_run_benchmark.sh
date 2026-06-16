#!/usr/bin/env bash
set -Eeuo pipefail

# Runs `canbench` in a given directory and produces a comment intended to be
# posted on the pull request. Adapted from dfinity/dex.
#
# The job passes/fails on whether the committed `canbench_results.yml` is up to
# date: any regression, improvement, or new benchmark means it is stale and must
# be refreshed with `canbench --persist` and committed. This keeps the recorded
# instruction counts always matching the code, so performance changes show up in
# the diff and (via the baseline-branch comparison below) in a PR comment.

if [ $# -lt 2 ]; then
    echo "Usage: $0 <canister_path> <job_name>"
    exit 1
fi

# Path the canbench.yml lives in (e.g. src/xrc).
CANISTER_PATH=$1

# The name of the CI job (used to namespace artifacts).
CANBENCH_JOB_NAME=$2

# Must match the file path specified in the GitHub Action.
COMMENT_MESSAGE_PATH=/tmp/canbench_result_${CANBENCH_JOB_NAME}.md

# GitHub CI is expected to have the baseline branch checked out in this folder.
BASELINE_BRANCH_DIR=_canbench_baseline_branch

CANBENCH_OUTPUT=/tmp/canbench_output_${CANBENCH_JOB_NAME}.txt

CANBENCH_RESULTS_FILE="$CANISTER_PATH/canbench_results.yml"
BASELINE_BRANCH_RESULTS_FILE="$BASELINE_BRANCH_DIR/$CANBENCH_RESULTS_FILE"

CANBENCH_RESULTS_CSV_FILE="/tmp/canbench_results_${CANBENCH_JOB_NAME}.csv"

# Install canbench, pinned to 0.4.1. Check the exact version rather than just
# that some canbench is on PATH: a cached or preinstalled build of a different
# version could change the output format or the measured instruction counts,
# which would silently skew the regression gate.
CANBENCH_VERSION=0.4.1
if ! canbench --version 2>/dev/null | grep -qw "$CANBENCH_VERSION"; then
    cargo install --version "$CANBENCH_VERSION" --locked --force canbench
fi

# Verify that the canbench results file exists.
if [ ! -f "$CANBENCH_RESULTS_FILE" ]; then
    echo "$CANBENCH_RESULTS_FILE not found. Did you forget to run \`canbench --persist [--csv]\`?"
    exit 1
fi

# Checks whether the benchmark output reports any change versus the committed
# results (streamed "(regressed/improved/new)" markers or the --show-summary
# "status:" lines).
has_updates() {
  local patterns=(
    "regressed by"
    "improved by"
    "\(new\)"
    "status:[[:space:]]+Regressions"
    "status:[[:space:]]+Improvements"
    "status:[[:space:]]+New[[:space:]]+benchmarks"
  )
  local all_patterns
  all_patterns=$(IFS='|'; echo "${patterns[*]}")
  grep -qE "$all_patterns" "$CANBENCH_OUTPUT"
}

# Check whether the committed results file is up to date with the current code.
pushd "$CANISTER_PATH" >/dev/null
canbench --less-verbose --hide-results --show-summary --csv --persist > "$CANBENCH_OUTPUT"
cp "./canbench_results.csv" "$CANBENCH_RESULTS_CSV_FILE"
if has_updates; then
  UPDATED_MSG="**❌ \`$CANBENCH_RESULTS_FILE\` is not up to date**
  If the performance change is expected, run \`canbench --persist [--csv]\` to update the benchmark results."
  echo "EXIT_STATUS=1" >> "$GITHUB_ENV"
else
  UPDATED_MSG="✅ \`$CANBENCH_RESULTS_FILE\` is up to date"
  echo "EXIT_STATUS=0" >> "$GITHUB_ENV"
fi
popd >/dev/null

commit_hash=$(git rev-parse HEAD)
time=$(date -u +"%Y-%m-%d %H:%M:%S UTC")

echo "# \`canbench\` 🏋 (dir: $CANISTER_PATH) $commit_hash $time" > "$COMMENT_MESSAGE_PATH"

# Compute the delta versus the baseline branch (for the PR comment).
if [ -f "$BASELINE_BRANCH_RESULTS_FILE" ]; then
  cp "$BASELINE_BRANCH_RESULTS_FILE" "$CANBENCH_RESULTS_FILE"
  pushd "$CANISTER_PATH" >/dev/null
  canbench --less-verbose --hide-results --show-summary --csv > "$CANBENCH_OUTPUT"
  cp "./canbench_results.csv" "$CANBENCH_RESULTS_CSV_FILE"
  popd >/dev/null
fi

CSV_RESULTS_FILE_MSG="📦 \`canbench_results_$CANBENCH_JOB_NAME.csv\` available in [artifacts](${GITHUB_SERVER_URL}/${GITHUB_REPOSITORY}/actions/runs/${GITHUB_RUN_ID})"

{
  echo "$UPDATED_MSG"
  echo "$CSV_RESULTS_FILE_MSG"
  echo ""
  echo "\`\`\`"
  cat "$CANBENCH_OUTPUT"
  echo "\`\`\`"
} >> "$COMMENT_MESSAGE_PATH"

cat "$COMMENT_MESSAGE_PATH"
