# apple-matting-cli

[中文说明](./README.md)

Local background removal for macOS, powered by Apple Vision and Core Image.
The project provides a one-shot CLI, a bounded-concurrency batch command, and a
small HTTP service for local integrations.
It also provides on-device OCR through Apple Vision.

## Requirements

- macOS 14.0 or later
- Apple Silicon or Intel Mac
- No cloud API or model download is required

Actual matting is macOS-only because it uses
`VNGenerateForegroundInstanceMaskRequest`.

## Install

### Homebrew

If Homebrew is not installed, use its official website:

<https://brew.sh/>

Install the CLI with:

```bash
brew install choco9527/tap/apple-matting-cli
```

This command automatically adds `choco9527/tap`, which is maintained by this
project and is not part of Homebrew Core.

### GitHub Release

Download the archive matching your Mac from the
[GitHub Releases](https://github.com/choco9527/apple-matting-cli/releases) page:

- `macos-arm64` for Apple Silicon
- `macos-x86_64` for Intel Macs

Verify the archive against `SHA256SUMS`, extract it, and place
`apple-matting-cli` in a directory on your `PATH`.

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
apple-matting-cli input.jpg --background white -o output.png
apple-matting-cli input.jpg --background "#FFCC00" -o output.png
```

When no output path is supplied, the result is written beside the input as
`input_nobg.png`. The background is transparent by default. `--background`
accepts `transparent`, `white`, `black`, or a quoted `#RRGGBB` color. All
results are PNG files. `--crop` trims the image to the detected foreground
bounds.

## Batch usage

```bash
apple-matting-cli --batch ./input -o ./output
apple-matting-cli --batch ./input -o ./output --recursive
apple-matting-cli --batch ./input -o ./output --crop --recursive --jobs 3
apple-matting-cli --batch ./input -o ./output --background white --jobs 3
```

Batch behavior:

- Supports JPG, JPEG, PNG, WEBP, and BMP input files.
- Requires a separate output directory and never writes into the input tree.
- Processes only the top level unless `--recursive` is supplied.
- Preserves relative subdirectories in recursive mode.
- Uses three workers by default; `--jobs` accepts values from 1 to 64.
- Applies one `--background` value to the entire batch; transparent is the default.
- Continues after individual image failures.
- Prints successful output paths to stdout and errors plus the final summary to stderr.
- Returns `0` when every image succeeds and `1` when any image fails.
- Rejects ambiguous inputs that map to the same PNG output path.
- Warns when an input exceeds 32 megapixels because very large images can cause
  substantial temporary memory pressure. Lower `--jobs` if the system becomes
  unresponsive, but note that image size is the dominant memory factor.

## OCR

OCR stays on the Mac and uses the Vision framework for images and PDFs. PDFs are
rendered and recognized page by page. It tries Simplified Chinese and English by
default, then writes `input_ocr.txt` next to the input file:

```bash
apple-matting-cli ocr input.png
apple-matting-cli ocr invoice.pdf
apple-matting-cli ocr input.png --lang zh-Hans,en-US
apple-matting-cli ocr input.png --format json -o result.json
```

`--lang` accepts comma-separated Apple language codes such as `zh-Hans`,
`zh-Hant`, `en-US`, and `ja-JP`. `--format text` (the default) writes plain text;
`--format json` writes the full text and a `text`, `confidence`, and `boundingBox`
for each recognized item. Coordinates are normalized Vision coordinates
`[x, y, width, height]` with their origin at the lower-left of the image. PDF
items additionally contain a one-based `page`; multi-page plain-text output uses
`Page N` separators.
To keep memory bounded, PDFs are limited to 200 pages and 32 megapixels per
rendered page. Pages are rendered, recognized, and released one at a time rather
than retaining every page bitmap in memory.

```json
{
  "text": "Example text",
  "items": [{
    "page": 1,
    "text": "Example text",
    "confidence": 0.98,
    "boundingBox": [0.1, 0.2, 0.4, 0.05]
  }]
}
```

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

Add `-F "crop=true"` to crop the response to the foreground bounds. Add
`-F "background=#FFFFFF"` to use a solid background. Successful responses use
`Content-Type: image/png`; invalid background values return HTTP `400`, and
matting failures return HTTP `422`.

OCR is available at `POST /ocr`, again using multipart field `file` for an image
or PDF:

```bash
curl -X POST -F "file=@input.png" -F "lang=zh-Hans,en-US" \
  -F "format=json" http://127.0.0.1:8080/ocr --output result.json
```

The default response is `text/plain; charset=utf-8`; `format=json` returns
`application/json; charset=utf-8`. An empty language value or invalid format returns
HTTP `400`; unsupported Vision language configurations and recognition failures return
HTTP `422`.

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
  apple-matting-cli <input-image> [-o|--output <output-png>] [--crop] [--background <color>]
  apple-matting-cli --batch <input-dir> -o <output-dir> [--crop] [--background <color>] [--recursive] [--jobs <count>]
  apple-matting-cli ocr <input-image> [-o|--output <output-file>] [--lang <codes>] [--format <text|json>]
  apple-matting-cli --server [--port <port>]
  apple-matting-cli --version
```

## License

Licensed under [GPL-3.0-only](./LICENSE).
