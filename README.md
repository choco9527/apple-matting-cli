# apple-matting-cli

[中文说明](./README.zh.md)

Local background removal for macOS, powered by Apple Vision and Core Image.
The project provides a one-shot CLI, a bounded-concurrency batch command, and a
small HTTP service for local integrations.

## Requirements

- macOS 14.0 or later
- Apple Silicon or Intel Mac
- No cloud API or model download is required

Actual matting is macOS-only because it uses
`VNGenerateForegroundInstanceMaskRequest`.

## Build from source

Install the Rust toolchain and Xcode Command Line Tools, then run:

```bash
cargo test --locked
cargo build --release --locked --bin apple-matting-cli
./target/release/apple-matting-cli --help
```

The release binary is written to `target/release/apple-matting-cli`.

## Single-image usage

```bash
apple-matting-cli input.jpg
apple-matting-cli input.jpg output.png
apple-matting-cli input.jpg -o output.png
apple-matting-cli input.jpg --output output.png
apple-matting-cli input.jpg --crop -o output.png
```

When no output path is supplied, the result is written beside the input as
`input_nobg.png`. All results are transparent PNG files. `--crop` trims the
image to the detected foreground bounds.

## Batch usage

```bash
apple-matting-cli --batch ./input -o ./output
apple-matting-cli --batch ./input -o ./output --recursive
apple-matting-cli --batch ./input -o ./output --crop --recursive --jobs 3
```

Batch behavior:

- Supports JPG, JPEG, PNG, WEBP, and BMP input files.
- Requires a separate output directory and never writes into the input tree.
- Processes only the top level unless `--recursive` is supplied.
- Preserves relative subdirectories in recursive mode.
- Uses three workers by default; `--jobs` accepts values from 1 to 64.
- Continues after individual image failures.
- Prints successful output paths to stdout and errors plus the final summary to stderr.
- Returns `0` when every image succeeds and `1` when any image fails.
- Rejects ambiguous inputs that map to the same PNG output path.
- Warns when an input exceeds 32 megapixels because very large images can cause
  substantial temporary memory pressure. Lower `--jobs` if the system becomes
  unresponsive, but note that image size is the dominant memory factor.

## Local HTTP service

Start the service:

```bash
apple-matting-cli --server --port 8080
```

Upload one image using multipart field `file`:

```bash
curl -X POST -F "file=@input.jpg" \
  http://127.0.0.1:8080/matting --output output.png
```

Add `-F "crop=true"` to crop the response to the foreground bounds. Successful
responses use `Content-Type: image/png`; matting failures return HTTP `422`.

The server listens on `0.0.0.0` and enables permissive CORS. It has no built-in
authentication, upload-size limit, rate limit, queue, or global concurrency
limit. Keep it on a trusted network or place it behind an authenticated proxy.

## Exit codes

| Code | Meaning |
| ---: | --- |
| `0` | Success |
| `1` | Matting, batch, file, or server failure |
| `2` | Invalid command-line arguments |

## Batch performance benchmark

Run the repeatable local benchmark before a release:

```bash
scripts/benchmark-batch.sh ./sample.png
```

It defaults to 100 images at 4000×4000 with three workers. Override the workload
with `BENCHMARK_COUNT`, `BENCHMARK_SIZE`, and `BENCHMARK_JOBS`. The script uses a
temporary directory, reports macOS timing and memory metrics, verifies the output
count, and removes all generated files when it exits.

## Supported commands

```text
Usage:
  apple-matting-cli <input-image> [-o|--output <output-png>] [--crop]
  apple-matting-cli --batch <input-dir> -o <output-dir> [--crop] [--recursive] [--jobs <count>]
  apple-matting-cli --server [--port <port>]
  apple-matting-cli --version
```

## Release status

The standalone repository is under active preparation. Tagged binary releases
and Homebrew installation will be enabled after the batch workflow and release
artifacts have completed validation.

## License

Licensed under [GPL-3.0-only](./LICENSE).
