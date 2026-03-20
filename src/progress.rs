use std::io::IsTerminal;
use std::time::Duration;

use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

pub struct StageProgress {
    bar: Option<ProgressBar>,
    total_stages: u64,
}

const BAR_TICKS: u64 = 10_000;

impl StageProgress {
    pub fn new(total_stages: u64, enabled: bool) -> Self {
        let stderr_is_terminal = std::io::stderr().is_terminal();
        let stdout_is_terminal = std::io::stdout().is_terminal();

        if !enabled || (!stderr_is_terminal && !stdout_is_terminal) {
            return Self {
                bar: None,
                total_stages,
            };
        }

        let draw_target = if stderr_is_terminal {
            ProgressDrawTarget::stderr()
        } else {
            ProgressDrawTarget::stdout()
        };

        let bar = ProgressBar::with_draw_target(Some(BAR_TICKS), draw_target);
        bar.set_style(
            ProgressStyle::with_template("{spinner:.cyan} [{bar:28.cyan/blue}] {msg}")
                .expect("progress template should be valid")
                .progress_chars("=>-"),
        );
        bar.enable_steady_tick(Duration::from_millis(120));

        Self {
            bar: Some(bar),
            total_stages,
        }
    }

    pub fn set(&self, stage: u64, label: &str, fraction: f64) {
        if let Some(bar) = &self.bar {
            let clamped = fraction.clamp(0.0, 1.0);
            let ticks = (clamped * BAR_TICKS as f64).round() as u64;
            let percent = clamped * 100.0;
            bar.set_position(ticks);
            bar.set_message(format!("{percent:>6.2}% {stage}/{} {label}", self.total_stages));
        }
    }

    pub fn finish(&self, message: &str) {
        if let Some(bar) = &self.bar {
            bar.finish_with_message(message.to_string());
        }
    }

    pub fn abandon(&self) {
        if let Some(bar) = &self.bar {
            bar.abandon();
        }
    }
}
