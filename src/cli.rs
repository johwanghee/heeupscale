use std::path::PathBuf;

use clap::{Args as ClapArgs, Parser, Subcommand, ValueEnum};
use serde::Deserialize;

#[derive(Debug, Clone, Parser)]
#[command(
    author,
    version,
    about = "Upscale a video with FFmpeg and write an IINA-friendly MP4.",
    args_conflicts_with_subcommands = true,
    subcommand_negates_reqs = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[command(flatten)]
    pub upscale: UpscaleArgs,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Initialize heeupscale config in a project directory.
    Init(InitArgs),
}

#[derive(Debug, Clone, ClapArgs)]
pub struct InitArgs {
    /// Directory to initialize. Defaults to the current directory.
    #[arg(default_value = ".")]
    pub dir: PathBuf,

    /// Emit machine-readable JSON instead of human-readable text.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, ClapArgs)]
pub struct UpscaleArgs {
    /// Source video file to upscale.
    pub input: Option<PathBuf>,

    /// Emit machine-readable JSON instead of human-readable text.
    #[arg(long)]
    pub json: bool,

    /// Explicit config file. Overrides auto-discovery.
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Output file. Defaults to `<input>_upscaled_<scale>x.mp4`.
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Upscale factor. Must be greater than 1.0.
    #[arg(long, value_parser = parse_scale)]
    pub scale: Option<f64>,

    /// x264 CRF quality setting. Lower is higher quality.
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=51))]
    pub crf: Option<u8>,

    /// x264 preset.
    #[arg(long, value_enum)]
    pub preset: Option<EncodePreset>,

    /// FFmpeg scaler kernel.
    #[arg(long, value_enum)]
    pub scaler: Option<Scaler>,

    /// Filter profile. `auto` adds gentle restoration for low-resolution sources.
    #[arg(long, value_enum)]
    pub profile: Option<FilterProfile>,

    /// AAC audio bitrate in kbps.
    #[arg(long, value_parser = parse_audio_bitrate)]
    pub audio_bitrate_kbps: Option<u16>,

    /// Processing backend.
    #[arg(long, value_enum)]
    pub engine: Option<Engine>,

    /// Override the fx-upscale binary path or name.
    #[arg(long)]
    pub fx_upscale_bin: Option<String>,

    /// Real-ESRGAN model to use when the realesrgan backend is selected.
    #[arg(long, value_enum)]
    pub realesrgan_model: Option<RealEsrganModel>,

    /// Override the realesrgan-ncnn-vulkan binary path or name.
    #[arg(long)]
    pub realesrgan_bin: Option<String>,

    /// Override the realesrgan model directory.
    #[arg(long)]
    pub realesrgan_model_path: Option<PathBuf>,

    /// Real-ESRGAN tile size. Use smaller values if VRAM is tight. `0` means auto.
    #[arg(long, value_parser = clap::value_parser!(u32))]
    pub realesrgan_tile: Option<u32>,

    /// Enable test-time augmentation for Real-ESRGAN.
    #[arg(
        long,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true"
    )]
    pub realesrgan_tta: Option<bool>,

    /// Overwrite the output file if it already exists.
    #[arg(
        short = 'y',
        long,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true"
    )]
    pub overwrite: Option<bool>,

    /// Open the resulting file in IINA after encoding.
    #[arg(
        long,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true"
    )]
    pub open: Option<bool>,

    /// Print the resolved FFmpeg command without running it.
    #[arg(long)]
    pub dry_run: bool,

    /// Override the FFmpeg binary path or name.
    #[arg(long)]
    pub ffmpeg_bin: Option<String>,

    /// Override the FFprobe binary path or name.
    #[arg(long)]
    pub ffprobe_bin: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EncodePreset {
    Ultrafast,
    Superfast,
    Veryfast,
    Faster,
    Fast,
    Medium,
    Slow,
    Slower,
    Veryslow,
}

impl EncodePreset {
    pub fn as_ffmpeg_value(self) -> &'static str {
        match self {
            Self::Ultrafast => "ultrafast",
            Self::Superfast => "superfast",
            Self::Veryfast => "veryfast",
            Self::Faster => "faster",
            Self::Fast => "fast",
            Self::Medium => "medium",
            Self::Slow => "slow",
            Self::Slower => "slower",
            Self::Veryslow => "veryslow",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scaler {
    Lanczos,
    Spline,
    Bicubic,
}

impl Scaler {
    pub fn as_ffmpeg_value(self) -> &'static str {
        match self {
            Self::Lanczos => "lanczos",
            Self::Spline => "spline",
            Self::Bicubic => "bicubic",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FilterProfile {
    Auto,
    ScaleOnly,
    Restore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Engine {
    Auto,
    Ffmpeg,
    FxUpscale,
    Realesrgan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RealEsrganModel {
    RealesrAnimevideov3,
    RealesrganX4plus,
    RealesrganX4plusAnime,
    RealesrnetX4plus,
}

impl RealEsrganModel {
    pub fn as_binary_value(self) -> &'static str {
        match self {
            Self::RealesrAnimevideov3 => "realesr-animevideov3",
            Self::RealesrganX4plus => "realesrgan-x4plus",
            Self::RealesrganX4plusAnime => "realesrgan-x4plus-anime",
            Self::RealesrnetX4plus => "realesrnet-x4plus",
        }
    }
}

fn parse_scale(raw: &str) -> Result<f64, String> {
    let scale: f64 = raw
        .parse()
        .map_err(|_| format!("`{raw}` is not a valid scale value"))?;

    validate_scale(scale)?;
    Ok(scale)
}

fn parse_audio_bitrate(raw: &str) -> Result<u16, String> {
    let bitrate: u16 = raw
        .parse()
        .map_err(|_| format!("`{raw}` is not a valid AAC bitrate"))?;

    validate_audio_bitrate(bitrate)?;
    Ok(bitrate)
}

pub fn validate_scale(scale: f64) -> Result<(), String> {
    if scale <= 1.0 {
        return Err("scale must be greater than 1.0".to_string());
    }

    if !scale.is_finite() {
        return Err("scale must be finite".to_string());
    }

    Ok(())
}

pub fn validate_audio_bitrate(bitrate: u16) -> Result<(), String> {
    if !(64..=512).contains(&bitrate) {
        return Err("audio bitrate must be between 64 and 512 kbps".to_string());
    }

    Ok(())
}
