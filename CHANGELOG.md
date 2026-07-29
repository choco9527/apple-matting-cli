# Changelog

All notable changes to this project will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
