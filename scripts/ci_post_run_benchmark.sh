#!/usr/bin/env bash
set -Eeuo pipefail

# Fails the benchmark job if ci_run_benchmark.sh found the committed
# canbench_results.yml to be out of date. Adapted from dfinity/dex.

if [ -z "${EXIT_STATUS+x}" ]; then
  echo "EXIT_STATUS is not set."
  echo "The benchmark step may have exited before exporting its status."
  exit 1
fi

if [ "$EXIT_STATUS" -eq 1 ]; then
  echo "canbench_results.yml is not up to date."
  echo "If the performance change is expected, run '(cd src/xrc && canbench --persist --csv)' locally and commit the updated results."
  exit 1
fi
