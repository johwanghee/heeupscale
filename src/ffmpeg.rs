use std::ffi::OsString;
use std::io::ErrorKind;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::planner::UpscalePlan;
use crate::progress::StageProgress;

#[derive(Debug, Clone)]
pub struct VideoMetadata {
    pub width: u32,
    pub height: u32,
    pub duration_seconds: Option<f64>,
    pub frame_rate_expr: Option<String>,
    pub frame_rate: Option<f64>,
    pub pixel_format: Option<String>,
}

pub fn ensure_tool(binary: &str, tool_name: &str) -> Result<()> {
    let status = Command::new(binary)
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    match status {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => bail!(
            "`{binary}` responded with status {status}. Check that `{tool_name}` is installed correctly."
        ),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            if matches!(tool_name, "ffmpeg" | "ffprobe") {
                bail!(
                    "`{binary}` was not found in PATH. On macOS install it with `brew install ffmpeg`."
                );
            }

            bail!("`{binary}` was not found in PATH.")
        }
        Err(error) => Err(error)
            .with_context(|| format!("failed to run `{binary}` to validate `{tool_name}`")),
    }
}

pub fn probe_video(ffprobe_bin: &str, input_path: &Path) -> Result<VideoMetadata> {
    let output = Command::new(ffprobe_bin)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,r_frame_rate,pix_fmt:format=duration",
            "-of",
            "json",
        ])
        .arg(input_path)
        .output()
        .with_context(|| format!("failed to run ffprobe for input `{}`", input_path.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "ffprobe failed for `{}`: {}",
            input_path.display(),
            stderr.trim()
        );
    }

    let response: FfprobeResponse = serde_json::from_slice(&output.stdout)
        .with_context(|| "failed to parse ffprobe json output".to_string())?;

    let stream = response
        .streams
        .into_iter()
        .next()
        .context("ffprobe returned no video stream")?;

    let width = stream.width.context("ffprobe did not return a width")?;
    let height = stream.height.context("ffprobe did not return a height")?;

    Ok(VideoMetadata {
        width,
        height,
        duration_seconds: response
            .format
            .and_then(|format| format.duration)
            .and_then(|value| value.parse().ok()),
        frame_rate_expr: stream.r_frame_rate.clone(),
        frame_rate: stream.r_frame_rate.as_deref().and_then(parse_frame_rate),
        pixel_format: stream.pix_fmt,
    })
}

pub fn run_ffmpeg(
    ffmpeg_bin: &str,
    overwrite: bool,
    plan: &UpscalePlan,
    quiet: bool,
) -> Result<()> {
    let mut command = Command::new(ffmpeg_bin);
    command.args(command_args(overwrite, plan));
    if quiet {
        command.stdout(Stdio::null()).stderr(Stdio::null());
    }

    let status = command
        .status()
        .with_context(|| format!("failed to spawn `{ffmpeg_bin}`"))?;

    if status.success() {
        return Ok(());
    }

    bail!("ffmpeg exited with status {status}");
}

pub fn run_ffmpeg_with_progress(
    ffmpeg_bin: &str,
    overwrite: bool,
    plan: &UpscalePlan,
    progress: &StageProgress,
    stage: u64,
    label: &str,
) -> Result<()> {
    run_args_with_progress(
        ffmpeg_bin,
        &command_args(overwrite, plan),
        progress,
        stage,
        label,
        plan.source.duration_seconds,
    )
}

pub fn open_in_iina(output_path: &Path) -> Result<()> {
    let status = Command::new("open")
        .arg("-a")
        .arg("IINA")
        .arg(output_path)
        .status()
        .with_context(|| "failed to run macOS `open` command".to_string())?;

    if status.success() {
        return Ok(());
    }

    bail!("failed to open `{}` in IINA", output_path.display());
}

pub fn render_command(ffmpeg_bin: &str, overwrite: bool, plan: &UpscalePlan) -> String {
    let mut parts = Vec::new();
    parts.push(shell_escape(ffmpeg_bin));

    for arg in command_args(overwrite, plan) {
        parts.push(shell_escape(&arg.to_string_lossy()));
    }

    parts.join(" ")
}

pub fn run_args(ffmpeg_bin: &str, args: &[OsString], quiet: bool) -> Result<()> {
    let mut command = Command::new(ffmpeg_bin);
    command.args(args);
    if quiet {
        command.stdout(Stdio::null()).stderr(Stdio::null());
    }

    let status = command
        .status()
        .with_context(|| format!("failed to spawn `{ffmpeg_bin}`"))?;

    if status.success() {
        return Ok(());
    }

    bail!("ffmpeg exited with status {status}");
}

pub fn run_args_with_progress(
    ffmpeg_bin: &str,
    args: &[OsString],
    progress: &StageProgress,
    stage: u64,
    label: &str,
    duration_seconds: Option<f64>,
) -> Result<()> {
    let mut command = Command::new(ffmpeg_bin);
    command
        .args(args)
        .arg("-nostats")
        .arg("-progress")
        .arg("pipe:2")
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .with_context(|| format!("failed to spawn `{ffmpeg_bin}`"))?;
    let stderr = child
        .stderr
        .take()
        .with_context(|| format!("failed to capture progress output from `{ffmpeg_bin}`"))?;

    progress.set(stage, label, 0.0);

    for line in BufReader::new(stderr).lines() {
        let line = line.with_context(|| format!("failed to read progress from `{ffmpeg_bin}`"))?;
        if line == "progress=end" {
            progress.set(stage, label, 1.0);
            continue;
        }

        if let Some(fraction) = parse_progress_fraction(&line, duration_seconds) {
            progress.set(stage, label, fraction);
        }
    }

    let status = child
        .wait()
        .with_context(|| format!("failed to wait for `{ffmpeg_bin}`"))?;

    if status.success() {
        progress.set(stage, label, 1.0);
        return Ok(());
    }

    bail!("ffmpeg exited with status {status}");
}

pub fn render_args(binary: &str, args: &[OsString]) -> String {
    let mut parts = Vec::with_capacity(args.len() + 1);
    parts.push(shell_escape(binary));

    for arg in args {
        parts.push(shell_escape(&arg.to_string_lossy()));
    }

    parts.join(" ")
}

pub fn extract_frames_args(
    overwrite: bool,
    input_path: &Path,
    output_pattern: &Path,
) -> Vec<OsString> {
    vec![
        OsString::from(if overwrite { "-y" } else { "-n" }),
        OsString::from("-hide_banner"),
        OsString::from("-i"),
        input_path.as_os_str().to_os_string(),
        OsString::from("-map"),
        OsString::from("0:v:0"),
        OsString::from("-vsync"),
        OsString::from("0"),
        output_pattern.as_os_str().to_os_string(),
    ]
}

pub struct AssembleVideoParams<'a> {
    pub overwrite: bool,
    pub frames_pattern: &'a Path,
    pub original_input: &'a Path,
    pub frame_rate_expr: &'a str,
    pub output_path: &'a Path,
    pub post_scale_filter: Option<&'a str>,
    pub preset: &'a str,
    pub crf: u8,
    pub audio_bitrate_kbps: u16,
}

pub fn assemble_video_args(params: &AssembleVideoParams<'_>) -> Vec<OsString> {
    let mut args = vec![
        OsString::from(if params.overwrite { "-y" } else { "-n" }),
        OsString::from("-hide_banner"),
        OsString::from("-framerate"),
        OsString::from(params.frame_rate_expr),
        OsString::from("-i"),
        params.frames_pattern.as_os_str().to_os_string(),
        OsString::from("-i"),
        params.original_input.as_os_str().to_os_string(),
        OsString::from("-map"),
        OsString::from("0:v:0"),
        OsString::from("-map"),
        OsString::from("1:a?"),
        OsString::from("-map_metadata"),
        OsString::from("1"),
    ];

    if let Some(filter) = params.post_scale_filter {
        args.push(OsString::from("-vf"));
        args.push(OsString::from(filter));
    }

    args.extend([
        OsString::from("-c:v"),
        OsString::from("libx264"),
        OsString::from("-preset"),
        OsString::from(params.preset),
        OsString::from("-crf"),
        OsString::from(params.crf.to_string()),
        OsString::from("-pix_fmt"),
        OsString::from("yuv420p"),
        OsString::from("-movflags"),
        OsString::from("+faststart"),
        OsString::from("-c:a"),
        OsString::from("aac"),
        OsString::from("-b:a"),
        OsString::from(format!("{}k", params.audio_bitrate_kbps)),
        params.output_path.as_os_str().to_os_string(),
    ]);

    args
}

fn command_args(overwrite: bool, plan: &UpscalePlan) -> Vec<OsString> {
    let mut args = Vec::new();

    args.push(OsString::from(if overwrite { "-y" } else { "-n" }));
    args.push(OsString::from("-hide_banner"));
    args.push(OsString::from("-i"));
    args.push(plan.input_path.clone().into_os_string());
    args.push(OsString::from("-vf"));
    args.push(OsString::from(plan.filter_graph.clone()));
    args.push(OsString::from("-map"));
    args.push(OsString::from("0:v:0"));
    args.push(OsString::from("-map"));
    args.push(OsString::from("0:a?"));
    args.push(OsString::from("-map_metadata"));
    args.push(OsString::from("0"));
    args.push(OsString::from("-c:v"));
    args.push(OsString::from("libx264"));
    args.push(OsString::from("-preset"));
    args.push(OsString::from(plan.preset.as_ffmpeg_value()));
    args.push(OsString::from("-crf"));
    args.push(OsString::from(plan.crf.to_string()));
    args.push(OsString::from("-pix_fmt"));
    args.push(OsString::from("yuv420p"));
    args.push(OsString::from("-movflags"));
    args.push(OsString::from("+faststart"));
    args.push(OsString::from("-c:a"));
    args.push(OsString::from("aac"));
    args.push(OsString::from("-b:a"));
    args.push(OsString::from(format!("{}k", plan.audio_bitrate_kbps)));
    args.push(plan.output_path.clone().into_os_string());

    args
}

fn shell_escape(value: &str) -> String {
    if value
        .chars()
        .all(|character| matches!(character, 'A'..='Z' | 'a'..='z' | '0'..='9' | '/' | '.' | '_' | '-' | ':' | '+' | '='))
    {
        return value.to_string();
    }

    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn parse_frame_rate(value: &str) -> Option<f64> {
    let (numerator, denominator) = value.split_once('/')?;
    let numerator: f64 = numerator.parse().ok()?;
    let denominator: f64 = denominator.parse().ok()?;

    if denominator == 0.0 {
        return None;
    }

    Some(numerator / denominator)
}

#[derive(Debug, Deserialize)]
struct FfprobeResponse {
    #[serde(default)]
    streams: Vec<FfprobeStream>,
    format: Option<FfprobeFormat>,
}

#[derive(Debug, Deserialize)]
struct FfprobeStream {
    width: Option<u32>,
    height: Option<u32>,
    r_frame_rate: Option<String>,
    pix_fmt: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FfprobeFormat {
    duration: Option<String>,
}

fn parse_progress_fraction(line: &str, duration_seconds: Option<f64>) -> Option<f64> {
    let duration_seconds = duration_seconds?;
    if duration_seconds <= 0.0 {
        return None;
    }

    if let Some(value) = line.strip_prefix("out_time=") {
        let seconds = parse_timestamp_seconds(value)?;
        return Some((seconds / duration_seconds).clamp(0.0, 1.0));
    }

    if let Some(value) = line.strip_prefix("out_time_us=") {
        let microseconds: f64 = value.parse().ok()?;
        return Some(((microseconds / 1_000_000.0) / duration_seconds).clamp(0.0, 1.0));
    }

    if let Some(value) = line.strip_prefix("out_time_ms=") {
        let microseconds: f64 = value.parse().ok()?;
        return Some(((microseconds / 1_000_000.0) / duration_seconds).clamp(0.0, 1.0));
    }

    None
}

fn parse_timestamp_seconds(value: &str) -> Option<f64> {
    let mut parts = value.trim().split(':');
    let hours: f64 = parts.next()?.parse().ok()?;
    let minutes: f64 = parts.next()?.parse().ok()?;
    let seconds: f64 = parts.next()?.parse().ok()?;
    Some(hours * 3600.0 + minutes * 60.0 + seconds)
}
