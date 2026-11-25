#!/usr/bin/env bash
# Run all fuzz targets in parallel for a fixed time budget.
#
#   fuzz/run.sh            # 2 hours (7200 s)
#   fuzz/run.sh 600        # 10 minutes
#   JOBS=8 fuzz/run.sh     # workers for each stateful target (default: (CPUs - 2) / 2)
#
# Crashes land in fuzz/artifacts/<target>/. Reproduce one with:
#   cargo +nightly fuzz run <target> fuzz/artifacts/<target>/crash-...
# and minimise it with:
#   cargo +nightly fuzz tmin <target> fuzz/artifacts/<target>/crash-...
set -euo pipefail

cd "$(dirname "$0")/.."
SECS=${1:-7200}
# amount_parse saturates in seconds, so it gets 2 workers and the rest go to
# the two stateful targets.
JOBS=${JOBS:-$(( ($(nproc) - 2) / 2 > 0 ? ($(nproc) - 2) / 2 : 1 ))}
LOGS=fuzz/logs
mkdir -p "$LOGS"

# Copy the seeds into the working corpus; libFuzzer adds what it finds there.
for t in csv_pipeline amount_parse engine_model; do
  mkdir -p "fuzz/corpus/$t"
  [ -d "fuzz/seeds/$t" ] && cp -n fuzz/seeds/$t/* "fuzz/corpus/$t/" 2>/dev/null || true
done

cargo +nightly fuzz build

run() {
  local target=$1 jobs=$2; shift 2
  echo "==> $target: $jobs workers for ${SECS}s, log in $LOGS/$target.log"
  # -close_fd_mask=2 silences the pipeline's malformed-line warnings on stderr.
  cargo +nightly fuzz run "$target" -- \
    -fork="$jobs" -ignore_crashes=1 -max_total_time="$SECS" \
    -close_fd_mask=2 -print_final_stats=1 "$@" \
    >"$LOGS/$target.log" 2>&1 &
}

run engine_model "$JOBS" -max_len=65536
run csv_pipeline "$JOBS" -max_len=8192 -dict=fuzz/csv.dict
run amount_parse 2 -max_len=64 -dict=fuzz/csv.dict

wait
echo
echo "Done. Crashes found:"
for t in engine_model csv_pipeline amount_parse; do
  n=$(find "fuzz/artifacts/$t" -type f 2>/dev/null | wc -l)
  echo "  $t: $n"
done
