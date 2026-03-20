# CLI Reference

현재 `heeupscale` CLI와 config 키를 LLM/에이전트가 빠르게 찾을 수 있게 정리한 요약입니다.

## Top-level commands

- `heeupscale init [DIR]`
- `heeupscale [OPTIONS] <INPUT>`

## Main command options

| Option | Meaning |
| --- | --- |
| `--json` | 단일 JSON 결과 출력 |
| `--config <PATH>` | 명시적 config 파일 |
| `--output <PATH>` | 출력 파일 경로 |
| `--scale <FACTOR>` | 업스케일 배율 |
| `--crf <0-51>` | FFmpeg/Realesrgan 재조립 화질 |
| `--preset <PRESET>` | FFmpeg/Realesrgan 재조립 preset |
| `--scaler <lanczos|spline|bicubic>` | FFmpeg scale kernel |
| `--profile <auto|scale-only|restore>` | FFmpeg 필터 프로필 |
| `--audio-bitrate-kbps <KBPS>` | AAC 비트레이트 |
| `--engine <auto|ffmpeg|fx-upscale|realesrgan>` | 처리 백엔드 |
| `--fx-upscale-bin <PATH>` | `fx-upscale` 바이너리 경로 |
| `--realesrgan-model <NAME>` | Real-ESRGAN 모델 |
| `--realesrgan-bin <PATH>` | `realesrgan-ncnn-vulkan` 경로 |
| `--realesrgan-model-path <PATH>` | Real-ESRGAN 모델 디렉터리 |
| `--realesrgan-tile <N>` | Real-ESRGAN tile |
| `--realesrgan-tta[=true|false]` | Real-ESRGAN TTA |
| `--overwrite[=true|false]` | 출력 덮어쓰기 |
| `--open[=true|false]` | 완료 후 IINA 열기 |
| `--dry-run` | 실제 실행 없이 계획만 출력 |
| `--ffmpeg-bin <PATH>` | `ffmpeg` 경로 |
| `--ffprobe-bin <PATH>` | `ffprobe` 경로 |

## Engine-specific notes

### `auto`

- 저해상도는 `fx-upscale` 우선
- `fx-upscale`가 없으면 `realesrgan`
- 둘 다 없으면 `ffmpeg`
- 큰 해상도는 기본적으로 `ffmpeg`

### `ffmpeg`

- `crf`, `preset`, `scaler`, `profile`, `audio_bitrate_kbps`가 직접 의미 있음

### `fx-upscale`

- `scale`과 `output`은 `heeupscale`가 계산
- 실제 외부 도구에는 width/height로 전달
- `crf`, `preset`, `profile`은 이 경로에서 직접 쓰이지 않음

### `realesrgan`

- 이미지 시퀀스로 추출 후 재조립
- `realesrgan_*` 옵션이 직접 의미 있음
- 재조립 단계에서 `crf`, `preset`, `audio_bitrate_kbps` 사용

## Config keys

`config.toml`에서는 `[heeupscale]` 아래 키를 사용:

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
```

기본 출력 폴더는 별도 키가 없으면 “입력 파일 옆”입니다.

선택적 키:

```toml
output_dir = "exports"
```

## Common command patterns

기본 실행:

```bash
heeupscale input.mp4
```

Dry run:

```bash
heeupscale input.mp4 --dry-run --open=false
```

JSON dry run:

```bash
heeupscale input.mp4 --json --dry-run --open=false
```

강제 `fx-upscale`:

```bash
heeupscale input.mp4 --engine fx-upscale
```

강제 `ffmpeg`:

```bash
heeupscale input.mp4 --engine ffmpeg --profile restore
```

출력 파일 지정:

```bash
heeupscale input.mp4 --output /path/to/out.mp4
```

## Current stdout contract

사람/LLM이 현재 안정적으로 기대할 수 있는 줄:

```text
config: <resolved config path>        # only when a config file was loaded
engine: <ffmpeg|fx-upscale|realesrgan>
input : <resolved input path>
output: <resolved output path>
video : <src_width>x<src_height> -> <dst_width>x<dst_height>
fps   : <frame rate>                  # when ffprobe returned it
pixfmt: <pixel format>                # when ffprobe returned it
scale : <scale label>
filter: <profile label>               # ffmpeg path
preset: <preset / crf>                # ffmpeg or realesrgan remux path
codec : h264 (fx-upscale)             # fx-upscale path
cmd   : <single command>              # ffmpeg path
cmd1  : <first command>               # multi-step path
cmd2  : <second command>
cmd3  : <third command>
done: <final output path>             # only after success
opened in IINA                        # only when --open succeeded
```

의미상 중요한 판정:

- `done:`이 있으면 성공
- 종료 코드가 0이 아니면 실패
- `engine:`은 config 값이 아니라 최종 선택 결과

## Current JSON contract

`--json`을 주면 호출당 JSON 객체 하나만 출력합니다.

공통 필드:

```text
mode
status
```

업스케일 호출:

- `status = "planned"`: `--dry-run`
- `status = "completed"`: 실제 실행 성공
- `status = "error"`: 실패

추가 주요 필드:

```text
config_path
requested_engine
selected_engine
input_path
output_path
source
target
resolved_options
plan.commands
opened_in_iina
error
```

초기화 호출:

- `heeupscale init --json`
- `status = "created" | "updated" | "exists"`
- `path`, `next` 포함
