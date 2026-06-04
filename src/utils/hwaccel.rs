use std::collections::HashSet;
use std::process::Command;

use anyhow::{Context, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoEncoderKind {
    Software,
    VideoToolbox,
    Nvenc,
    Qsv,
    Amf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HwAccelCapabilities {
    pub encoder: VideoEncoderKind,
    pub encoder_name: String,
    pub hwaccel: Option<String>,
}

impl HwAccelCapabilities {
    pub fn software() -> Self {
        Self {
            encoder: VideoEncoderKind::Software,
            encoder_name: "libx264".into(),
            hwaccel: None,
        }
    }

    pub fn uses_hardware(&self) -> bool {
        self.encoder != VideoEncoderKind::Software
    }
}

#[derive(Debug, Clone)]
pub struct VideoEncodeSettings {
    pub crf: u8,
    pub x264_preset: Option<String>,
    pub video_bitrate: Option<String>,
    pub maxrate: Option<String>,
    pub bufsize: Option<String>,
}

pub fn detect_hw_capabilities(ffmpeg_bin: &str, enabled: bool) -> Result<HwAccelCapabilities> {
    if !enabled {
        return Ok(HwAccelCapabilities::software());
    }

    let encoders = list_encoders(ffmpeg_bin).unwrap_or_default();
    let hwaccels = list_hwaccels(ffmpeg_bin).unwrap_or_default();
    Ok(pick_encoder(&encoders, &hwaccels))
}

pub fn insert_hwaccel_before_input(args: &mut Vec<String>, caps: &HwAccelCapabilities) {
    let Some(hwaccel) = caps.hwaccel.as_deref() else {
        return;
    };

    let input_index = args.iter().position(|arg| arg == "-i");
    let Some(index) = input_index else {
        return;
    };

    let hwaccel_args = ["-hwaccel".to_string(), hwaccel.to_string()];
    args.splice(index..index, hwaccel_args);
}

pub fn append_h264_encode_args(
    args: &mut Vec<String>,
    caps: &HwAccelCapabilities,
    settings: &VideoEncodeSettings,
) {
    args.push("-c:v".into());
    args.push(caps.encoder_name.clone());

    if let Some(video_bitrate) = &settings.video_bitrate {
        append_bitrate_args(args, caps, video_bitrate, settings);
        return;
    }

    match caps.encoder {
        VideoEncoderKind::Software => {
            if let Some(preset) = &settings.x264_preset {
                args.extend(["-preset".into(), preset.clone()]);
            }
            args.extend([
                "-crf".into(),
                settings.crf.to_string(),
            ]);
        }
        VideoEncoderKind::VideoToolbox => {
            args.extend([
                "-q:v".into(),
                crf_to_videotoolbox_q(settings.crf).to_string(),
            ]);
            if settings
                .x264_preset
                .as_deref()
                .is_some_and(is_fast_preset)
            {
                args.extend(["-prio_speed".into(), "1".into()]);
            }
        }
        VideoEncoderKind::Nvenc => {
            args.extend([
                "-rc:v".into(),
                "vbr".into(),
                "-cq:v".into(),
                settings.crf.to_string(),
            ]);
            if let Some(preset) = map_nvenc_preset(settings.x264_preset.as_deref()) {
                args.extend(["-preset".into(), preset.into()]);
            }
        }
        VideoEncoderKind::Qsv => {
            args.extend([
                "-global_quality".into(),
                settings.crf.to_string(),
            ]);
            if let Some(preset) = map_qsv_preset(settings.x264_preset.as_deref()) {
                args.extend(["-preset".into(), preset.into()]);
            }
        }
        VideoEncoderKind::Amf => {
            args.extend([
                "-rc".into(),
                "cqp".into(),
                "-qp_i".into(),
                settings.crf.to_string(),
                "-qp_p".into(),
                settings.crf.to_string(),
            ]);
            if let Some(preset) = map_amf_quality(settings.x264_preset.as_deref()) {
                args.extend(["-quality".into(), preset.into()]);
            }
        }
    }
}

fn append_bitrate_args(
    args: &mut Vec<String>,
    caps: &HwAccelCapabilities,
    video_bitrate: &str,
    settings: &VideoEncodeSettings,
) {
    args.extend(["-b:v".into(), video_bitrate.into()]);
    if let Some(maxrate) = &settings.maxrate {
        args.push("-maxrate".into());
        args.push(maxrate.clone());
    }
    if let Some(bufsize) = &settings.bufsize {
        args.push("-bufsize".into());
        args.push(bufsize.clone());
    }
    if caps.encoder == VideoEncoderKind::VideoToolbox {
        args.extend(["-allow_sw".into(), "1".into()]);
    }
}

fn pick_encoder(encoders: &HashSet<String>, hwaccels: &HashSet<String>) -> HwAccelCapabilities {
    const CHOICES: &[(&str, VideoEncoderKind, Option<&str>)] = &[
        (
            "h264_videotoolbox",
            VideoEncoderKind::VideoToolbox,
            Some("videotoolbox"),
        ),
        ("h264_nvenc", VideoEncoderKind::Nvenc, Some("cuda")),
        ("h264_qsv", VideoEncoderKind::Qsv, Some("qsv")),
        ("h264_amf", VideoEncoderKind::Amf, None),
    ];

    for (name, kind, preferred_hwaccel) in CHOICES {
        if !encoders.contains(*name) {
            continue;
        }
        let hwaccel = preferred_hwaccel
            .and_then(|hwaccel| hwaccels.contains(hwaccel).then(|| hwaccel.to_string()))
            .or_else(|| fallback_hwaccel(*kind, hwaccels));
        return HwAccelCapabilities {
            encoder: *kind,
            encoder_name: name.to_string(),
            hwaccel,
        };
    }

    HwAccelCapabilities::software()
}

fn fallback_hwaccel(kind: VideoEncoderKind, hwaccels: &HashSet<String>) -> Option<String> {
    let candidates = match kind {
        VideoEncoderKind::Nvenc => ["cuda", "nvdec", "auto"],
        VideoEncoderKind::Amf => ["d3d11va", "dxva2", "auto"],
        _ => return None,
    };

    candidates
        .iter()
        .find(|name| hwaccels.contains(**name))
        .map(|name| (*name).to_string())
}

fn list_encoders(ffmpeg_bin: &str) -> Result<HashSet<String>> {
    let output = Command::new(ffmpeg_bin)
        .args(["-hide_banner", "-encoders"])
        .output()
        .with_context(|| format!("failed to launch {ffmpeg_bin}"))?;

    if !output.status.success() {
        anyhow::bail!("{ffmpeg_bin} -encoders failed");
    }

    Ok(parse_encoder_names(&String::from_utf8_lossy(&output.stdout)))
}

fn list_hwaccels(ffmpeg_bin: &str) -> Result<HashSet<String>> {
    let output = Command::new(ffmpeg_bin)
        .args(["-hide_banner", "-hwaccels"])
        .output()
        .with_context(|| format!("failed to launch {ffmpeg_bin}"))?;

    if !output.status.success() {
        anyhow::bail!("{ffmpeg_bin} -hwaccels failed");
    }

    Ok(parse_hwaccel_names(&String::from_utf8_lossy(&output.stdout)))
}

fn parse_encoder_names(output: &str) -> HashSet<String> {
    let mut encoders = HashSet::new();
    for line in output.lines() {
        let mut parts = line.split_whitespace();
        let Some(flags) = parts.next() else {
            continue;
        };
        if !flags.starts_with('V') {
            continue;
        }
        let Some(name) = parts.next() else {
            continue;
        };
        encoders.insert(name.to_string());
    }
    encoders
}

fn parse_hwaccel_names(output: &str) -> HashSet<String> {
    output
        .lines()
        .skip(1)
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn crf_to_videotoolbox_q(crf: u8) -> u8 {
    // VideoToolbox q:v is 1-100 with higher values meaning lower quality.
    (i16::from(crf) + 42).clamp(1, 100) as u8
}

fn is_fast_preset(preset: &str) -> bool {
    matches!(
        preset,
        "ultrafast" | "superfast" | "veryfast" | "faster" | "fast"
    )
}

fn map_nvenc_preset(preset: Option<&str>) -> Option<&'static str> {
    Some(match preset? {
        "ultrafast" | "superfast" | "veryfast" => "p1",
        "faster" | "fast" => "p3",
        "medium" => "p5",
        "slow" | "slower" | "veryslow" => "p7",
        _ => "p5",
    })
}

fn map_qsv_preset(preset: Option<&str>) -> Option<&'static str> {
    Some(match preset? {
        "ultrafast" | "superfast" | "veryfast" | "faster" | "fast" => "veryfast",
        "slow" | "slower" | "veryslow" => "veryslow",
        _ => "medium",
    })
}

fn map_amf_quality(preset: Option<&str>) -> Option<&'static str> {
    Some(match preset? {
        "ultrafast" | "superfast" | "veryfast" | "faster" | "fast" => "speed",
        "slow" | "slower" | "veryslow" => "quality",
        _ => "balanced",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_encoder_and_hwaccel_lists() {
        let encoders = parse_encoder_names(
            " Encoders:\n V..... h264_videotoolbox    VideoToolbox H.264 Encoder\n V..... libx264              libx264 H.264\n",
        );
        assert!(encoders.contains("h264_videotoolbox"));
        assert!(encoders.contains("libx264"));

        let hwaccels = parse_hwaccel_names("Hardware acceleration methods:\nvideotoolbox\ncuda\n");
        assert!(hwaccels.contains("videotoolbox"));
        assert!(hwaccels.contains("cuda"));
    }

    #[test]
    fn prefers_videotoolbox_on_mac_when_available() {
        let encoders = HashSet::from([
            "h264_videotoolbox".into(),
            "libx264".into(),
        ]);
        let hwaccels = HashSet::from(["videotoolbox".into()]);
        let caps = pick_encoder(&encoders, &hwaccels);
        assert_eq!(caps.encoder, VideoEncoderKind::VideoToolbox);
        assert_eq!(caps.encoder_name, "h264_videotoolbox");
        assert_eq!(caps.hwaccel.as_deref(), Some("videotoolbox"));
    }

    #[test]
    fn falls_back_to_software_when_no_hardware_encoder() {
        let encoders = HashSet::from(["libx264".into()]);
        let hwaccels = HashSet::new();
        let caps = pick_encoder(&encoders, &hwaccels);
        assert_eq!(caps, HwAccelCapabilities::software());
    }

    #[test]
    fn maps_crf_to_videotoolbox_quality() {
        assert_eq!(crf_to_videotoolbox_q(23), 65);
        assert_eq!(crf_to_videotoolbox_q(18), 60);
    }

    #[test]
    fn inserts_hwaccel_before_input() {
        let caps = HwAccelCapabilities {
            encoder: VideoEncoderKind::VideoToolbox,
            encoder_name: "h264_videotoolbox".into(),
            hwaccel: Some("videotoolbox".into()),
        };
        let mut args = vec![
            "-hide_banner".into(),
            "-y".into(),
            "-ss".into(),
            "10".into(),
            "-i".into(),
            "clip.mp4".into(),
        ];
        insert_hwaccel_before_input(&mut args, &caps);
        assert_eq!(
            args,
            vec![
                "-hide_banner",
                "-y",
                "-ss",
                "10",
                "-hwaccel",
                "videotoolbox",
                "-i",
                "clip.mp4"
            ]
        );
    }
}
