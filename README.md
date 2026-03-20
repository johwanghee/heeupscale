# heeupscale

`heeupscale` is a Rust single-binary CLI that wraps `ffmpeg` and optional AI backends to make video upscaling simpler and keep the output easy to play in IINA.

## Docs

- Human overview and quick start: `README.md`
- LLM/agent operation guide: `docs/LLM_GUIDE.md`
- Option and config summary: `docs/CLI_REFERENCE.md`
- Repository work rules: `AGENTS.md`

## Current MVP

- Reads source video metadata with `ffprobe`
- Upscales with the `scale` filter and a selectable kernel like `lanczos`
- Can use `fx-upscale` on macOS Apple Silicon for AI video upscaling
- Encodes to an IINA-friendly `mp4` with `libx264 + aac + yuv420p`
- Can open the finished result directly in IINA

## Prerequisites

Install FFmpeg and IINA on macOS:

```bash
brew install ffmpeg
brew install fx-upscale
brew install --cask iina
```

## Build

```bash
cargo build --release
```

The binary will be created at `target/release/heeupscale`.

## Easiest Usage

With no config at all, just pass a file:

```bash
heeupscale input.mov
```

That uses built-in defaults:

- engine: `auto`
- scale: `2.0`
- crf: `20`
- preset: `slow`
- scaler: `lanczos`
- profile: `auto`
- fx-upscale binary: `fx-upscale`
- realesrgan model: `realesrnet-x4plus`
- audio: `aac 192k`
- output: `<same folder>/<name>_upscaled_2x.mp4`

## Project Config

If you want folder-level defaults, initialize the project once:

```bash
heeupscale init
```

That creates `./config.toml` in the current folder. After that, `heeupscale input.mov` will automatically use the project settings.
`init` now writes the full built-in default set explicitly so the generated file is transparent by itself.

You can also initialize another folder directly:

```bash
heeupscale init path/to/project
```

If you want `heeupscale 파일.mov`만으로 계속 같은 규칙이 적용되게 하려면, 프로젝트 폴더에 `config.toml`을 두면 됩니다.

`config.toml` example:

```toml
[heeupscale]
engine = "auto"
scale = 2.0
crf = 20
preset = "slow"
scaler = "lanczos"
profile = "auto"
fx_upscale_bin = "fx-upscale"
realesrgan_model = "realesrnet-x4plus"
realesrgan_bin = "realesrgan-ncnn-vulkan"
realesrgan_tile = 0
realesrgan_tta = false
audio_bitrate_kbps = 192
open = false
overwrite = false
ffmpeg_bin = "ffmpeg"
ffprobe_bin = "ffprobe"
# output_dir is unset by default, so the output is written next to the input file.
# output_dir = "exports"
```

Auto-discovery rules:

- `./config.toml` with a `[heeupscale]` section
- `./heeupscale.toml` with the same keys at top level
- parent directories are searched automatically
- `--config /path/to/config.toml` overrides discovery

Priority is:

- CLI option
- project config
- built-in default

With the sample config above, this is enough:

```bash
heeupscale input.mov
```

The result will be written next to the input file by default, and it will not open automatically unless you change `open = true` or set an `output_dir`.

`engine = "auto"` means:

- low-resolution sources use `fx-upscale` first if it is installed
- otherwise `realesrgan-ncnn-vulkan` is tried if it is available
- otherwise `ffmpeg` is used automatically
- `engine = "fx-upscale"` forces the Metal backend and errors if the binary is missing
- `engine = "realesrgan"` forces the AI backend and errors if the binary is missing

## More Examples

Choose output path and open in IINA after encode:

```bash
heeupscale input.mov --output out/movie-4k.mp4 --open
```

Tune quality and scaler:

```bash
heeupscale concert.mkv --scale 2 --crf 16 --preset slow --scaler lanczos
```

Disable the restoration pass and use plain scaling only:

```bash
heeupscale concert.mkv --profile scale-only
```

Force the AI backend:

```bash
heeupscale concert.mkv --engine realesrgan
```

Force the Metal backend on macOS:

```bash
heeupscale concert.mkv --engine fx-upscale
```

Anime source with the dedicated model:

```bash
heeupscale anime.mp4 --engine realesrgan --realesrgan-model realesr-animevideov3
```

Inspect the final FFmpeg command without running it:

```bash
heeupscale trailer.mov --scale 1.5 --dry-run
```

Emit a single JSON object for LLMs or automations:

```bash
heeupscale trailer.mov --json --dry-run
heeupscale trailer.mov --json --overwrite
heeupscale init --json
```

## CLI Reference

```text
heeupscale init [DIR]
heeupscale <INPUT>
  --json
  --config <PATH>
  --output <PATH>
  --engine <auto|ffmpeg|fx-upscale|realesrgan>
  --scale <FACTOR>
  --crf <0-51>
  --preset <PRESET>
  --scaler <lanczos|spline|bicubic>
  --profile <auto|scale-only|restore>
  --fx-upscale-bin <PATH>
  --realesrgan-model <realesr-animevideov3|realesrgan-x4plus|realesrgan-x4plus-anime|realesrnet-x4plus>
  --realesrgan-bin <PATH>
  --realesrgan-model-path <PATH>
  --realesrgan-tile <N>
  --realesrgan-tta
  --audio-bitrate-kbps <KBPS>
  --open
  --dry-run
  --overwrite
```

## Output Strategy

The default output is intentionally conservative for playback:

- Container: `mp4`
- Video: `libx264`
- Pixel format: `yuv420p`
- Audio: `aac`
- Fast start: enabled via `-movflags +faststart`

That combination is a good baseline for IINA, Finder previews, and general player compatibility.

## Notes On Quality

Plain FFmpeg scaling cannot invent new detail. `profile = "auto"` tries to make low-resolution sources look less harsh by applying a gentle denoise before scaling and a light sharpen after scaling. It usually helps more than pure scaling on old low-bitrate files, but it is still not an AI upscaler.

On Apple Silicon, `fx-upscale` is currently the preferred AI backend because it works directly on video and uses Metal. `heeupscale` runs it in a temporary workspace, then moves the resulting `mp4` to the final output path you requested.

If `fx-upscale` is not available, `heeupscale` can still call `realesrgan-ncnn-vulkan` as an external backend. That integration follows the official image-sequence usage: extract frames, run Real-ESRGAN on PNG frames, then assemble the video again with FFmpeg.

Install `fx-upscale` and `realesrgan-ncnn-vulkan` from their official sources:

- [fx-upscale GitHub](https://github.com/finnvoor/fx-upscale)
- [Homebrew Formula: fx-upscale](https://formulae.brew.sh/formula/fx-upscale)

- [Real-ESRGAN-ncnn-vulkan README](https://github.com/xinntao/Real-ESRGAN-ncnn-vulkan)
- [Real-ESRGAN-ncnn-vulkan Releases](https://github.com/xinntao/Real-ESRGAN-ncnn-vulkan/releases)

## Next Steps

Reasonable next features:

- target width or height presets like `1440p` and `2160p`
- optional HEVC output profile
- batch processing for a directory
- higher-quality pipelines using `zscale` or external AI upscalers
