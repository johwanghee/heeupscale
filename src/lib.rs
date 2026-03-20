mod cli;
mod config;
mod ffmpeg;
mod fx_upscale;
mod planner;
mod progress;
mod realesrgan;

use std::io::IsTerminal;

use anyhow::{Context, Result};
use clap::Parser;
use serde_json::json;

use crate::cli::{Cli, Command, EncodePreset, Engine, FilterProfile, InitArgs, Scaler};
use crate::config::{InitAction, Settings};
use crate::planner::UpscalePlan;

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    let json_output = json_output(&cli);
    let mode = command_mode(&cli);

    if let Err(error) = run_with_cli(cli) {
        if json_output {
            print_json_error(mode, &error)?;
            return Ok(());
        }

        return Err(error);
    }

    Ok(())
}

fn run_with_cli(cli: Cli) -> Result<()> {
    if let Some(Command::Init(args)) = &cli.command {
        return run_init(args, args.json);
    }

    let json_output = cli.upscale.json;
    let show_progress = !json_output && std::io::stderr().is_terminal();
    let quiet_subprocesses = json_output || show_progress;
    let settings = config::resolve(cli.upscale)?;
    let input_path = settings.input.canonicalize().with_context(|| {
        format!(
            "failed to resolve input file `{}`",
            settings.input.display()
        )
    })?;

    ffmpeg::ensure_tool(&settings.ffmpeg_bin, "ffmpeg")?;
    ffmpeg::ensure_tool(&settings.ffprobe_bin, "ffprobe")?;

    let source = ffmpeg::probe_video(&settings.ffprobe_bin, &input_path)?;
    let plan = UpscalePlan::build(&settings, input_path, source)?;
    let engine = resolve_engine(&settings, &plan.source);
    let commands = render_commands(&settings, &plan, engine);

    match engine {
        RuntimeEngine::FxUpscale => fx_upscale::ensure_tool(&settings)?,
        RuntimeEngine::Realesrgan => realesrgan::ensure_tool(&settings)?,
        RuntimeEngine::Ffmpeg => {}
    }

    if !json_output {
        print_plan(&settings, &plan, engine, &commands);
    }

    if settings.dry_run {
        if json_output {
            print_json_plan(&settings, &plan, engine, &commands, "planned", false)?;
        }
        return Ok(());
    }

    if let Some(parent) = plan.output_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create output directory `{}`", parent.display()))?;
    }

    match engine {
        RuntimeEngine::Ffmpeg => {
            if show_progress {
                let progress = progress::StageProgress::new(1, true);
                progress.set(1, "Encoding video", 0.0);
                let result = ffmpeg::run_ffmpeg_with_progress(
                    &settings.ffmpeg_bin,
                    settings.overwrite,
                    &plan,
                    &progress,
                    1,
                    "Encoding video",
                );
                match result {
                    Ok(()) => progress.finish("Encoding complete"),
                    Err(error) => {
                        progress.abandon();
                        return Err(error);
                    }
                }
            } else {
                ffmpeg::run_ffmpeg(
                    &settings.ffmpeg_bin,
                    settings.overwrite,
                    &plan,
                    quiet_subprocesses,
                )?;
            }
        }
        RuntimeEngine::FxUpscale => {
            fx_upscale::run_pipeline(&settings, &plan, quiet_subprocesses, show_progress)?;
        }
        RuntimeEngine::Realesrgan => {
            realesrgan::run_pipeline(
                &settings,
                &plan,
                &plan.source,
                quiet_subprocesses,
                show_progress,
            )?;
        }
    }

    let mut opened_in_iina = false;
    if settings.open {
        ffmpeg::open_in_iina(&plan.output_path)?;
        opened_in_iina = true;
    }

    if json_output {
        print_json_plan(
            &settings,
            &plan,
            engine,
            &commands,
            "completed",
            opened_in_iina,
        )?;
    } else {
        println!("done: {}", plan.output_path.display());
        if opened_in_iina {
            println!("opened in IINA");
        }
    }

    Ok(())
}

fn run_init(args: &InitArgs, json_output: bool) -> Result<()> {
    let result = config::init_project(args)?;

    if json_output {
        let status = match result.action {
            InitAction::Created => "created",
            InitAction::Appended => "updated",
            InitAction::AlreadyPresent => "exists",
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "mode": "init",
                "status": status,
                "path": result.path.display().to_string(),
                "next": "heeupscale <video-file>",
            }))?
        );
        return Ok(());
    }

    match result.action {
        InitAction::Created => println!("created: {}", result.path.display()),
        InitAction::Appended => println!("updated: {}", result.path.display()),
        InitAction::AlreadyPresent => println!("exists : {}", result.path.display()),
    }

    println!("next   : heeupscale <video-file>");
    Ok(())
}

fn print_plan(settings: &Settings, plan: &UpscalePlan, engine: RuntimeEngine, commands: &[String]) {
    if let Some(config_path) = &settings.config_path {
        println!("config: {}", config_path.display());
    }

    println!("engine: {}", engine.label());
    println!("input : {}", plan.input_path.display());
    println!("output: {}", plan.output_path.display());
    println!(
        "video : {}x{} -> {}x{}",
        plan.source.width, plan.source.height, plan.target.width, plan.target.height
    );

    if let Some(frame_rate) = plan.source.frame_rate {
        println!("fps   : {frame_rate:.3}");
    }

    if let Some(pixel_format) = &plan.source.pixel_format {
        println!("pixfmt: {pixel_format}");
    }

    println!("scale : {}", plan.scale_label);

    match engine {
        RuntimeEngine::Ffmpeg => {
            println!("filter: {}", plan.filter_profile_label);
            println!(
                "preset: {} / crf {}",
                plan.preset.as_ffmpeg_value(),
                plan.crf
            );
            if let Some(command) = commands.first() {
                println!("cmd   : {command}");
            }
        }
        RuntimeEngine::FxUpscale => {
            println!("codec : h264 (fx-upscale)");
            for (index, command) in commands.iter().enumerate() {
                println!("cmd{}  : {}", index + 1, command);
            }
        }
        RuntimeEngine::Realesrgan => {
            println!(
                "preset: {} / crf {}",
                plan.preset.as_ffmpeg_value(),
                plan.crf
            );
            for (index, command) in commands.iter().enumerate() {
                println!("cmd{}  : {}", index + 1, command);
            }
        }
    }
}

fn print_json_plan(
    settings: &Settings,
    plan: &UpscalePlan,
    engine: RuntimeEngine,
    commands: &[String],
    status: &str,
    opened_in_iina: bool,
) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "mode": "upscale",
            "status": status,
            "dry_run": settings.dry_run,
            "config_path": settings.config_path.as_ref().map(|path| path.display().to_string()),
            "requested_engine": engine_name(settings.engine),
            "selected_engine": engine.label(),
            "input_path": plan.input_path.display().to_string(),
            "output_path": plan.output_path.display().to_string(),
            "source": {
                "width": plan.source.width,
                "height": plan.source.height,
                "frame_rate_expr": plan.source.frame_rate_expr,
                "frame_rate": plan.source.frame_rate,
                "pixel_format": plan.source.pixel_format,
            },
            "target": {
                "width": plan.target.width,
                "height": plan.target.height,
            },
            "resolved_options": {
                "scale": settings.scale,
                "crf": settings.crf,
                "preset": preset_name(settings.preset),
                "scaler": scaler_name(settings.scaler),
                "profile": filter_profile_name(settings.filter_profile),
                "audio_bitrate_kbps": settings.audio_bitrate_kbps,
                "overwrite": settings.overwrite,
                "open": settings.open,
                "output_dir": settings.output_dir.as_ref().map(|path| path.display().to_string()),
                "ffmpeg_bin": settings.ffmpeg_bin,
                "ffprobe_bin": settings.ffprobe_bin,
                "fx_upscale_bin": settings.fx_upscale_bin,
                "realesrgan_model": settings.realesrgan_model.as_binary_value(),
                "realesrgan_bin": settings.realesrgan_bin,
                "realesrgan_model_path": settings
                    .realesrgan_model_path
                    .as_ref()
                    .map(|path| path.display().to_string()),
                "realesrgan_tile": settings.realesrgan_tile,
                "realesrgan_tta": settings.realesrgan_tta,
            },
            "plan": {
                "scale_factor": plan.scale_factor,
                "scale_label": plan.scale_label,
                "filter_profile_label": json_filter_profile_label(engine, plan),
                "output_video_codec": output_video_codec(engine),
                "commands": commands,
            },
            "opened_in_iina": opened_in_iina,
        }))?
    );
    Ok(())
}

fn print_json_error(mode: &'static str, error: &anyhow::Error) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "mode": mode,
            "status": "error",
            "error": format!("{error:#}"),
        }))?
    );
    Ok(())
}

fn render_commands(settings: &Settings, plan: &UpscalePlan, engine: RuntimeEngine) -> Vec<String> {
    match engine {
        RuntimeEngine::Ffmpeg => vec![ffmpeg::render_command(
            &settings.ffmpeg_bin,
            settings.overwrite,
            plan,
        )],
        RuntimeEngine::FxUpscale => fx_upscale::render_commands(settings, plan),
        RuntimeEngine::Realesrgan => realesrgan::render_commands(settings, plan, &plan.source),
    }
}

fn resolve_engine(settings: &Settings, source: &ffmpeg::VideoMetadata) -> RuntimeEngine {
    match settings.engine {
        Engine::Ffmpeg => RuntimeEngine::Ffmpeg,
        Engine::FxUpscale => RuntimeEngine::FxUpscale,
        Engine::Realesrgan => RuntimeEngine::Realesrgan,
        Engine::Auto => {
            if source.width <= 640 && source.height <= 360 {
                if fx_upscale::is_available(settings) {
                    RuntimeEngine::FxUpscale
                } else if realesrgan::is_available(settings) {
                    RuntimeEngine::Realesrgan
                } else {
                    RuntimeEngine::Ffmpeg
                }
            } else {
                RuntimeEngine::Ffmpeg
            }
        }
    }
}

fn command_mode(cli: &Cli) -> &'static str {
    match &cli.command {
        Some(Command::Init(_)) => "init",
        None => "upscale",
    }
}

fn json_output(cli: &Cli) -> bool {
    match &cli.command {
        Some(Command::Init(args)) => args.json,
        None => cli.upscale.json,
    }
}

fn engine_name(engine: Engine) -> &'static str {
    match engine {
        Engine::Auto => "auto",
        Engine::Ffmpeg => "ffmpeg",
        Engine::FxUpscale => "fx-upscale",
        Engine::Realesrgan => "realesrgan",
    }
}

fn preset_name(preset: EncodePreset) -> &'static str {
    preset.as_ffmpeg_value()
}

fn scaler_name(scaler: Scaler) -> &'static str {
    scaler.as_ffmpeg_value()
}

fn filter_profile_name(profile: FilterProfile) -> &'static str {
    match profile {
        FilterProfile::Auto => "auto",
        FilterProfile::ScaleOnly => "scale-only",
        FilterProfile::Restore => "restore",
    }
}

fn json_filter_profile_label(engine: RuntimeEngine, plan: &UpscalePlan) -> Option<&str> {
    match engine {
        RuntimeEngine::Ffmpeg => Some(plan.filter_profile_label.as_str()),
        RuntimeEngine::FxUpscale | RuntimeEngine::Realesrgan => None,
    }
}

fn output_video_codec(engine: RuntimeEngine) -> &'static str {
    match engine {
        RuntimeEngine::Ffmpeg | RuntimeEngine::Realesrgan => "libx264",
        RuntimeEngine::FxUpscale => "h264",
    }
}

#[derive(Debug, Clone, Copy)]
enum RuntimeEngine {
    Ffmpeg,
    FxUpscale,
    Realesrgan,
}

impl RuntimeEngine {
    fn label(self) -> &'static str {
        match self {
            Self::Ffmpeg => "ffmpeg",
            Self::FxUpscale => "fx-upscale",
            Self::Realesrgan => "realesrgan",
        }
    }
}
