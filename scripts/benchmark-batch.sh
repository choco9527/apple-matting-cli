#!/usr/bin/env bash
set -euo pipefail

source_image="${1:?Usage: scripts/benchmark-batch.sh <source-image>}"
benchmark_count="${BENCHMARK_COUNT:-100}"
benchmark_size="${BENCHMARK_SIZE:-4000}"
benchmark_jobs="${BENCHMARK_JOBS:-3}"
benchmark_binary="${APPLE_MATTING_BENCH_BINARY:-target/release/apple-matting-cli}"

for value in "$benchmark_count" "$benchmark_size" "$benchmark_jobs"; do
  if [[ ! "$value" =~ ^[1-9][0-9]*$ ]]; then
    echo "Benchmark count, size, and jobs must be positive integers" >&2
    exit 2
  fi
done

if [[ ! -f "$source_image" ]]; then
  echo "Source image does not exist: $source_image" >&2
  exit 2
fi

if [[ ! -x "$benchmark_binary" ]]; then
  cargo build --release --locked --bin apple-matting-cli
fi

benchmark_root="$(mktemp -d /tmp/apple-matting-cli-benchmark.XXXXXX)"
trap 'rm -rf "$benchmark_root"' EXIT
input_dir="$benchmark_root/input"
output_dir="$benchmark_root/output"
scaled_image="$benchmark_root/source.png"
paths_file="$benchmark_root/paths.txt"

mkdir -p "$input_dir"
sips -z "$benchmark_size" "$benchmark_size" "$source_image" --out "$scaled_image"
for index in $(seq -w 1 "$benchmark_count"); do
  ln "$scaled_image" "$input_dir/image-$index.png"
done

echo "Running $benchmark_count image(s) at ${benchmark_size}x${benchmark_size}, jobs=$benchmark_jobs"
/usr/bin/time -l "$benchmark_binary" \
  --batch "$input_dir" \
  -o "$output_dir" \
  --jobs "$benchmark_jobs" \
  > "$paths_file"

output_count="$(find "$output_dir" -type f | wc -l | tr -d ' ')"
echo "Generated outputs: $output_count"
test "$output_count" = "$benchmark_count"
