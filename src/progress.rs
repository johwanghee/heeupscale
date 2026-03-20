use std::ffi::OsString;
use std::io::{BufReader, Read};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use indicatif::{ProgressBar, ProgressStyle};

pub struct StageProgress {
    bar: Option<ProgressBar>,
    total_stages: u64,
}

impl StageProgress {
    pub fn new(total_stages: u64, enabled: bool) -> Self {
        if !enabled {
            return Self {
                bar: None,
                total_stages,
            };
        }

        let bar = ProgressBar::new(100);
        bar.set_style(
            ProgressStyle::with_template("{spinner:.cyan} [{bar:28.cyan/blue}] {pos:>3}% {msg}")
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
            let percent = (fraction.clamp(0.0, 1.0) * 100.0).round() as u64;
            bar.set_position(percent);
            bar.set_message(format!("{stage}/{} {label}", self.total_stages));
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

pub fn run_command_with_percent_progress(
    binary: &str,
    args: &[OsString],
    progress: &StageProgress,
    stage: u64,
    label: &str,
) -> Result<()> {
    let mut command = Command::new(binary);
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .with_context(|| format!("failed to spawn `{binary}`"))?;

    let stdout = child
        .stdout
        .take()
        .with_context(|| format!("failed to capture stdout for `{binary}`"))?;
    let stderr = child
        .stderr
        .take()
        .with_context(|| format!("failed to capture stderr for `{binary}`"))?;

    let (sender, receiver) = mpsc::channel::<String>();
    let stdout_sender = sender.clone();
    let stdout_thread = thread::spawn(move || forward_progress_stream(stdout, stdout_sender));

    let stderr_sender = sender.clone();
    let stderr_thread = thread::spawn(move || forward_progress_stream(stderr, stderr_sender));

    drop(sender);
    progress.set(stage, label, 0.0);

    for line in receiver {
        if let Some(fraction) = parse_percent_fraction(&line) {
            progress.set(stage, label, fraction);
        }
    }

    let status = child
        .wait()
        .with_context(|| format!("failed to wait for `{binary}`"))?;

    let _ = stdout_thread.join();
    let _ = stderr_thread.join();

    if status.success() {
        progress.set(stage, label, 1.0);
        return Ok(());
    }

    bail!("`{binary}` exited with status {status}");
}

fn parse_percent_fraction(line: &str) -> Option<f64> {
    for token in line.split_whitespace().rev() {
        let Some(raw) = token.strip_suffix('%') else {
            continue;
        };
        let raw =
            raw.trim_matches(|character: char| !character.is_ascii_digit() && character != '.');

        if raw.is_empty() {
            continue;
        }

        if let Ok(value) = raw.parse::<f64>() {
            return Some((value / 100.0).clamp(0.0, 1.0));
        }
    }

    None
}

fn forward_progress_stream<R: Read>(reader: R, sender: mpsc::Sender<String>) {
    let mut reader = BufReader::new(reader);
    let mut buffer = Vec::new();
    let mut byte = [0u8; 1];

    loop {
        match reader.read(&mut byte) {
            Ok(0) => {
                flush_progress_buffer(&mut buffer, &sender);
                break;
            }
            Ok(_) => {
                if matches!(byte[0], b'\n' | b'\r') {
                    flush_progress_buffer(&mut buffer, &sender);
                } else {
                    buffer.push(byte[0]);
                }
            }
            Err(_) => break,
        }
    }
}

fn flush_progress_buffer(buffer: &mut Vec<u8>, sender: &mpsc::Sender<String>) {
    if buffer.is_empty() {
        return;
    }

    let line = String::from_utf8_lossy(buffer).trim().to_string();
    buffer.clear();
    if !line.is_empty() {
        let _ = sender.send(line);
    }
}

#[cfg(test)]
mod tests {
    use super::parse_percent_fraction;

    #[test]
    fn parses_plain_percent_line() {
        assert_eq!(parse_percent_fraction("12.50%"), Some(0.125));
    }

    #[test]
    fn parses_percent_token_with_extra_text() {
        assert_eq!(parse_percent_fraction("progress 87.50% done"), Some(0.875));
    }
}
