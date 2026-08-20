# Changelog

All notable changes to this project will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- `--server` 默认改为只监听 `127.0.0.1`，不再写死 `0.0.0.0`；新增 `--bind <ip>` 参数。

## [0.3.0] - 2026-08-18

### Added

- On-device OCR for images and PDFs through `apple-matting-cli ocr`.
- Plain-text and JSON OCR output, including recognized text confidence, normalized
  bounding boxes, and PDF page numbers.
- Local multipart HTTP endpoint at `POST /ocr`.

### Changed

- Process PDF pages one at a time and enforce PDF page and pixel limits to keep
  OCR memory bounded.

## [0.2.0] - 2026-07-29

### Added

- Configurable transparent, named, and `#RRGGBB` solid backgrounds for single,
  batch, and HTTP matting.

## [0.1.0] - 2026-07-29

### Added

- Standalone macOS Vision background-removal CLI.
- Optional foreground-bound cropping with `--crop`.
- Batch directory processing with recursion and bounded concurrency.
- Local multipart HTTP endpoint at `POST /matting`.
- Chinese and English usage documentation.

### Changed

- Reuse a Core Image render context and drain autoreleased native objects after
  every image to keep long batch jobs memory-bounded.
- Warn when processing images above 32 megapixels.
