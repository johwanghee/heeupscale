use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use tempfile::TempDir;

use crate::config::Settings;
use crate::ffmpeg;
use crate::planner::UpscalePlan;
use crate::progress::{StageProgress, run_command_with_percent_progress};

const OUTPUT_CODEC: &str = "h264";

pub fn is_available(settings: &Settings) -> bool {
    Command::new(&settings.fx_upscale_bin)
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub fn ensure_tool(settings: &Settings) -> Result<()> {
    let status = Command::new(&settings.fx_upscale_bin)
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    match status {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => bail!(
            "`{}` responded with status {status}. On macOS install or update it with `brew install fx-upscale`.",
            settings.fx_upscale_bin
        ),
        Err(error) if error.kind() == ErrorKind::NotFound => bail!(
            "`{}` was not found in PATH. On macOS install it with `brew install fx-upscale`.",
            settings.fx_upscale_bin
        ),
        Err(error) => Err(error).with_context(|| {
            format!(
                "failed to run `{}` to validate fx-upscale",
                settings.fx_upscale_bin
            )
        }),
    }
}

pub fn render_commands(settings: &Settings, plan: &UpscalePlan) -> Vec<String> {
    let staged_input = Path::new("$TMPDIR").join(input_file_name(&plan.input_path));
    let staged_output = staged_output_path(&staged_input);

    vec![
        ffmpeg::render_args(
            "ln",
            &[
                OsString::from("-s"),
                plan.input_path.as_os_str().to_os_string(),
                staged_input.as_os_str().to_os_string(),
            ],
        ),
        ffmpeg::render_args(
            &settings.fx_upscale_bin,
            &upscale_args(&staged_input, plan.target.width, plan.target.height),
        ),
        ffmpeg::render_args(
            "mv",
            &[
                staged_output.as_os_str().to_os_string(),
                plan.output_path.as_os_str().to_os_string(),
            ],
        ),
    ]
}

pub fn run_pipeline(
    settings: &Settings,
    plan: &UpscalePlan,
    quiet: bool,
    show_progress: bool,
) -> Result<()> {
    let progress = StageProgress::new(3, show_progress);
    let workspace =
        TempDir::new().with_context(|| "failed to create temporary fx-upscale workspace")?;
    let staged_input = workspace.path().join(input_file_name(&plan.input_path));
    progress.set(1, "Preparing input", 0.0);
    if let Err(error) = stage_input(&plan.input_path, &staged_input) {
        progress.abandon();
        return Err(error);
    }
    progress.set(1, "Preparing input", 1.0);

    let staged_output = staged_output_path(&staged_input);
    progress.set(2, "AI upscaling", 0.0);
    if show_progress {
        if let Err(error) = run_command_with_percent_progress(
            &settings.fx_upscale_bin,
            &upscale_args(&staged_input, plan.target.width, plan.target.height),
            &progress,
            2,
            "AI upscaling",
        ) {
            progress.abandon();
            return Err(error);
        }
    } else if let Err(error) = run_args(
        &settings.fx_upscale_bin,
        &upscale_args(&staged_input, plan.target.width, plan.target.height),
        quiet,
    ) {
        progress.abandon();
        return Err(error);
    }
    progress.set(2, "AI upscaling", 1.0);

    if !staged_output.is_file() {
        progress.abandon();
        bail!(
            "`{}` finished without producing the expected output `{}`",
            settings.fx_upscale_bin,
            staged_output.display()
        );
    }

    progress.set(3, "Finalizing output", 0.0);
    if let Err(error) = place_output(&staged_output, &plan.output_path, settings.overwrite) {
        progress.abandon();
        return Err(error);
    }
    progress.set(3, "Finalizing output", 1.0);
    progress.finish("Upscale complete");
    Ok(())
}

fn upscale_args(input_path: &Path, width: u32, height: u32) -> Vec<OsString> {
    vec![
        input_path.as_os_str().to_os_string(),
        OsString::from("--width"),
        OsString::from(width.to_string()),
        OsString::from("--height"),
        OsString::from(height.to_string()),
        OsString::from("--codec"),
        OsString::from(OUTPUT_CODEC),
    ]
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

    bail!("fx-upscale exited with status {status}");
}

fn input_file_name(input_path: &Path) -> OsString {
    input_path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_else(|| OsString::from("input.mp4"))
}

fn staged_output_path(staged_input: &Path) -> PathBuf {
    let parent = staged_input
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let stem = staged_input
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "video".to_string());

    parent.join(format!("{stem} Upscaled.mp4"))
}

fn stage_input(source: &Path, staged_input: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        if std::os::unix::fs::symlink(source, staged_input).is_ok() {
            return Ok(());
        }
    }

    std::fs::copy(source, staged_input)
        .with_context(|| format!("failed to stage `{}` for fx-upscale", source.display()))?;
    Ok(())
}

fn place_output(generated: &Path, destination: &Path, overwrite: bool) -> Result<()> {
    if destination.exists() {
        if !overwrite {
            bail!(
                "output file `{}` already exists. Re-run with `--overwrite` to replace it.",
                destination.display()
            );
        }

        std::fs::remove_file(destination).with_context(|| {
            format!(
                "failed to remove existing output `{}`",
                destination.display()
            )
        })?;
    }

    if let Err(error) = std::fs::rename(generated, destination) {
        std::fs::copy(generated, destination).with_context(|| {
            format!(
                "failed to move fx-upscale output from `{}` to `{}` after rename failed: {error}",
                generated.display(),
                destination.display()
            )
        })?;
        std::fs::remove_file(generated).with_context(|| {
            format!(
                "failed to clean up staged fx-upscale output `{}`",
                generated.display()
            )
        })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{EncodePreset, Engine, FilterProfile, RealEsrganModel, Scaler};
    use crate::config::Settings;
    use crate::ffmpeg::VideoMetadata;
    use crate::planner::UpscalePlan;

    fn sample_settings() -> Settings {
        Settings {
            input: PathBuf::from("/tmp/input.mp4"),
            output: None,
            output_dir: None,
            engine: Engine::FxUpscale,
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

    fn sample_plan() -> UpscalePlan {
        UpscalePlan {
            input_path: PathBuf::from("/tmp/input.mov"),
            output_path: PathBuf::from("/tmp/output.mp4"),
            source: VideoMetadata {
                width: 480,
                height: 272,
                duration_seconds: Some(10.0),
                frame_rate_expr: Some("24/1".to_string()),
                frame_rate: Some(24.0),
                pixel_format: Some("yuv420p".to_string()),
            },
            target: crate::planner::Dimensions {
                width: 960,
                height: 544,
            },
            ai_target: crate::planner::Dimensions {
                width: 960,
                height: 544,
            },
            scale_factor: 2.0,
            scale_label: "2x".to_string(),
            filter_profile_label: "auto -> restore".to_string(),
            filter_graph: "scale=960:544:flags=lanczos".to_string(),
            preset: EncodePreset::Slow,
            crf: 20,
            audio_bitrate_kbps: 192,
        }
    }

    #[test]
    fn staged_output_uses_fx_upscale_naming() {
        assert_eq!(
            staged_output_path(Path::new("/tmp/input.mov")),
            PathBuf::from("/tmp/input Upscaled.mp4")
        );
    }

    #[test]
    fn render_commands_include_requested_dimensions() {
        let commands = render_commands(&sample_settings(), &sample_plan());
        assert!(commands[1].contains("fx-upscale"));
        assert!(commands[1].contains("--width 960"));
        assert!(commands[1].contains("--height 544"));
        assert!(commands[1].contains("--codec h264"));
    }
}
