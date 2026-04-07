use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use super::log::Logger;

#[derive(Debug, Clone)]
pub struct ProcessSpec {
    program: String,
    args: Vec<String>,
}

impl ProcessSpec {
    pub fn new(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
        }
    }

    pub fn run(&self, logger: &Logger, dry_run: bool) -> Result<()> {
        logger.command(self.render());
        if dry_run {
            return Ok(());
        }

        let status = Command::new(&self.program)
            .args(&self.args)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .with_context(|| format!("failed to launch {}", self.program))?;

        if !status.success() {
            bail!("{} exited with status {status}", self.program);
        }

        Ok(())
    }

    pub fn render(&self) -> String {
        let mut parts = vec![shell_escape(&self.program)];
        parts.extend(self.args.iter().map(|arg| shell_escape(arg)));
        parts.join(" ")
    }
}

pub fn render_filter_chain(parts: &[String]) -> String {
    parts.join(",")
}

#[derive(Debug, Clone)]
pub struct MediaProbe {
    pub duration_seconds: Option<f64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub audio_streams: usize,
    pub rotation_degrees: i32,
}

pub fn probe_media(ffprobe_bin: &str, input: &std::path::Path) -> Result<MediaProbe> {
    let output = Command::new(ffprobe_bin)
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_streams",
            "-show_format",
        ])
        .arg(input)
        .output()
        .with_context(|| format!("failed to launch {ffprobe_bin}"))?;

    if !output.status.success() {
        bail!("ffprobe failed for {}", input.display());
    }

    let parsed: FfprobeOutput =
        serde_json::from_slice(&output.stdout).context("failed to parse ffprobe output")?;
    let duration_seconds = parsed
        .format
        .and_then(|format| format.duration)
        .and_then(|duration| duration.parse().ok());
    let video_codec = parsed
        .streams
        .iter()
        .find(|stream| stream.codec_type.as_deref() == Some("video"))
        .and_then(|stream| stream.codec_name.clone());
    let audio_codec = parsed
        .streams
        .iter()
        .find(|stream| stream.codec_type.as_deref() == Some("audio"))
        .and_then(|stream| stream.codec_name.clone());
    let audio_streams = parsed
        .streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("audio"))
        .count();
    let rotation_degrees = parsed
        .streams
        .iter()
        .find(|stream| stream.codec_type.as_deref() == Some("video"))
        .and_then(|stream| stream.side_data_list.as_ref())
        .and_then(|items| items.iter().find_map(|item| item.rotation))
        .unwrap_or(0);

    Ok(MediaProbe {
        duration_seconds,
        video_codec,
        audio_codec,
        audio_streams,
        rotation_degrees,
    })
}

pub fn is_mp4_video_compatible(codec: &str) -> bool {
    matches!(codec, "h264" | "hevc" | "mpeg4")
}

pub fn is_mp4_audio_compatible(codec: &str) -> bool {
    matches!(codec, "aac" | "mp3" | "ac3" | "alac")
}

fn shell_escape(value: &str) -> String {
    if value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "/._-+=:".contains(character))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

#[derive(Debug, Deserialize)]
struct FfprobeOutput {
    #[serde(default)]
    streams: Vec<FfprobeStream>,
    format: Option<FfprobeFormat>,
}

#[derive(Debug, Deserialize)]
struct FfprobeStream {
    codec_name: Option<String>,
    codec_type: Option<String>,
    side_data_list: Option<Vec<FfprobeSideData>>,
}

#[derive(Debug, Deserialize)]
struct FfprobeSideData {
    rotation: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct FfprobeFormat {
    duration: Option<String>,
}
