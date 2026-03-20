use std::path::PathBuf;

use anyhow::{Result, bail, ensure};

use crate::cli::{EncodePreset, FilterProfile, Scaler};
use crate::config::Settings;
use crate::ffmpeg::VideoMetadata;

#[derive(Debug, Clone)]
pub struct UpscalePlan {
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub source: VideoMetadata,
    pub target: Dimensions,
    pub ai_target: Dimensions,
    pub scale_factor: f64,
    pub scale_label: String,
    pub filter_profile_label: String,
    pub filter_graph: String,
    pub preset: EncodePreset,
    pub crf: u8,
    pub audio_bitrate_kbps: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dimensions {
    pub width: u32,
    pub height: u32,
}

impl UpscalePlan {
    pub fn build(settings: &Settings, input_path: PathBuf, source: VideoMetadata) -> Result<Self> {
        let output_path = resolved_output_path(
            &input_path,
            settings.output.clone(),
            settings.output_dir.clone(),
            settings.scale,
        )?;

        ensure!(
            input_path != output_path,
            "output path must be different from the input path"
        );

        let target = upscale_dimensions(source.width, source.height, settings.scale)?;
        let ai_target = upscale_dimensions(
            source.width,
            source.height,
            crate::realesrgan::inference_scale_for(settings.realesrgan_model, settings.scale)
                as f64,
        )?;
        let scale_label = format_scale(settings.scale);
        let (filter_profile_label, filter_graph) =
            build_filter_graph(&source, target, settings.scaler, settings.filter_profile);

        Ok(Self {
            input_path,
            output_path,
            source,
            target,
            ai_target,
            scale_factor: settings.scale,
            scale_label,
            filter_profile_label,
            filter_graph,
            preset: settings.preset,
            crf: settings.crf,
            audio_bitrate_kbps: settings.audio_bitrate_kbps,
        })
    }
}

fn resolved_output_path(
    input_path: &std::path::Path,
    explicit_output: Option<PathBuf>,
    output_dir: Option<PathBuf>,
    scale: f64,
) -> Result<PathBuf> {
    if let Some(mut output) = explicit_output {
        if output.extension().is_none() {
            output.set_extension("mp4");
        }

        return Ok(output);
    }

    let parent = output_dir.unwrap_or_else(|| {
        input_path
            .parent()
            .map_or_else(PathBuf::new, std::path::Path::to_path_buf)
    });
    let stem = input_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("video");
    let filename = format!("{stem}_upscaled_{}.mp4", format_scale(scale));

    Ok(parent.join(filename))
}

fn upscale_dimensions(width: u32, height: u32, scale: f64) -> Result<Dimensions> {
    if width == 0 || height == 0 {
        bail!("source dimensions must be non-zero");
    }

    Ok(Dimensions {
        width: upscale_dimension(width, scale),
        height: upscale_dimension(height, scale),
    })
}

fn upscale_dimension(value: u32, scale: f64) -> u32 {
    let scaled = ((value as f64) * scale).round().max(2.0) as u32;
    if scaled.is_multiple_of(2) {
        scaled
    } else {
        scaled + 1
    }
}

fn build_filter_graph(
    source: &VideoMetadata,
    target: Dimensions,
    scaler: Scaler,
    profile: FilterProfile,
) -> (String, String) {
    match resolved_filter_profile(source, profile) {
        ResolvedFilterProfile::ScaleOnly => {
            let label = match profile {
                FilterProfile::Auto => "auto -> scale-only".to_string(),
                FilterProfile::ScaleOnly => "scale-only".to_string(),
                FilterProfile::Restore => "restore".to_string(),
            };
            (label, scale_only_filter(target, scaler))
        }
        ResolvedFilterProfile::Restore => {
            let label = match profile {
                FilterProfile::Auto => "auto -> restore".to_string(),
                FilterProfile::ScaleOnly => "scale-only".to_string(),
                FilterProfile::Restore => "restore".to_string(),
            };
            (label, restore_filter(target, scaler))
        }
    }
}

fn resolved_filter_profile(
    source: &VideoMetadata,
    profile: FilterProfile,
) -> ResolvedFilterProfile {
    match profile {
        FilterProfile::ScaleOnly => ResolvedFilterProfile::ScaleOnly,
        FilterProfile::Restore => ResolvedFilterProfile::Restore,
        FilterProfile::Auto => {
            if source.width <= 640 || source.height <= 360 {
                ResolvedFilterProfile::Restore
            } else {
                ResolvedFilterProfile::ScaleOnly
            }
        }
    }
}

fn scale_only_filter(target: Dimensions, scaler: Scaler) -> String {
    format!(
        "scale={}:{}:flags={}",
        target.width,
        target.height,
        scaler.as_ffmpeg_value()
    )
}

fn restore_filter(target: Dimensions, scaler: Scaler) -> String {
    format!(
        "hqdn3d=1.2:1.2:6:6,scale={}:{}:flags={},unsharp=5:5:0.35:5:5:0.0",
        target.width,
        target.height,
        scaler.as_ffmpeg_value()
    )
}

fn format_scale(scale: f64) -> String {
    if (scale.fract()).abs() < f64::EPSILON {
        format!("{}x", scale as i64)
    } else {
        format!("{}x", trim_float(scale).replace('.', "_"))
    }
}

fn trim_float(value: f64) -> String {
    let raw = format!("{value:.3}");
    raw.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolvedFilterProfile {
    ScaleOnly,
    Restore,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffmpeg::VideoMetadata;

    #[test]
    fn upscale_rounds_to_even_dimensions() {
        let dimensions = upscale_dimensions(853, 481, 1.5).expect("dimensions should resolve");

        assert_eq!(
            dimensions,
            Dimensions {
                width: 1280,
                height: 722
            }
        );
    }

    #[test]
    fn default_output_uses_iina_friendly_mp4_name() {
        let output =
            resolved_output_path(std::path::Path::new("/tmp/input clip.mov"), None, None, 2.0)
                .expect("output path should resolve");

        assert_eq!(output, PathBuf::from("/tmp/input clip_upscaled_2x.mp4"));
    }

    #[test]
    fn explicit_output_without_extension_defaults_to_mp4() {
        let output = resolved_output_path(
            std::path::Path::new("/tmp/input.mov"),
            Some(PathBuf::from("/tmp/custom-output")),
            None,
            2.0,
        )
        .expect("output path should resolve");

        assert_eq!(output, PathBuf::from("/tmp/custom-output.mp4"));
    }

    #[test]
    fn config_output_dir_moves_render_target() {
        let output = resolved_output_path(
            std::path::Path::new("/tmp/source/input.mov"),
            None,
            Some(PathBuf::from("/tmp/renders")),
            2.0,
        )
        .expect("output path should resolve");

        assert_eq!(output, PathBuf::from("/tmp/renders/input_upscaled_2x.mp4"));
    }

    #[test]
    fn scale_only_filter_uses_requested_kernel() {
        let filter = scale_only_filter(
            Dimensions {
                width: 3840,
                height: 2160,
            },
            Scaler::Spline,
        );

        assert_eq!(filter, "scale=3840:2160:flags=spline");
    }

    #[test]
    fn auto_profile_uses_restore_chain_for_small_sources() {
        let source = VideoMetadata {
            width: 480,
            height: 272,
            frame_rate_expr: Some("24/1".to_string()),
            frame_rate: Some(24.0),
            pixel_format: Some("yuv420p".to_string()),
        };

        let (label, filter) = build_filter_graph(
            &source,
            Dimensions {
                width: 960,
                height: 544,
            },
            Scaler::Lanczos,
            FilterProfile::Auto,
        );

        assert_eq!(label, "auto -> restore");
        assert!(filter.contains("hqdn3d="));
        assert!(filter.contains("unsharp="));
        assert!(filter.contains("scale=960:544:flags=lanczos"));
    }

    #[test]
    fn auto_profile_keeps_scale_only_for_large_sources() {
        let source = VideoMetadata {
            width: 1920,
            height: 1080,
            frame_rate_expr: Some("24/1".to_string()),
            frame_rate: Some(24.0),
            pixel_format: Some("yuv420p".to_string()),
        };

        let (label, filter) = build_filter_graph(
            &source,
            Dimensions {
                width: 3840,
                height: 2160,
            },
            Scaler::Lanczos,
            FilterProfile::Auto,
        );

        assert_eq!(label, "auto -> scale-only");
        assert_eq!(filter, "scale=3840:2160:flags=lanczos");
    }
}
