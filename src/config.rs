use std::env;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::cli::{
    EncodePreset, Engine, FilterProfile, InitArgs, RealEsrganModel, Scaler, UpscaleArgs,
    validate_audio_bitrate, validate_scale,
};

const DEFAULT_ENGINE: Engine = Engine::Auto;
const DEFAULT_SCALE: f64 = 2.0;
const DEFAULT_CRF: u8 = 20;
const DEFAULT_PRESET: EncodePreset = EncodePreset::Slow;
const DEFAULT_SCALER: Scaler = Scaler::Lanczos;
const DEFAULT_FILTER_PROFILE: FilterProfile = FilterProfile::Auto;
const DEFAULT_FX_UPSCALE_BIN: &str = "fx-upscale";
const DEFAULT_REALESRGAN_MODEL: RealEsrganModel = RealEsrganModel::RealesrnetX4plus;
const DEFAULT_REALESRGAN_BIN: &str = "realesrgan-ncnn-vulkan";
const DEFAULT_REALESRGAN_TILE: u32 = 0;
const DEFAULT_REALESRGAN_TTA: bool = false;
const DEFAULT_AUDIO_BITRATE_KBPS: u16 = 192;
const DEFAULT_OVERWRITE: bool = false;
const DEFAULT_OPEN: bool = false;
const DEFAULT_FFMPEG_BIN: &str = "ffmpeg";
const DEFAULT_FFPROBE_BIN: &str = "ffprobe";

#[derive(Debug, Clone)]
pub struct Settings {
    pub input: PathBuf,
    pub output: Option<PathBuf>,
    pub output_dir: Option<PathBuf>,
    pub engine: Engine,
    pub scale: f64,
    pub crf: u8,
    pub preset: EncodePreset,
    pub scaler: Scaler,
    pub filter_profile: FilterProfile,
    pub fx_upscale_bin: String,
    pub realesrgan_model: RealEsrganModel,
    pub realesrgan_bin: String,
    pub realesrgan_model_path: Option<PathBuf>,
    pub realesrgan_tile: u32,
    pub realesrgan_tta: bool,
    pub audio_bitrate_kbps: u16,
    pub overwrite: bool,
    pub open: bool,
    pub dry_run: bool,
    pub ffmpeg_bin: String,
    pub ffprobe_bin: String,
    pub config_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitResult {
    pub path: PathBuf,
    pub action: InitAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitAction {
    Created,
    Appended,
    AlreadyPresent,
}

#[derive(Debug, Clone)]
struct LoadedConfig {
    path: PathBuf,
    root: PathBuf,
    values: FileConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct DocumentConfig {
    #[serde(rename = "heeupscale", alias = "hee-upscale")]
    heeupscale: Option<FileConfig>,
    #[serde(flatten)]
    top_level: FileConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct FileConfig {
    engine: Option<Engine>,
    scale: Option<f64>,
    crf: Option<u8>,
    preset: Option<EncodePreset>,
    scaler: Option<Scaler>,
    profile: Option<FilterProfile>,
    fx_upscale_bin: Option<String>,
    realesrgan_model: Option<RealEsrganModel>,
    realesrgan_bin: Option<String>,
    realesrgan_model_path: Option<PathBuf>,
    realesrgan_tile: Option<u32>,
    realesrgan_tta: Option<bool>,
    audio_bitrate_kbps: Option<u16>,
    overwrite: Option<bool>,
    open: Option<bool>,
    output_dir: Option<PathBuf>,
    ffmpeg_bin: Option<String>,
    ffprobe_bin: Option<String>,
}

impl FileConfig {
    fn is_empty(&self) -> bool {
        self.engine.is_none()
            && self.scale.is_none()
            && self.crf.is_none()
            && self.preset.is_none()
            && self.scaler.is_none()
            && self.profile.is_none()
            && self.fx_upscale_bin.is_none()
            && self.realesrgan_model.is_none()
            && self.realesrgan_bin.is_none()
            && self.realesrgan_model_path.is_none()
            && self.realesrgan_tile.is_none()
            && self.realesrgan_tta.is_none()
            && self.audio_bitrate_kbps.is_none()
            && self.overwrite.is_none()
            && self.open.is_none()
            && self.output_dir.is_none()
            && self.ffmpeg_bin.is_none()
            && self.ffprobe_bin.is_none()
    }
}

pub fn resolve(args: UpscaleArgs) -> Result<Settings> {
    let current_dir =
        env::current_dir().with_context(|| "failed to resolve current working directory")?;
    let input = args.input.clone().context(
        "missing input file. Use `heeupscale <input>` to upscale a video or `heeupscale init` to create a project config.",
    )?;
    let loaded = load_config(args.config.as_deref(), &current_dir, args.input.as_deref())?;
    let config = loaded.as_ref().map(|loaded| &loaded.values);

    let scale = args
        .scale
        .or(config.and_then(|config| config.scale))
        .unwrap_or(DEFAULT_SCALE);
    validate_scale(scale).map_err(anyhow::Error::msg)?;

    let crf = args
        .crf
        .or(config.and_then(|config| config.crf))
        .unwrap_or(DEFAULT_CRF);
    if crf > 51 {
        bail!("crf must be between 0 and 51");
    }

    let audio_bitrate_kbps = args
        .audio_bitrate_kbps
        .or(config.and_then(|config| config.audio_bitrate_kbps))
        .unwrap_or(DEFAULT_AUDIO_BITRATE_KBPS);
    validate_audio_bitrate(audio_bitrate_kbps).map_err(anyhow::Error::msg)?;

    let output_dir = config
        .and_then(|config| config.output_dir.clone())
        .map(|path| resolve_config_relative_path(loaded.as_ref(), path));

    Ok(Settings {
        input,
        output: args.output,
        output_dir,
        engine: args
            .engine
            .or(config.and_then(|config| config.engine))
            .unwrap_or(DEFAULT_ENGINE),
        scale,
        crf,
        preset: args
            .preset
            .or(config.and_then(|config| config.preset))
            .unwrap_or(DEFAULT_PRESET),
        scaler: args
            .scaler
            .or(config.and_then(|config| config.scaler))
            .unwrap_or(DEFAULT_SCALER),
        filter_profile: args
            .profile
            .or(config.and_then(|config| config.profile))
            .unwrap_or(DEFAULT_FILTER_PROFILE),
        fx_upscale_bin: args
            .fx_upscale_bin
            .or_else(|| config.and_then(|config| config.fx_upscale_bin.clone()))
            .unwrap_or_else(|| DEFAULT_FX_UPSCALE_BIN.to_string()),
        realesrgan_model: args
            .realesrgan_model
            .or(config.and_then(|config| config.realesrgan_model))
            .unwrap_or(DEFAULT_REALESRGAN_MODEL),
        realesrgan_bin: args
            .realesrgan_bin
            .or_else(|| config.and_then(|config| config.realesrgan_bin.clone()))
            .unwrap_or_else(|| DEFAULT_REALESRGAN_BIN.to_string()),
        realesrgan_model_path: args
            .realesrgan_model_path
            .or_else(|| config.and_then(|config| config.realesrgan_model_path.clone()))
            .map(|path| resolve_config_relative_path(loaded.as_ref(), path)),
        realesrgan_tile: args
            .realesrgan_tile
            .or(config.and_then(|config| config.realesrgan_tile))
            .unwrap_or(DEFAULT_REALESRGAN_TILE),
        realesrgan_tta: args
            .realesrgan_tta
            .or(config.and_then(|config| config.realesrgan_tta))
            .unwrap_or(DEFAULT_REALESRGAN_TTA),
        audio_bitrate_kbps,
        overwrite: args
            .overwrite
            .or(config.and_then(|config| config.overwrite))
            .unwrap_or(DEFAULT_OVERWRITE),
        open: args
            .open
            .or(config.and_then(|config| config.open))
            .unwrap_or(DEFAULT_OPEN),
        dry_run: args.dry_run,
        ffmpeg_bin: args
            .ffmpeg_bin
            .or_else(|| config.and_then(|config| config.ffmpeg_bin.clone()))
            .unwrap_or_else(|| DEFAULT_FFMPEG_BIN.to_string()),
        ffprobe_bin: args
            .ffprobe_bin
            .or_else(|| config.and_then(|config| config.ffprobe_bin.clone()))
            .unwrap_or_else(|| DEFAULT_FFPROBE_BIN.to_string()),
        config_path: loaded.as_ref().map(|loaded| loaded.path.clone()),
    })
}

pub fn init_project(args: &InitArgs) -> Result<InitResult> {
    let current_dir =
        env::current_dir().with_context(|| "failed to resolve current working directory")?;
    let directory = absolutize(&current_dir, &args.dir);
    std::fs::create_dir_all(&directory)
        .with_context(|| format!("failed to create directory `{}`", directory.display()))?;

    let config_path = directory.join("config.toml");
    let alternate_path = directory.join("heeupscale.toml");
    let legacy_alternate_path = directory.join("hee-upscale.toml");

    if !config_path.exists() && load_one_if_supported(&alternate_path)?.is_some() {
        return Ok(InitResult {
            path: alternate_path,
            action: InitAction::AlreadyPresent,
        });
    }

    if !config_path.exists() && load_one_if_supported(&legacy_alternate_path)?.is_some() {
        return Ok(InitResult {
            path: legacy_alternate_path,
            action: InitAction::AlreadyPresent,
        });
    }

    if !config_path.exists() {
        std::fs::write(&config_path, project_template())
            .with_context(|| format!("failed to write `{}`", config_path.display()))?;
        return Ok(InitResult {
            path: config_path,
            action: InitAction::Created,
        });
    }

    let content = std::fs::read_to_string(&config_path)
        .with_context(|| format!("failed to read `{}`", config_path.display()))?;
    let document: DocumentConfig = toml::from_str(&content)
        .with_context(|| format!("failed to parse config file `{}`", config_path.display()))?;

    if document.heeupscale.is_some() {
        return Ok(InitResult {
            path: config_path,
            action: InitAction::AlreadyPresent,
        });
    }

    let mut updated = content;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    if !updated.is_empty() {
        updated.push('\n');
    }
    updated.push_str(&project_template());

    std::fs::write(&config_path, updated)
        .with_context(|| format!("failed to write `{}`", config_path.display()))?;

    Ok(InitResult {
        path: config_path,
        action: InitAction::Appended,
    })
}

fn load_config(
    explicit: Option<&Path>,
    current_dir: &Path,
    input: Option<&Path>,
) -> Result<Option<LoadedConfig>> {
    if let Some(path) = explicit {
        let path = absolutize(current_dir, path);
        return load_one(&path, false).map(Some);
    }

    for path in discovery_candidates(current_dir, input) {
        if let Some(loaded) = load_one_if_supported(&path)? {
            return Ok(Some(loaded));
        }
    }

    Ok(None)
}

fn discovery_candidates(current_dir: &Path, input: Option<&Path>) -> Vec<PathBuf> {
    let mut directories = ancestor_dirs(current_dir.to_path_buf());
    let input_dir = input.and_then(|input| {
        absolutize(current_dir, input)
            .parent()
            .map(Path::to_path_buf)
    });

    if let Some(input_dir) = input_dir {
        for directory in ancestor_dirs(input_dir) {
            if !directories.contains(&directory) {
                directories.push(directory);
            }
        }
    }

    let mut candidates = Vec::new();
    for directory in directories {
        candidates.push(directory.join("heeupscale.toml"));
        candidates.push(directory.join("config.toml"));
        candidates.push(directory.join("hee-upscale.toml"));
    }

    candidates
}

fn ancestor_dirs(start: PathBuf) -> Vec<PathBuf> {
    let mut directories = Vec::new();
    let mut current = Some(start);

    while let Some(path) = current {
        directories.push(path.clone());
        current = path.parent().map(Path::to_path_buf);
    }

    directories
}

fn load_one_if_supported(path: &Path) -> Result<Option<LoadedConfig>> {
    if !path.is_file() {
        return Ok(None);
    }

    let section_only = path.file_name().and_then(|name| name.to_str()) == Some("config.toml");
    match parse_config(path, section_only)? {
        Some(values) => Ok(Some(LoadedConfig {
            path: path.to_path_buf(),
            root: path.parent().map(Path::to_path_buf).unwrap_or_default(),
            values,
        })),
        None => Ok(None),
    }
}

fn load_one(path: &Path, section_only: bool) -> Result<LoadedConfig> {
    let values = parse_config(path, section_only)?.with_context(|| {
        format!(
            "`{}` does not contain a usable heeupscale config",
            path.display()
        )
    })?;

    Ok(LoadedConfig {
        path: path.to_path_buf(),
        root: path.parent().map(Path::to_path_buf).unwrap_or_default(),
        values,
    })
}

fn parse_config(path: &Path, section_only: bool) -> Result<Option<FileConfig>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read config file `{}`", path.display()))?;
    let document: DocumentConfig = toml::from_str(&content)
        .with_context(|| format!("failed to parse config file `{}`", path.display()))?;

    if let Some(section) = document.heeupscale {
        return Ok(Some(section));
    }

    if section_only || document.top_level.is_empty() {
        return Ok(None);
    }

    Ok(Some(document.top_level))
}

fn absolutize(current_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        current_dir.join(path)
    }
}

fn resolve_config_relative_path(loaded: Option<&LoadedConfig>, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        return path;
    }

    if let Some(loaded) = loaded {
        return loaded.root.join(path);
    }

    path
}

fn project_template() -> String {
    format!(
        concat!(
            "[heeupscale]\n",
            "engine = \"{}\"\n",
            "scale = {:.1}\n",
            "crf = {}\n",
            "preset = \"{}\"\n",
            "scaler = \"{}\"\n",
            "profile = \"{}\"\n",
            "fx_upscale_bin = \"{}\"\n",
            "realesrgan_model = \"{}\"\n",
            "realesrgan_bin = \"{}\"\n",
            "realesrgan_tile = {}\n",
            "realesrgan_tta = {}\n",
            "audio_bitrate_kbps = {}\n",
            "open = {}\n",
            "overwrite = {}\n",
            "ffmpeg_bin = \"{}\"\n",
            "ffprobe_bin = \"{}\"\n",
            "# output_dir is unset by default, so the output is written next to the input file.\n",
            "# output_dir = \"exports\"\n"
        ),
        engine_value(DEFAULT_ENGINE),
        DEFAULT_SCALE,
        DEFAULT_CRF,
        DEFAULT_PRESET.as_ffmpeg_value(),
        DEFAULT_SCALER.as_ffmpeg_value(),
        filter_profile_value(DEFAULT_FILTER_PROFILE),
        DEFAULT_FX_UPSCALE_BIN,
        DEFAULT_REALESRGAN_MODEL.as_binary_value(),
        DEFAULT_REALESRGAN_BIN,
        DEFAULT_REALESRGAN_TILE,
        DEFAULT_REALESRGAN_TTA,
        DEFAULT_AUDIO_BITRATE_KBPS,
        DEFAULT_OPEN,
        DEFAULT_OVERWRITE,
        DEFAULT_FFMPEG_BIN,
        DEFAULT_FFPROBE_BIN,
    )
}

fn engine_value(engine: Engine) -> &'static str {
    match engine {
        Engine::Auto => "auto",
        Engine::Ffmpeg => "ffmpeg",
        Engine::FxUpscale => "fx-upscale",
        Engine::Realesrgan => "realesrgan",
    }
}

fn filter_profile_value(profile: FilterProfile) -> &'static str {
    match profile {
        FilterProfile::Auto => "auto",
        FilterProfile::ScaleOnly => "scale-only",
        FilterProfile::Restore => "restore",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_toml_uses_section_values() {
        let document: DocumentConfig = toml::from_str(
            r#"
            [heeupscale]
            engine = "fx-upscale"
            scale = 1.5
            profile = "auto"
            realesrgan_model = "realesrnet-x4plus"
            open = true
            output_dir = "exports"
            "#,
        )
        .expect("config should parse");

        let config = document.heeupscale.expect("section should exist");
        assert_eq!(config.engine, Some(Engine::FxUpscale));
        assert_eq!(config.scale, Some(1.5));
        assert_eq!(config.profile, Some(FilterProfile::Auto));
        assert_eq!(
            config.realesrgan_model,
            Some(RealEsrganModel::RealesrnetX4plus)
        );
        assert_eq!(config.open, Some(true));
        assert_eq!(config.output_dir, Some(PathBuf::from("exports")));
    }

    #[test]
    fn heeupscale_toml_can_use_top_level_values() {
        let document: DocumentConfig = toml::from_str(
            r#"
            scale = 2.0
            preset = "slow"
            "#,
        )
        .expect("config should parse");

        assert_eq!(document.top_level.scale, Some(2.0));
        assert_eq!(document.top_level.preset, Some(EncodePreset::Slow));
    }

    #[test]
    fn init_creates_config_toml() {
        let directory = temp_test_dir("create");
        let result = init_project(&InitArgs {
            dir: directory.clone(),
            json: false,
        })
        .expect("init should succeed");

        assert_eq!(result.action, InitAction::Created);
        assert_eq!(result.path, directory.join("config.toml"));

        let content =
            std::fs::read_to_string(directory.join("config.toml")).expect("config should exist");
        assert!(content.contains("[heeupscale]"));
        assert!(content.contains("engine = \"auto\""));
        assert!(content.contains("profile = \"auto\""));
        assert!(content.contains("fx_upscale_bin = \"fx-upscale\""));
        assert!(content.contains("realesrgan_bin = \"realesrgan-ncnn-vulkan\""));
        assert!(content.contains("open = false"));
        assert!(content.contains("ffmpeg_bin = \"ffmpeg\""));
        assert!(content.contains("# output_dir = \"exports\""));

        cleanup_test_dir(&directory);
    }

    #[test]
    fn init_appends_section_to_existing_config() {
        let directory = temp_test_dir("append");
        let config_path = directory.join("config.toml");
        std::fs::write(&config_path, "[app]\nname = \"demo\"\n").expect("seed config");

        let result = init_project(&InitArgs {
            dir: directory.clone(),
            json: false,
        })
        .expect("init should succeed");

        assert_eq!(result.action, InitAction::Appended);

        let content = std::fs::read_to_string(&config_path).expect("config should exist");
        assert!(content.contains("[app]"));
        assert!(content.contains("[heeupscale]"));

        cleanup_test_dir(&directory);
    }

    #[test]
    fn init_is_idempotent_when_section_exists() {
        let directory = temp_test_dir("existing");
        let config_path = directory.join("config.toml");
        let template = project_template();
        std::fs::write(&config_path, &template).expect("seed config");

        let result = init_project(&InitArgs {
            dir: directory.clone(),
            json: false,
        })
        .expect("init should succeed");

        assert_eq!(result.action, InitAction::AlreadyPresent);
        let content = std::fs::read_to_string(&config_path).expect("config should exist");
        assert_eq!(content, template);

        cleanup_test_dir(&directory);
    }

    fn temp_test_dir(label: &str) -> PathBuf {
        let unique = format!(
            "heeupscale-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time should move forward")
                .as_nanos()
        );
        let directory = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&directory).expect("temp dir should be created");
        directory
    }

    fn cleanup_test_dir(path: &Path) {
        let _ = std::fs::remove_dir_all(path);
    }
}
