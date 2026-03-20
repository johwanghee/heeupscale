use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use tempfile::TempDir;

use crate::config::Settings;
use crate::ffmpeg::{self, AssembleVideoParams, VideoMetadata};
use crate::planner::{Dimensions, UpscalePlan};

pub fn is_available(settings: &Settings) -> bool {
    smoke_test(settings).is_ok()
}

pub fn ensure_tool(settings: &Settings) -> Result<()> {
    if let Err(error) = Command::new(&settings.realesrgan_bin)
        .arg("-v")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        if error.kind() == ErrorKind::NotFound {
            bail!(
                "`{}` was not found in PATH. Download `realesrgan-ncnn-vulkan` from the official releases: https://github.com/xinntao/Real-ESRGAN-ncnn-vulkan/releases",
                settings.realesrgan_bin
            );
        }

        return Err(error).with_context(|| {
            format!(
                "failed to run `{}` to validate realesrgan",
                settings.realesrgan_bin
            )
        });
    }

    smoke_test(settings)
}

pub fn render_commands(
    settings: &Settings,
    plan: &UpscalePlan,
    source: &VideoMetadata,
) -> Vec<String> {
    let input_frames_pattern = PathBuf::from("$TMPDIR/input_frames/frame_%08d.png");
    let output_frames_pattern = PathBuf::from("$TMPDIR/output_frames/frame_%08d.png");
    let input_frames_dir = Path::new("$TMPDIR/input_frames");
    let output_frames_dir = Path::new("$TMPDIR/output_frames");
    let ai_scale = inference_scale_for(plan.scale_factor);
    let post_scale_filter = post_scale_filter(plan.ai_target, plan.target);

    let extract =
        ffmpeg::extract_frames_args(settings.overwrite, &plan.input_path, &input_frames_pattern);
    let upscale = upscale_args(settings, input_frames_dir, output_frames_dir, ai_scale);
    let assemble = ffmpeg::assemble_video_args(&AssembleVideoParams {
        overwrite: settings.overwrite,
        frames_pattern: &output_frames_pattern,
        original_input: &plan.input_path,
        frame_rate_expr: source.frame_rate_expr.as_deref().unwrap_or("24"),
        output_path: &plan.output_path,
        post_scale_filter: post_scale_filter.as_deref(),
        preset: plan.preset.as_ffmpeg_value(),
        crf: plan.crf,
        audio_bitrate_kbps: plan.audio_bitrate_kbps,
    });

    vec![
        ffmpeg::render_args(&settings.ffmpeg_bin, &extract),
        render_args(&settings.realesrgan_bin, &upscale),
        ffmpeg::render_args(&settings.ffmpeg_bin, &assemble),
    ]
}

pub fn run_pipeline(
    settings: &Settings,
    plan: &UpscalePlan,
    source: &VideoMetadata,
    quiet: bool,
) -> Result<()> {
    let temp_dir = TempDir::new().with_context(|| "failed to create temporary workspace")?;
    let input_frames_dir = temp_dir.path().join("input_frames");
    let output_frames_dir = temp_dir.path().join("output_frames");
    std::fs::create_dir_all(&input_frames_dir)
        .with_context(|| format!("failed to create `{}`", input_frames_dir.display()))?;
    std::fs::create_dir_all(&output_frames_dir)
        .with_context(|| format!("failed to create `{}`", output_frames_dir.display()))?;

    let input_frames_pattern = input_frames_dir.join("frame_%08d.png");
    let output_frames_pattern = output_frames_dir.join("frame_%08d.png");
    let ai_scale = inference_scale_for(plan.scale_factor);
    let post_scale_filter = post_scale_filter(plan.ai_target, plan.target);

    ffmpeg::run_args(
        &settings.ffmpeg_bin,
        &ffmpeg::extract_frames_args(settings.overwrite, &plan.input_path, &input_frames_pattern),
        quiet,
    )?;

    run_args(
        &settings.realesrgan_bin,
        &upscale_args(settings, &input_frames_dir, &output_frames_dir, ai_scale),
        quiet,
    )?;

    ffmpeg::run_args(
        &settings.ffmpeg_bin,
        &ffmpeg::assemble_video_args(&AssembleVideoParams {
            overwrite: settings.overwrite,
            frames_pattern: &output_frames_pattern,
            original_input: &plan.input_path,
            frame_rate_expr: source.frame_rate_expr.as_deref().unwrap_or("24"),
            output_path: &plan.output_path,
            post_scale_filter: post_scale_filter.as_deref(),
            preset: plan.preset.as_ffmpeg_value(),
            crf: plan.crf,
            audio_bitrate_kbps: plan.audio_bitrate_kbps,
        }),
        quiet,
    )?;

    Ok(())
}

pub fn inference_scale_for(requested_scale: f64) -> u8 {
    if requested_scale <= 2.0 {
        2
    } else if requested_scale <= 3.0 {
        3
    } else {
        4
    }
}

pub fn upscale_args(
    settings: &Settings,
    input_dir: &Path,
    output_dir: &Path,
    scale: u8,
) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("-i"),
        input_dir.as_os_str().to_os_string(),
        OsString::from("-o"),
        output_dir.as_os_str().to_os_string(),
        OsString::from("-n"),
        OsString::from(settings.realesrgan_model.as_binary_value()),
        OsString::from("-s"),
        OsString::from(scale.to_string()),
        OsString::from("-f"),
        OsString::from("png"),
        OsString::from("-t"),
        OsString::from(settings.realesrgan_tile.to_string()),
    ];

    if let Some(model_path) = &settings.realesrgan_model_path {
        args.push(OsString::from("-m"));
        args.push(model_path.as_os_str().to_os_string());
    }

    if settings.realesrgan_tta {
        args.push(OsString::from("-x"));
    }

    args
}

pub fn post_scale_filter(ai_target: Dimensions, final_target: Dimensions) -> Option<String> {
    if ai_target == final_target {
        return None;
    }

    Some(format!(
        "scale={}:{}:flags=lanczos",
        final_target.width, final_target.height
    ))
}

pub fn render_args(binary: &str, args: &[OsString]) -> String {
    ffmpeg::render_args(binary, args)
}

fn run_args(binary: &str, args: &[OsString], quiet: bool) -> Result<()> {
    let mut command = Command::new(binary);
    command.args(args);
    if quiet {
        command.stdout(Stdio::null()).stderr(Stdio::null());
    }

    let status = command
        .status()
        .with_context(|| format!("failed to spawn `{binary}`"))?;

    if status.success() {
        return Ok(());
    }

    bail!("realesrgan exited with status {status}")
}

fn smoke_test(settings: &Settings) -> Result<()> {
    let temp_dir = TempDir::new().with_context(|| "failed to create realesrgan test directory")?;
    let input_path = temp_dir.path().join("smoke.png");
    let output_path = temp_dir.path().join("smoke-out.png");

    let ffmpeg_status = Command::new(&settings.ffmpeg_bin)
        .args([
            "-hide_banner",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=32x32:d=0.1",
            "-frames:v",
            "1",
            "-y",
        ])
        .arg(&input_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| {
            format!(
                "failed to run `{}` for realesrgan smoke test",
                settings.ffmpeg_bin
            )
        })?;

    if !ffmpeg_status.success() {
        bail!("ffmpeg smoke test image generation failed with status {ffmpeg_status}");
    }

    let status = Command::new(&settings.realesrgan_bin)
        .arg("-i")
        .arg(&input_path)
        .arg("-o")
        .arg(&output_path)
        .arg("-n")
        .arg(settings.realesrgan_model.as_binary_value())
        .arg("-s")
        .arg("2")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| {
            format!(
                "failed to spawn `{}` for smoke test",
                settings.realesrgan_bin
            )
        })?;

    if !status.success() || !output_path.exists() {
        bail!(
            "`{}` failed a runtime smoke test. On this machine the official macOS binary appears incompatible, so use `engine = \"auto\"` or `engine = \"ffmpeg\"` unless you replace it with a working build.",
            settings.realesrgan_bin
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{EncodePreset, Engine, FilterProfile, RealEsrganModel, Scaler};
    use crate::config::Settings;

    fn sample_settings() -> Settings {
        Settings {
            input: PathBuf::from("/tmp/input.mp4"),
            output: None,
            output_dir: None,
            engine: Engine::Realesrgan,
            scale: 2.0,
            crf: 20,
            preset: EncodePreset::Slow,
            scaler: Scaler::Lanczos,
            filter_profile: FilterProfile::Auto,
            fx_upscale_bin: "fx-upscale".to_string(),
            realesrgan_model: RealEsrganModel::RealesrnetX4plus,
            realesrgan_bin: "realesrgan-ncnn-vulkan".to_string(),
            realesrgan_model_path: None,
            realesrgan_tile: 0,
            realesrgan_tta: false,
            audio_bitrate_kbps: 192,
            overwrite: false,
            open: false,
            dry_run: false,
            ffmpeg_bin: "ffmpeg".to_string(),
            ffprobe_bin: "ffprobe".to_string(),
            config_path: None,
        }
    }

    #[test]
    fn inference_scale_rounds_up_to_supported_value() {
        assert_eq!(inference_scale_for(1.5), 2);
        assert_eq!(inference_scale_for(2.0), 2);
        assert_eq!(inference_scale_for(2.2), 3);
        assert_eq!(inference_scale_for(3.6), 4);
    }

    #[test]
    fn post_scale_filter_is_omitted_when_dimensions_match() {
        assert_eq!(
            post_scale_filter(
                Dimensions {
                    width: 960,
                    height: 544
                },
                Dimensions {
                    width: 960,
                    height: 544
                }
            ),
            None
        );
    }

    #[test]
    fn upscale_args_include_model_and_scale() {
        let settings = sample_settings();
        let args = upscale_args(&settings, Path::new("/tmp/in"), Path::new("/tmp/out"), 2);
        let rendered = render_args(&settings.realesrgan_bin, &args);

        assert!(rendered.contains("realesrnet-x4plus"));
        assert!(rendered.contains(" -s 2 "));
        assert!(rendered.contains(" -f png "));
    }
}
