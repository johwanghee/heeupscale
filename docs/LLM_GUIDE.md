# LLM Guide

이 문서는 사람보다 LLM/에이전트가 빠르게 읽고 `heeupscale`를 안정적으로 호출할 수 있게 만든 운영 가이드입니다.

## 먼저 읽을 순서

1. 이 문서 `docs/LLM_GUIDE.md`
2. 전체 옵션 요약 `docs/CLI_REFERENCE.md`
3. 실제 help 출력 `heeupscale --help`
4. 필요하면 대상 파일 기준 `heeupscale <input> --dry-run`

## 설치와 실행 확인

기본 확인 순서:

```bash
heeupscale --help
heeupscale init
heeupscale input.mp4 --dry-run
```

macOS Apple Silicon 권장 설치:

```bash
brew install ffmpeg
brew install fx-upscale
brew install --cask iina
```

## 명령 문법

기본 형태:

```bash
heeupscale [OPTIONS] <INPUT>
```

프로젝트 설정 초기화:

```bash
heeupscale init [DIR]
```

## 기본 동작

- 출력 파일 기본값은 입력 파일 옆의 `<name>_upscaled_<scale>x.mp4`
- 기본 `scale`은 `2.0`
- 기본 `engine`은 `auto`
- 기본 `open`은 `false`
- 기본 `overwrite`는 `false`
- `config.toml`이 없으면 내부 기본값만 사용

## 엔진 선택 규칙

`engine = "auto"`일 때:

1. 입력 영상이 저해상도(`<= 640x360`)면 `fx-upscale`를 먼저 시도
2. `fx-upscale`를 쓸 수 없으면 `realesrgan`
3. 그것도 안 되면 `ffmpeg`
4. 더 큰 해상도는 기본적으로 `ffmpeg`

강제 선택:

- `--engine fx-upscale`
- `--engine realesrgan`
- `--engine ffmpeg`

## 옵션 해석 규칙

항상 의미가 있는 값:

- `input`
- `output`
- `scale`
- `engine`
- `open`
- `overwrite`
- `config`

`ffmpeg` 경로에서 주로 쓰는 값:

- `crf`
- `preset`
- `scaler`
- `profile`
- `audio_bitrate_kbps`

`fx-upscale` 경로에서 주로 쓰는 값:

- `fx_upscale_bin`

`realesrgan` 경로에서 주로 쓰는 값:

- `realesrgan_model`
- `realesrgan_bin`
- `realesrgan_model_path`
- `realesrgan_tile`
- `realesrgan_tta`
- 재조립 인코딩 시 `crf`, `preset`, `audio_bitrate_kbps`

즉, `auto`로 저해상도 영상을 Apple Silicon에서 처리할 때는 `crf/preset/scaler/profile`이 실제 실행에서 쓰이지 않을 수 있습니다.

## 설정 파일 규칙

우선순위:

1. CLI 옵션
2. 프로젝트 config
3. 내부 기본값

자동 탐색 후보:

- 현재 폴더와 상위 폴더의 `config.toml`
- 현재 폴더와 상위 폴더의 `heeupscale.toml`
- 레거시 `hee-upscale.toml`

`config.toml`은 `[heeupscale]` 섹션만 읽고, `heeupscale.toml`은 top-level 키를 읽습니다.

## LLM 권장 절차

처음 이 CLI를 쓸 때 권장 순서는 아래와 같습니다.

1. `heeupscale --help`
2. 작업 폴더에 config가 있는지 확인
3. 없으면 `heeupscale init`
4. 실제 실행 전에는 필요하면 `--dry-run`
5. 출력 경로 충돌 가능성이 있으면 `--overwrite` 여부를 명시
6. macOS Apple Silicon 저해상도 소스는 우선 `auto` 유지
7. 백엔드를 고정해야 하면 `--engine fx-upscale` 또는 `--engine ffmpeg`

## 작업별 명령 매핑

프로젝트 초기화:

```bash
heeupscale init
```

파일 하나 바로 업스케일:

```bash
heeupscale input.mp4
```

출력 위치만 명시:

```bash
heeupscale input.mp4 --output /path/to/out.mp4
```

실행 전 계획만 확인:

```bash
heeupscale input.mp4 --dry-run --open=false
```

JSON 계획 확인:

```bash
heeupscale input.mp4 --json --dry-run --open=false
```

Metal 백엔드 강제:

```bash
heeupscale input.mp4 --engine fx-upscale
```

FFmpeg 백엔드 강제:

```bash
heeupscale input.mp4 --engine ffmpeg
```

기존 출력 덮어쓰기:

```bash
heeupscale input.mp4 --overwrite
```

## 출력 해석

`--dry-run` 또는 실제 실행 시 출력에서 중요하게 볼 줄:

- `config:` 실제로 읽힌 설정 파일
- `engine:` 최종 선택된 백엔드
- `video:` 입력 해상도 -> 출력 해상도
- `cmd:` 또는 `cmd1/cmd2/...` 실제 실행 계획
- `done:` 최종 출력 파일

## 실제 출력 예시

`--dry-run` 예시:

```text
config: /work/video-project/config.toml
engine: fx-upscale
input : /work/video-project/input.mp4
output: /work/video-project/input_upscaled_2x.mp4
video : 480x272 -> 960x544
fps   : 24.000
pixfmt: yuv420p
scale : 2x
codec : h264 (fx-upscale)
cmd1  : ln -s /work/video-project/input.mp4 '$TMPDIR/input.mp4'
cmd2  : fx-upscale '$TMPDIR/input.mp4' --width 960 --height 544 --codec h264
cmd3  : mv '$TMPDIR/input Upscaled.mp4' /work/video-project/input_upscaled_2x.mp4
```

실제 실행 예시:

```text
config: /work/video-project/config.toml
engine: fx-upscale
input : /work/video-project/input.mp4
output: /work/video-project/input_upscaled_2x.mp4
video : 480x272 -> 960x544
fps   : 24.000
pixfmt: yuv420p
scale : 2x
codec : h264 (fx-upscale)
cmd1  : ln -s /work/video-project/input.mp4 '$TMPDIR/input.mp4'
cmd2  : fx-upscale '$TMPDIR/input.mp4' --width 960 --height 544 --codec h264
cmd3  : mv '$TMPDIR/input Upscaled.mp4' /work/video-project/input_upscaled_2x.mp4
done: /work/video-project/input_upscaled_2x.mp4
```

의미:

- `engine`은 최종 선택 결과이며 config의 raw 값과 다를 수 있음
- `cmd` 계열은 실제 호출 계획
- `done`이 있으면 실행 완료로 해석
- `done`이 없고 프로세스가 오류로 끝났으면 실패로 해석

## JSON 출력 규칙

`--json`을 주면 호출당 JSON 객체 하나만 출력합니다.

상태 해석:

- `status = "planned"`: dry-run 결과
- `status = "completed"`: 실제 실행 성공
- `status = "error"`: 실행 실패
- `init --json`은 `created|updated|exists`

권장 사용:

- LLM/자동화는 `--json` 우선
- 사람이 직접 확인할 때는 기본 텍스트 출력 사용

대표 예시:

```bash
heeupscale input.mp4 --json --dry-run
heeupscale input.mp4 --json --overwrite
heeupscale init --json
```

## LLM이 피해야 할 실수

- `auto`인데 `realesrgan_model`이 보인다고 해서 항상 Real-ESRGAN이 실행된다고 가정하지 말 것
- `output_dir`가 없으면 기본 출력이 입력 파일 옆이라는 점을 놓치지 말 것
- `overwrite=false` 상태에서 기존 파일이 있으면 실패할 수 있다는 점을 반영할 것
- `fx-upscale` 경로에서 `crf/preset`이 적용된다고 설명하지 말 것
- 사용자가 명시적으로 원하지 않았는데 `--open`을 켜지 말 것
