# ffx

`ffx` is an opinionated Rust CLI wrapper around FFmpeg for common media tasks. It keeps the command surface small, applies useful defaults, and still exposes enough control for real batch workflows.

## Features

- `compress` for H.264 MP4 output with CRF-based defaults
- `to-mp4` for smart remux-or-reencode conversion
- `gif` with `palettegen + paletteuse`
- `audio` extraction to `mp3` or `m4a`
- `thumb` extraction using FFmpeg's `thumbnail` filter
- `cut` for single or multi-segment trimming with optional merge
- Batch processing with configurable concurrency
- `--dry-run` support for inspecting generated FFmpeg commands
- Presets for `web`, `discord`, and `high-quality`

## Requirements

- Rust 1.85+ recommended for building
- `ffmpeg` and `ffprobe` available on `PATH`

## Install

```bash
cargo build --release
./target/release/ffx --help
```

## Command Overview

```bash
ffx compress input.mov
ffx to-mp4 clip.mkv
ffx gif clip.mp4 --from 00:00:02 --duration 3 --fps 15
ffx audio lecture.mp4 --format m4a
ffx thumb video.mp4
ffx cut input.mp4 --from 00:00:10 --to 00:00:30
```

## Usage

### Compress

Default behavior converts to H.264/AAC MP4, applies `CRF 23`, and scales down to a maximum height of `1080p`.

```bash
ffx compress input.mov
ffx compress *.mov --preset discord -j 4
ffx compress demo.mov --target-size-mb 20
ffx compress input.mov --output exports/final.mp4
```

Example generated command:

```bash
ffmpeg -hide_banner -y -i input.mov \
  -c:v libx264 -preset medium -crf 23 \
  -vf "scale='if(gt(ih,1080),trunc(iw*1080/ih/2)*2,iw)':'if(gt(ih,1080),1080,ih)'" \
  -c:a aac -b:a 128k -movflags +faststart input_compressed.mp4
```

### To MP4

`to-mp4` probes codecs first. If the input streams are MP4-compatible it remuxes with `-c copy`; otherwise it falls back to H.264/AAC re-encoding.

```bash
ffx to-mp4 clip.mkv
ffx to-mp4 clip.webm --reencode --preset high-quality
```

### GIF

`gif` uses FFmpeg's palette workflow for better quality than a naive one-pass GIF export.

```bash
ffx gif clip.mp4
ffx gif clip.mp4 --from 12 --duration 2.5 --fps 15 --width 640
```

Example generated command:

```bash
ffmpeg -hide_banner -y -ss 12 -i clip.mp4 -t 2.5 \
  -vf "fps=15,scale=640:-1:flags=lanczos,split[s0][s1],[s0]palettegen=stats_mode=full[p],[s1][p]paletteuse=dither=sierra2_4a" \
  clip.gif
```

### Audio

```bash
ffx audio video.mp4
ffx audio video.mp4 --format m4a --bitrate 256k
```

### Thumbnail

```bash
ffx thumb video.mp4
ffx thumb video.mp4 --at 00:00:05 --width 1920
```

### Cut

Single trim:

```bash
ffx cut input.mp4 --from 00:00:10 --to 00:00:20
```

Multi-segment merge:

```bash
ffx cut input.mp4 \
  --segment 00:00:10-00:00:20 \
  --segment 00:01:00-00:01:30
```

Split output instead of merge:

```bash
ffx cut input.mp4 \
  --segment 10-20 \
  --segment 60-90 \
  --split
```

When using multiple segments, `ffx`:

1. Extracts each range into a temporary directory.
2. Writes a concat list file.
3. Merges the parts with `ffmpeg -f concat`.
4. Falls back to re-encoding if stream copy fails and fallback is enabled.

## Dry Run

Inspect the FFmpeg commands without executing them:

```bash
ffx --dry-run compress input.mov
ffx --dry-run cut input.mp4 --segment 10-20 --segment 30-40
```

## Development

```bash
cargo fmt
cargo test
```
