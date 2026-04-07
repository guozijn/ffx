use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "ffx",
    version,
    about = "Opinionated FFmpeg wrapper for everyday media workflows"
)]
pub struct Cli {
    #[arg(long, global = true, help = "Print commands without executing them")]
    pub dry_run: bool,

    #[arg(long, short, global = true, help = "Enable verbose logging")]
    pub verbose: bool,

    #[arg(
        long,
        short = 'j',
        global = true,
        default_value_t = default_jobs(),
        help = "Number of files to process concurrently"
    )]
    pub jobs: usize,

    #[arg(
        long,
        global = true,
        default_value = "ffmpeg",
        help = "Path to the ffmpeg binary"
    )]
    pub ffmpeg_bin: String,

    #[arg(
        long,
        global = true,
        default_value = "ffprobe",
        help = "Path to the ffprobe binary"
    )]
    pub ffprobe_bin: String,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Compress(CompressArgs),
    ToMp4(ToMp4Args),
    Gif(GifArgs),
    Audio(AudioArgs),
    Thumb(ThumbArgs),
    Cut(CutArgs),
}

#[derive(Debug, Clone, Args)]
pub struct OutputOptions {
    #[arg(long, short, help = "Write the result to this exact file path")]
    pub output: Option<PathBuf>,

    #[arg(long, help = "Write generated files into this directory")]
    pub output_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum Preset {
    Web,
    Discord,
    HighQuality,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum AudioFormat {
    Mp3,
    M4a,
}

#[derive(Debug, Clone, Args)]
pub struct CompressArgs {
    #[arg(required = true, help = "One or more input media files")]
    pub inputs: Vec<PathBuf>,

    #[command(flatten)]
    pub output: OutputOptions,

    #[arg(long, default_value_t = 23, help = "H.264 CRF value")]
    pub crf: u8,

    #[arg(long, default_value = "medium", help = "FFmpeg encoder preset")]
    pub speed: String,

    #[arg(long, help = "Disable automatic resize to a maximum of 1080p")]
    pub no_resize: bool,

    #[arg(long, help = "Target maximum output height")]
    pub max_height: Option<u32>,

    #[arg(long, value_enum, help = "Apply an opinionated compression preset")]
    pub preset: Option<Preset>,

    #[arg(long, help = "Target file size in megabytes; uses bitrate mode")]
    pub target_size_mb: Option<u64>,

    #[arg(
        long,
        default_value = "128k",
        help = "AAC bitrate for compressed audio"
    )]
    pub audio_bitrate: String,
}

#[derive(Debug, Clone, Args)]
pub struct ToMp4Args {
    #[arg(required = true, help = "One or more input media files")]
    pub inputs: Vec<PathBuf>,

    #[command(flatten)]
    pub output: OutputOptions,

    #[arg(long, help = "Force re-encoding instead of trying stream copy")]
    pub reencode: bool,

    #[arg(long, value_enum, help = "Encoding preset when re-encoding is needed")]
    pub preset: Option<Preset>,
}

#[derive(Debug, Clone, Args)]
pub struct GifArgs {
    #[arg(required = true, help = "One or more input video files")]
    pub inputs: Vec<PathBuf>,

    #[command(flatten)]
    pub output: OutputOptions,

    #[arg(long, default_value_t = 12, help = "GIF frame rate")]
    pub fps: u32,

    #[arg(long, default_value_t = 480, help = "Output width in pixels")]
    pub width: u32,

    #[arg(long, help = "Clip start time")]
    pub from: Option<String>,

    #[arg(long, help = "Clip duration")]
    pub duration: Option<String>,
}

#[derive(Debug, Clone, Args)]
pub struct AudioArgs {
    #[arg(required = true, help = "One or more input media files")]
    pub inputs: Vec<PathBuf>,

    #[command(flatten)]
    pub output: OutputOptions,

    #[arg(long, value_enum, default_value_t = AudioFormat::Mp3, help = "Audio output format")]
    pub format: AudioFormat,

    #[arg(long, default_value = "192k", help = "Target audio bitrate")]
    pub bitrate: String,
}

#[derive(Debug, Clone, Args)]
pub struct ThumbArgs {
    #[arg(required = true, help = "One or more input video files")]
    pub inputs: Vec<PathBuf>,

    #[command(flatten)]
    pub output: OutputOptions,

    #[arg(long, help = "Timestamp to capture")]
    pub at: Option<String>,

    #[arg(
        long,
        default_value_t = 1280,
        help = "Maximum width of the output image"
    )]
    pub width: u32,
}

#[derive(Debug, Clone, Args)]
pub struct CutArgs {
    #[arg(required = true, help = "One or more input media files")]
    pub inputs: Vec<PathBuf>,

    #[command(flatten)]
    pub output: OutputOptions,

    #[arg(long, help = "Trim start time")]
    pub from: Option<String>,

    #[arg(long, help = "Trim end time")]
    pub to: Option<String>,

    #[arg(
        long = "segment",
        value_name = "START-END",
        help = "Add a segment range; supports HH:MM:SS-HH:MM:SS or seconds-seconds"
    )]
    pub segments: Vec<String>,

    #[arg(long, help = "Write each segment to its own file instead of merging")]
    pub split: bool,

    #[arg(long, help = "Sort segments before validation and extraction")]
    pub sort_segments: bool,

    #[arg(long, help = "Always re-encode extracted content")]
    pub reencode: bool,

    #[arg(
        long,
        default_value_t = true,
        action = clap::ArgAction::Set,
        help = "Retry with re-encoding if stream copy fails"
    )]
    pub fallback_reencode: bool,
}

fn default_jobs() -> usize {
    std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(1)
}
