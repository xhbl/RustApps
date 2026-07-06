use serde::Deserialize;
use std::env;
use std::fmt;
use std::process::Command;
use std::sync::OnceLock;

// ─── ffprobe JSON response structs (raw, stringly-typed from ffprobe) ───

#[derive(Debug, Deserialize)]
struct FfprobeOutput {
    streams: Vec<Stream>,
    format: Format,
}

#[derive(Debug, Deserialize)]
struct Stream {
    #[serde(rename = "codec_type")]
    codec_type: String,
    #[serde(rename = "codec_name")]
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    #[serde(rename = "pix_fmt")]
    pix_fmt: Option<String>,
    #[serde(rename = "r_frame_rate")]
    r_frame_rate: Option<String>,
    #[serde(rename = "avg_frame_rate")]
    avg_frame_rate: Option<String>,
    #[serde(rename = "nb_frames")]
    nb_frames: Option<String>,
    duration: Option<String>,
    #[serde(rename = "bit_rate")]
    bit_rate: Option<String>,
    #[serde(rename = "display_aspect_ratio")]
    display_aspect_ratio: Option<String>,
    #[serde(rename = "sample_aspect_ratio")]
    sample_aspect_ratio: Option<String>,
    #[serde(rename = "color_primaries")]
    color_primaries: Option<String>,
    #[serde(rename = "color_transfer")]
    color_transfer: Option<String>,
    colorspace: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Format {
    duration: Option<String>,
    #[serde(rename = "bit_rate")]
    bit_rate: Option<String>,
}

// ─── Numeric domain types (exact, suitable for computation) ───

/// Frame rate as an exact rational number (num / den).
/// e.g. NTSC film = 24000/1001, NTSC video = 30000/1001, exact = 60/1.
/// Storing the numerator/denominator preserves precision that f64 would lose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FrameRate {
    num: u32,
    den: u32,
}

impl FrameRate {
    /// Construct a rational frame rate. Falls back to 0/1 if `den` is 0.
    fn new(num: u32, den: u32) -> Self {
        if den == 0 {
            FrameRate { num: 0, den: 1 }
        } else {
            FrameRate { num, den }
        }
    }

    /// Convert to f64 for floating-point calculations.
    fn as_f64(self) -> f64 {
        self.num as f64 / self.den as f64
    }
}

impl fmt::Display for FrameRate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Show exact rational plus decimal approximation, e.g. "24000/1001 (23.976 fps)"
        write!(f, "{}/{} ({:.3} fps)", self.num, self.den, self.as_f64())
    }
}

// ─── Video info data structure (all numeric, ready for computation) ───

/// Aggregated video information. All numeric fields use exact numeric types
/// so they can participate directly in further calculations; formatting is
/// applied only at display time via the `Display` impl.
#[derive(Debug)]
struct VideoInfo {
    codec: String,
    width: u32,
    height: u32,
    /// Display (playback) aspect ratio width/height, taking the source's DAR
    /// (and SAR fallback) into account. This is what viewers actually see.
    display_aspect: f64,
    bit_depth: u8,
    /// Original pixel format (e.g. "yuv420p", "yuv420p10le").
    pix_fmt: Option<String>,
    /// Declared color primaries (e.g. "bt709", "bt2020"), if any.
    color_primaries: Option<String>,
    /// Declared transfer characteristics (e.g. "bt709", "smpte2084").
    color_transfer: Option<String>,
    /// Declared colorspace/matrix (e.g. "bt709", "bt2020nc"), if any.
    colorspace: Option<String>,
    /// Frame rate as a rational (num / den): the exact nominal rate for CFR
    /// sources, or the average rate for VFR sources (used to compare against
    /// a `--setfps` target).
    frame_rate: FrameRate,
    /// Whether the source is constant frame rate (cfr) rather than variable
    /// (vfr); derived by comparing the nominal and average rates.
    cfr: bool,
    /// Total frame count if known exactly, otherwise None.
    total_frames: Option<u64>,
    /// Duration in seconds.
    duration: f64,
    /// Average bit rate in bits per second.
    bit_rate: u64,
}

impl VideoInfo {
    /// Frame rate as a floating-point value (convenience for calculations).
    fn fps(&self) -> f64 {
        self.frame_rate.as_f64()
    }

    /// Total frames if known exactly, otherwise estimated from duration × fps.
    fn estimated_total_frames(&self) -> u64 {
        match self.total_frames {
            Some(n) => n,
            None => (self.duration * self.fps()).round() as u64,
        }
    }

    /// Estimated file size in bytes = duration × bit_rate / 8.
    #[allow(dead_code)]
    fn estimated_file_size(&self) -> u64 {
        (self.duration * self.bit_rate as f64 / 8.0).round() as u64
    }

    /// Pixel count per frame = width × height.
    #[allow(dead_code)]
    fn pixel_count(&self) -> u64 {
        self.width as u64 * self.height as u64
    }
}

impl fmt::Display for VideoInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{:<14}: {}", "Codec", self.codec)?;
        writeln!(
            f,
            "{:<14}: {} x {}",
            "Resolution",
            self.width,
            self.height
        )?;
        writeln!(f, "{:<14}: {}", "Pixel Format", self.pix_fmt.as_deref().unwrap_or("unknown"))?;
        writeln!(f, "{:<14}: {} bit", "Bit Depth", self.bit_depth)?;
        // Colour summary: PQ/HLG transfer characteristics mean HDR, otherwise
        // the stream is ordinary SDR.
        let color_label = match self.color_transfer.as_deref() {
            Some(t) if is_hdr_transfer(t) => {
                let name = if t.eq_ignore_ascii_case("arib-std-b67") {
                    "HLG"
                } else {
                    "PQ"
                };
                format!("HDR ({name})")
            }
            _ => "SDR".to_string(),
        };
        writeln!(f, "{:<14}: {}", "Color", color_label)?;
        writeln!(f, "{:<14}: {:.3} fps", "Frame Rate", self.fps())?;
        match self.total_frames {
            Some(frames) => writeln!(f, "{:<14}: {}", "Total Frames", frames)?,
            None => writeln!(
                f,
                "{:<14}: ~{} (estimated)",
                "Total Frames",
                self.estimated_total_frames()
            )?,
        }
        writeln!(
            f,
            "{:<14}: {}",
            "Duration",
            format_duration(self.duration)
        )?;
        writeln!(
            f,
            "{:<14}: {}",
            "Avg Bitrate",
            format_bitrate(self.bit_rate)
        )?;
        Ok(())
    }
}

// ─── Formatting helpers (only used at display time) ───

/// Format seconds as HH:MM:SS.mmm
fn format_duration(secs: f64) -> String {
    let total_secs = secs as u64;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    let millis = ((secs - total_secs as f64) * 1000.0).round() as u64;
    format!("{:02}:{:02}:{:02}.{:03}", hours, minutes, seconds, millis)
}

/// Format a bitrate (in bps) into a human-readable form.
fn format_bitrate(bps: u64) -> String {
    if bps >= 1_000_000 {
        format!(
            "{:.2} Mbps ({:.0} bps)",
            bps as f64 / 1_000_000.0,
            bps
        )
    } else if bps >= 1_000 {
        format!(
            "{:.2} Kbps ({:.0} bps)",
            bps as f64 / 1_000.0,
            bps
        )
    } else {
        format!("{} bps", bps)
    }
}

// ─── Parsing helpers ───

/// Parse a frame rate string such as "30000/1001", "60/1" or "25"
/// into an exact `FrameRate` rational.
fn parse_frame_rate(s: &str) -> Option<FrameRate> {
    if let Some(pos) = s.find('/') {
        let num: u32 = s[..pos].parse().ok()?;
        let den: u32 = s[pos + 1..].parse().ok()?;
        Some(FrameRate::new(num, den))
    } else {
        let num: u32 = s.parse().ok()?;
        Some(FrameRate::new(num, 1))
    }
}

/// Standard CFR rates, in ascending order, paired with the `--setfps` label
/// that maps to each. These are the rates a source is snapped to when
/// normalising the output frame rate, and the only rates `--setfps` accepts.
const STANDARD_RATES: [(&str, FrameRate); 7] = [
    ("23.976", FrameRate { num: 24000, den: 1001 }), // NTSC film
    ("24", FrameRate { num: 24, den: 1 }),           // 24
    ("25", FrameRate { num: 25, den: 1 }),           // 25 (PAL)
    ("29.97", FrameRate { num: 30000, den: 1001 }),  // NTSC video
    ("30", FrameRate { num: 30, den: 1 }),           // 30
    ("59.97", FrameRate { num: 60000, den: 1001 }),  // 59.97
    ("60", FrameRate { num: 60, den: 1 }),           // 60
];

/// Snap a source frame rate to the closest standard CFR rate. Returns that
/// rate only when it is within ±0.1% of the source, so a genuinely unusual rate
/// (e.g. 12 fps) is left untouched rather than force-fit onto a standard value.
fn normalize_fps(src: FrameRate) -> Option<FrameRate> {
    let src_f = src.as_f64();
    STANDARD_RATES
        .iter()
        .map(|&(_, r)| r)
        .min_by(|a, b| (a.as_f64() - src_f).abs().total_cmp(&(b.as_f64() - src_f).abs()))
        .filter(|&closest| fps_nearly_equal(src, closest))
}

/// Parse a `--setfps` label (e.g. "23.976") into its exact rational frame rate
/// by looking it up in [`STANDARD_RATES`]; the NTSC decimal labels map to their
/// exact `num/1001` rationals (e.g. 23.976 → 24000/1001).
fn parse_setfps(s: &str) -> Option<FrameRate> {
    STANDARD_RATES
        .iter()
        .find(|(label, _)| *label == s)
        .map(|&(_, r)| r)
}

/// Whether the ratio `src/dst` lies within `[1 - tol, 1 + tol]`, compared
/// exactly on the rationals. `tol` is a pair `(num, den)` so e.g. ±5% is
/// `(5, 100)` and ±0.1% is `(1, 1000)`.
fn fps_within(src: FrameRate, dst: FrameRate, tol: (u32, u32)) -> bool {
    // src/dst = (src.num * dst.den) / (src.den * dst.num) = a/b.
    // 1-tol <= a/b <= 1+tol  <=>  lo*b <= lo_den*a && hi*b >= hi_den*a,
    // with lo = (tol.den - tol.num)/tol.den and hi = (tol.den + tol.num)/tol.den.
    let (lo_num, lo_den) = (tol.1 - tol.0, tol.1);
    let (hi_num, hi_den) = (tol.1 + tol.0, tol.1);
    let a = src.num as u64 * dst.den as u64;
    let b = src.den as u64 * dst.num as u64;
    lo_num as u64 * b <= lo_den as u64 * a && hi_num as u64 * b >= hi_den as u64 * a
}

/// Whether retiming from `src` to `dst` is within the ±5% window. Used to
/// validate an explicit `--setfps` target.
fn fps_close(src: FrameRate, dst: FrameRate) -> bool {
    fps_within(src, dst, (5, 100))
}

/// Whether `src` and `dst` differ by at most 0.1%. Used for silent automatic
/// normalisation (no `--setfps`), which must be stricter so it only snaps
/// rates already effectively equal to the target (e.g. the 23.976↔24 /
/// 29.97↔30 / 59.97↔60 pairs, ratio 0.999001).
fn fps_nearly_equal(src: FrameRate, dst: FrameRate) -> bool {
    fps_within(src, dst, (1, 1000))
}

/// Resolve an external program name to an executable path. A copy sitting next
/// to this binary (e.g. a bundled ffmpeg.exe) is preferred over the system
/// PATH, so the tool runs with its own helpers when present; otherwise the
/// bare name is returned and the OS resolves it via PATH.
fn resolve_binary(name: &str) -> String {
    if let Some(dir) = env::current_exe().ok().as_deref().and_then(std::path::Path::parent) {
        let candidate = dir.join(format!("{}{}", name, env::consts::EXE_SUFFIX));
        if candidate.is_file() {
            return candidate.to_string_lossy().into_owned();
        }
    }
    name.to_string()
}

/// Parse a ratio string such as "16:9" or "32:27" into a float value.
fn parse_ratio(s: &str) -> Option<f64> {
    if let Some(pos) = s.find(':') {
        let num: f64 = s[..pos].parse().ok()?;
        let den: f64 = s[pos + 1..].parse().ok()?;
        if den == 0.0 {
            None
        } else {
            Some(num / den)
        }
    } else {
        s.parse().ok()
    }
}

/// Compute the display (playback) aspect ratio width/height. Prefers the
/// stream's declared display aspect ratio (DAR); otherwise derives it from the
/// coded dimensions scaled by the sample aspect ratio (SAR). Either way this
/// is the ratio viewers actually see, so it is the one used for scaling.
fn source_display_aspect(
    width: u32,
    height: u32,
    dar: Option<&String>,
    sar: Option<&String>,
) -> f64 {
    let coded = width as f64 / height as f64;
    if let Some(r) = dar.and_then(|s| parse_ratio(s)) {
        return r;
    }
    if let Some(r) = sar.and_then(|s| parse_ratio(s)) {
        return coded * r;
    }
    coded
}

/// Infer bit depth from a pixel format name.
fn infer_bit_depth(pix_fmt: &str) -> u8 {
    // Common formats: yuv420p (8-bit), yuv420p10le (10-bit), yuv420p12le (12-bit), etc.
    // First strip the le/be suffix
    let rest = pix_fmt
        .strip_suffix("le")
        .or_else(|| pix_fmt.strip_suffix("be"))
        .unwrap_or(pix_fmt);

    // Extract trailing digits
    let digits: String = rest.chars().rev().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() {
        let depth: u8 = digits.chars().rev().collect::<String>().parse().unwrap_or(8);
        if (8..=16).contains(&depth) {
            return depth;
        }
    }

    8 // default 8-bit
}

/// Invoke ffprobe to extract video information.
fn get_video_info(path: &str) -> Result<VideoInfo, Box<dyn std::error::Error>> {
    let output = Command::new(resolve_binary("ffprobe"))
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            path,
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffprobe execution failed: {}", stderr).into());
    }

    let probe: FfprobeOutput = serde_json::from_slice(&output.stdout)?;

    // Find the first video stream
    let video_stream = probe
        .streams
        .iter()
        .find(|s| s.codec_type == "video")
        .ok_or("no video stream found")?;

    let codec = video_stream
        .codec_name
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let width = video_stream.width.ok_or("missing width information")?;
    let height = video_stream.height.ok_or("missing height information")?;

    let bit_depth = video_stream
        .pix_fmt
        .as_ref()
        .map(|s| infer_bit_depth(s))
        .unwrap_or(8);

    let rfr = video_stream
        .r_frame_rate
        .as_ref()
        .and_then(|s| parse_frame_rate(s));
    // avg_frame_rate is the real average; skip it when unknown ("0/0").
    let afr = video_stream
        .avg_frame_rate
        .as_ref()
        .and_then(|s| parse_frame_rate(s))
        .filter(|f| f.num != 0);
    // CFR when the nominal and average rates agree (or when only one is known).
    let cfr = match (rfr, afr) {
        (Some(r), Some(a)) => r == a,
        _ => true,
    };
    // CFR sources report their exact rate in both fields; VFR sources only
    // have a meaningful rate in avg_frame_rate, so prefer that when retiming.
    let frame_rate = if cfr { rfr.or(afr) } else { afr.or(rfr) }
        .ok_or("missing frame rate information")?;

    // Duration: prefer stream-level, fall back to format-level
    let duration = video_stream
        .duration
        .as_ref()
        .and_then(|s| s.parse::<f64>().ok())
        .or_else(|| {
            probe
                .format
                .duration
                .as_ref()
                .and_then(|s| s.parse::<f64>().ok())
        })
        .ok_or("missing duration information")?;

    // Prefer nb_frames from the stream, otherwise leave as None (estimable on demand).
    let total_frames = video_stream
        .nb_frames
        .as_ref()
        .and_then(|s| s.parse::<u64>().ok());

    let bit_rate = video_stream
        .bit_rate
        .as_ref()
        .or(probe.format.bit_rate.as_ref())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let display_aspect = source_display_aspect(
        width,
        height,
        video_stream.display_aspect_ratio.as_ref(),
        video_stream.sample_aspect_ratio.as_ref(),
    );

    Ok(VideoInfo {
        codec,
        width,
        height,
        display_aspect,
        bit_depth,
        pix_fmt: video_stream.pix_fmt.clone(),
        color_primaries: video_stream.color_primaries.clone(),
        color_transfer: video_stream.color_transfer.clone(),
        colorspace: video_stream.colorspace.clone(),
        frame_rate,
        cfr,
        total_frames,
        duration,
        bit_rate,
    })
}

// ─── CLI preprocessing filters: build the ffmpeg -vf filter string ───

/// Target canvas resolution preset (all 16:9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RescaleTarget {
    P480,
    P720,
    P1080,
    P1440,
    P2160,
    P2880,
    P4320,
}

impl RescaleTarget {
    /// Parse a preset string such as `"1080p"` (case-insensitive).
    fn parse(s: &str) -> Option<RescaleTarget> {
        match s.to_ascii_lowercase().as_str() {
            "480p" => Some(Self::P480),
            "720p" => Some(Self::P720),
            "1080p" => Some(Self::P1080),
            "1440p" => Some(Self::P1440),
            "2160p" => Some(Self::P2160),
            "2880p" => Some(Self::P2880),
            "4320p" => Some(Self::P4320),
            _ => None,
        }
    }

    /// Target canvas (width, height).
    fn size(self) -> (u32, u32) {
        match self {
            Self::P480 => (854, 480),
            Self::P720 => (1280, 720),
            Self::P1080 => (1920, 1080),
            Self::P1440 => (2560, 1440),
            Self::P2160 => (3840, 2160),
            Self::P2880 => (5120, 2880),
            Self::P4320 => (7680, 4320),
        }
    }
}

/// Round a value down to the nearest even number (minimum 2).
///
/// yuv420p and other 4:2:0 chroma-subsampled formats require both width and
/// height to be even, otherwise the encoder rejects the frame. Every size this
/// tool emits (scale target and pad canvas) must therefore be even.
fn even(v: i64) -> i64 {
    (v & !1).max(2)
}

/// True if the transfer characteristic is an HDR EOTF needing tone mapping.
/// PQ (SMPTE ST 2084) and HLG (ARIB STD-B67).
fn is_hdr_transfer(transfer: &str) -> bool {
    transfer.eq_ignore_ascii_case("smpte2084") || transfer.eq_ignore_ascii_case("arib-std-b67")
}

/// Whether a metadata value is BT.709 (missing/unknown metadata is treated as
/// BT.709, i.e. the ordinary SDR default).
fn is_bt709(v: Option<&String>) -> bool {
    v.is_none_or(|s| s.eq_ignore_ascii_case("bt709"))
}

/// Detect (cached, once per run) whether the local ffmpeg build provides the
/// `zscale` filter (libzimg). When present we use the higher-quality z.lib
/// conversion path, otherwise we fall back to the built-in `colorspace`.
fn ffmpeg_supports_zscale() -> bool {
    static HAS_ZSCALE: OnceLock<bool> = OnceLock::new();
    *HAS_ZSCALE.get_or_init(|| {
        Command::new(resolve_binary("ffmpeg"))
            .args(["-hide_banner", "-filters"])
            .output()
            // `-filters` rows look like: " TSC zscale  V->V Video resampler"
            .map(|o| {
                let out = String::from_utf8_lossy(&o.stdout);
                out.lines()
                    .any(|l| l.split_whitespace().nth(1) == Some("zscale"))
            })
            .unwrap_or(false)
    })
}

/// Detect (cached, once per run) whether the local ffmpeg build can actually
/// run the `libplacebo` filter against an initialised Vulkan device. libplacebo
/// is the same library mpv uses for its high-quality HDR→SDR tone mapping
/// (BT.2390 EETF + perceptual gamut mapping), so it is preferred when the
/// GPU/driver stack cooperates. The probe runs a throwaway one-frame graph: if
/// Vulkan cannot initialise (no GPU, missing driver, or a build without
/// libplacebo) the command exits non-zero and we fall back to the CPU path.
fn ffmpeg_supports_libplacebo() -> bool {
    static HAS_LIBPLACEBO: OnceLock<bool> = OnceLock::new();
    *HAS_LIBPLACEBO.get_or_init(|| {
        Command::new(resolve_binary("ffmpeg"))
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-init_hw_device",
                "vulkan=vk",
                "-f",
                "lavfi",
                "-i",
                "nullsrc=s=2x2:r=1",
                "-frames:v",
                "1",
                "-vf",
                "format=yuv420p,hwupload,libplacebo=format=yuv420p,hwdownload,format=yuv420p",
                "-f",
                "null",
                "-",
            ])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}

/// Build the colour-conversion filter part that brings a source to the fixed
/// target **BT.709 SDR, 8-bit 4:2:0 (yuv420p)**. Returns `None` when the
/// source already conforms.
///
/// For HDR (PQ/HLG) sources the tone mapping must operate in **linear light**,
/// so the chain linearizes first, merging the BT.2020→BT.709 primaries
/// conversion into the same zscale pass (zimg applies transfer decode before
/// the gamut matrix) to save a full-resolution resample:
///   `zscale=transfer=linear:primaries=bt709` → `format=gbrpf32le` (float) →
///   `tonemap=mobius` → `zscale=transfer=bt709:matrix=bt709:range=tv` →
///   `format=yuv420p`.
///
/// Non-HDR sources that still differ only get the colourspace/pixel-format
/// normalisation part (`zscale ...` or `colorspace=all=bt709`). Without zscale,
/// HDR falls back to an approximate tone map in the encoded domain (ffmpeg
/// warns "linear light only", but it is the best available on such builds).
fn build_color_filters(info: &VideoInfo, use_libplacebo: bool) -> Option<String> {
    let transfer = info.color_transfer.as_deref();
    let is_hdr = transfer.is_some_and(is_hdr_transfer);
    let is_8bit_420 = info.pix_fmt.as_deref() == Some("yuv420p");
    let already_bt709 =
        is_bt709(info.color_primaries.as_ref()) && is_bt709(info.colorspace.as_ref());

    if !is_hdr && is_8bit_420 && already_bt709 {
        return None;
    }

    let mut parts = Vec::new();

    if is_hdr {
        if use_libplacebo && ffmpeg_supports_libplacebo() {
            // Preferred: libplacebo's GPU pipeline (the same BT.2390 EETF +
            // perceptual gamut mapping mpv uses). It reads the source's colour
            // metadata itself, so only the SDR target tags are fixed here.
            parts.push("hwupload".to_string());
            parts.push(
                "libplacebo=tonemapping=bt.2390:colorspace=bt709:\
                 color_primaries=bt709:color_trc=bt709:gamut_mode=desaturate:format=yuv420p"
                    .to_string(),
            );
            parts.push("hwdownload".to_string());
            parts.push("format=yuv420p".to_string());
        } else if ffmpeg_supports_zscale() {
            // CPU fallback: pq/hlg → linear light → mobius tone map → BT.709.
            let tin = if transfer.is_some_and(|t| t.eq_ignore_ascii_case("arib-std-b67")) {
                "tin=arib-std-b67"
            } else {
                "tin=smpte2084:npl=100"
            };
            parts.push(format!("zscale=transfer=linear:primaries=bt709:{tin}"));
            parts.push("format=gbrpf32le".to_string());
            parts.push("tonemap=mobius:desat=0".to_string());
            parts.push("zscale=transfer=bt709:matrix=bt709:range=tv".to_string());
            parts.push("format=yuv420p".to_string());
        } else {
            // No zscale: approximate tone map in the encoded domain (ffmpeg
            // warns "linear light only", but it is the best available).
            parts.push("tonemap=mobius:desat=0".to_string());
            parts.push("format=yuv420p".to_string());
        }
        // After conversion the frame colour metadata can still report the
        // source's BT.2020/PQ (notably after the GPU libplacebo round-trip),
        // which libx265 then honours and misdetects as HDR. Force BT.709 tags
        // so downstream encoders see SDR instead.
        parts.push(
            "setparams=color_primaries=bt709:color_trc=bt709:colorspace=bt709:range=limited"
                .to_string(),
        );
        return Some(parts.join(","));
    }

    // Non-HDR source that still needs colourspace/pixel-format normalisation.
    if ffmpeg_supports_zscale() {
        parts.push(
            "zscale=primaries=bt709:transfer=bt709:matrix=bt709:range=limited".to_string(),
        );
    } else {
        parts.push("colorspace=all=bt709:range=tv".to_string());
    }
    parts.push("format=yuv420p".to_string());

    Some(parts.join(","))
}

/// Compute the geometry (scale/pad) filter parts for a target canvas. Returns
/// the parts plus whether any geometry change is actually needed.
fn compute_geometry(
    target: RescaleTarget,
    letterbox: bool,
    pillarbox: bool,
    info: &VideoInfo,
) -> (Vec<String>, bool, (u32, u32)) {
    let (w, h) = target.size();
    let canvas_aspect = w as f64 / h as f64;
    let diameter = info.display_aspect;

    // Wider-than-canvas sources are width-bound (letterbox bars only);
    // narrower sources are height-bound (pillarbox bars only).
    let source_wider = diameter > canvas_aspect;
    let pad_requested = if source_wider { letterbox } else { pillarbox };

    // Destination coded-pixel size derived from the display aspect (so DAR is
    // honoured), forced even for 4:2:0 encoders via `even()`. Scaling is
    // bidirectional.
    let (ow, oh) = if source_wider {
        (w as i64, even((w as f64 / diameter).round() as i64))
    } else {
        (even((h as f64 * diameter).round() as i64), h as i64)
    };

    let scale_needed = ow as u32 != info.width || oh as u32 != info.height;

    // Only pad when there is actual leftover on the bound axis.
    let leftover = if source_wider {
        oh < h as i64
    } else {
        ow < w as i64
    };
    let pad_needed = pad_requested && leftover;

    let mut parts = Vec::new();
    if scale_needed {
        parts.push(format!("scale={ow}:{oh}:flags=lanczos"));
    }
    if pad_needed {
        parts.push(format!("pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:black"));
    }

    // Actual output frame size: the canvas when bars are added, otherwise the
    // scaled (source-equivalent) coded size.
    let out_dims = if pad_needed { (w, h) } else { (ow as u32, oh as u32) };

    (parts, scale_needed || pad_needed, out_dims)
}

/// Build the ffmpeg `-vf` filter string for video preprocessing, or `None`
/// when nothing needs to be done (source already matches the target geometry
/// and colourspace, with no effective padding flag).
///
/// The destination size is derived from the source's **display aspect ratio**
/// (`info.display_aspect`), so a file whose coded dimensions differ from its
/// DAR (e.g. 1440×1080 with DAR 16:9) is treated by its on-screen ratio and
/// lands on the canvas correctly (that example → 1280×720 for `-rs 720p`).
///
/// Black bars are added only to the axis that matches the source orientation
/// and only when that axis actually has leftover space:
///   - Source **wider** than the canvas: width-bound → `--letterbox`.
///   - Source **narrower** than the canvas: height-bound → `--pillarbox`.
///
/// The colour part ([`build_color_filters`]) brings the video to BT.709 SDR
/// 8-bit 4:2:0 whenever the source differs. Filter order:
/// geometry → tone map → colourspace → pixel format → SAR.
fn build_vf_filter(
    target: Option<RescaleTarget>,
    letterbox: bool,
    pillarbox: bool,
    setfps: Option<FrameRate>,
    info: &VideoInfo,
    use_libplacebo: bool,
) -> Option<String> {
    let (mut parts, geometry_needed, _out_dims) = match target {
        Some(target) => compute_geometry(target, letterbox, pillarbox, info),
        None => (Vec::new(), false, (info.width, info.height)),
    };
    let color_needed = build_color_filters(info, use_libplacebo);

    // Retime first: re-stamp each frame to the requested rate without adding
    // or dropping frames (the frame count is preserved; only the duration
    // changes). Placed before the spatial/colour filters.
    if let Some(fps) = setfps {
        parts.insert(0, format!("setpts=N*{}/TB/{}", fps.den, fps.num));
    }

    // Nothing to do: target geometry already matches, colour is conforming and
    // no frame-rate retiming was requested.
    if !geometry_needed && color_needed.is_none() && setfps.is_none() {
        return None;
    }

    if let Some(color) = color_needed {
        // Tone-map / colourspace-convert / fix pixel format (in that order),
        // keeping HDR→SDR before subsampling to retain detail.
        parts.push(color);
    }
    parts.push("setsar=1".to_string());

    Some(parts.join(","))
}

/// Build only the geometry part (scale/pad + setsar), used by mpv which does
/// its own HDR→SDR tone mapping via `--vo=gpu-next`.
fn build_geometry_vf(
    target: RescaleTarget,
    letterbox: bool,
    pillarbox: bool,
    info: &VideoInfo,
) -> Option<String> {
    let (mut parts, geometry_needed, _out_dims) = compute_geometry(target, letterbox, pillarbox, info);
    if !geometry_needed {
        return None;
    }
    parts.push("setsar=1".to_string());
    Some(parts.join(","))
}

fn print_usage() {
    eprintln!(
        "{} v{} - {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_DESCRIPTION")
    );
    eprintln!(
        "by {}. This program is released under the {} License",
        env!("CARGO_PKG_AUTHORS"),
        env!("CARGO_PKG_LICENSE")
    );
    eprintln!();
    eprintln!("Usage: {} [options] <video file path>", env!("CARGO_PKG_NAME"));
    eprintln!("Options:");
    eprintln!("  -rs, --rescale <preset>  Rescale to a target canvas. Presets:");
    eprintln!("                          480p 720p 1080p 1440p 2160p 2880p 4320p");
    eprintln!("  -lb, --letterbox         Pad with black bars (top/bottom) to the preset canvas.");
    eprintln!("  -pb, --pillarbox         Pad with black bars (left/right) to the preset canvas.");
    eprintln!("  -sf, --setfps <fps>      Retime (no frame drop/dup) to a standard rate:");
    eprintln!("                          23.976/24/25/29.97/30/59.97/60 (only when the");
    eprintln!("                          source fps is within ±5% of the target).");
    eprintln!("  -pp, --playpreview       Play the stream (prefers mpv, falls back to ffplay).");
    eprintln!("  -op, --outpipe           Pipe decoded/preprocessed yuv4mpeg stream to stdout.");
    eprintln!("  -rc, --recode <kbps>     Re-encode 2-pass; requires --outfile.");
    eprintln!("  -ec, --encoder <name>    Encoder for --recode: x265 (default) or x264.");
    eprintln!("  -of, --outfile <path>    Output file for --recode (.mkv/.mp4/.hevc, or no dot = raw).");
    eprintln!("  -h, --help               Show this help message.");
    eprintln!();
    eprintln!("Hardware decoding is auto-detected in the emitted ffmpeg command via");
    eprintln!("`-hwaccel auto` (NVIDIA CUDA / AMD AMF / Intel QSV / DXVA2 / D3D11VA),");
    eprintln!("with an automatic fallback to software decode if unavailable.");
    eprintln!("For HDR→SDR tone mapping, the libplacebo GPU pipeline (BT.2390, the same");
    eprintln!("algorithm mpv uses) is auto-detected and preferred; otherwise it falls");
    eprintln!("back to a CPU zscale+tonemap (mobius) chain.");
    eprintln!("When any preprocessing option is given without -op, the program prints the");
    eprintln!("ffmpeg -vf filter string plus a ready-to-paste ffmpeg decode+preprocess");
    eprintln!("command that you can pipe into x265 or other encoders.");
    eprintln!("With -op the ffmpeg pipeline is executed instead: the yuv4mpeg stream is");
    eprintln!("written to stdout for direct piping (e.g. `{} ... -op in.mkv | x265 --y4m -`).", env!("CARGO_PKG_NAME"));
}

/// Print an error message and exit with a non-zero status.
fn fail(msg: &str) -> ! {
    eprintln!("Error: {}", msg);
    std::process::exit(1);
}

/// Build the auto-detect hardware-decode prefix for *formatting* into the
/// printable command string (see [`build_ffmpeg_command`]).
fn build_decode_prefix() -> &'static str {
    "-hwaccel auto "
}

/// Same as [`build_decode_prefix`] but as owned tokens for direct
/// [`Command::arg`] calls. Keeping them in sync avoids drift between the
/// printed command and what `-op` actually runs.
fn build_decode_prefix_tokens() -> Vec<&'static str> {
    vec!["-hwaccel", "auto"]
}

/// Build a ready-to-paste ffmpeg decode + preprocess command. Returns None
/// only when neither hwaccel nor a -vf filter is present (trivial case already
/// covered by the "No preprocessing needed" branch).
///
/// The command outputs raw video via `-f yuv4mpegpipe` to stdout so the user
/// can pipe it directly into x265: `x265 --y4m - --output out.hevc`.
fn build_ffmpeg_command(
    input_path: &str,
    vf: &str,
    _info: &VideoInfo,
    fps: Option<FrameRate>,
) -> Option<String> {
    // The libplacebo path needs a Vulkan device initialised for its uploads.
    let vulkan = if vf.contains("libplacebo") {
        "-init_hw_device vulkan=vk "
    } else {
        ""
    };
    let prefix = build_decode_prefix();
    let input = shell_escape(input_path);
    let rate = fps
        .map(|f| format!(" -r {}/{}", f.num, f.den))
        .unwrap_or_default();
    Some(format!(
        "ffmpeg -hide_banner -loglevel error {vulkan}{prefix}-i {input}{rate} \
         -vf \"{vf}\" \
         -pix_fmt yuv420p -colorspace bt709 -color_primaries bt709 -color_trc bt709 \
         -f yuv4mpegpipe -"
    ))
}

/// Minimal shell-safe path escape: wrap in double quotes and double any
/// embedded double-quotes. Covers the paths our CLI already accepts.
fn shell_escape(s: &str) -> String {
    let escaped = s.replace('"', "\"\"");
    format!("\"{escaped}\"")
}

/// Execute ffmpeg and stream its decoded/preprocessed yuv4mpeg output directly
/// to our own stdout — exactly the shape [`build_ffmpeg_command`] prints as a
/// string. Intended for shell pipelines such as:
///   `xwaf -rs 1080p -op in.mkv | x265 --y4m -o out.hevc -`
///
/// Stdin/stderr are inherited so ffmpeg progress/errors and `q`/signals still
/// work; stdout is forwarded untouched to the caller for downstream encoding.
/// The function does not return — it exits with ffmpeg's own exit code.
fn run_ffmpeg_pipe(input_path: &str, vf: Option<&str>, fps: Option<FrameRate>) -> ! {
    let mut cmd = Command::new(resolve_binary("ffmpeg"));
    cmd.args(["-hide_banner", "-loglevel", "error"]);
    if vf.map(|f| f.contains("libplacebo")).unwrap_or(false) {
        cmd.args(["-init_hw_device", "vulkan=vk"]);
    }
    for tok in build_decode_prefix_tokens() {
        cmd.arg(tok);
    }
    cmd.args(["-i", input_path]);
    if let Some(fps) = fps {
        let rate = format!("{}/{}", fps.num, fps.den);
        cmd.arg("-r").arg(&rate);
    }
    if let Some(f) = vf {
        cmd.args(["-vf", f]);
    }
    cmd.args([
        "-pix_fmt", "yuv420p",
        "-colorspace", "bt709",
        "-color_primaries", "bt709",
        "-color_trc", "bt709",
        "-f", "yuv4mpegpipe",
        "-",
    ]);
    cmd.stdin(std::process::Stdio::inherit());
    cmd.stderr(std::process::Stdio::inherit());
    cmd.stdout(std::process::Stdio::inherit());

    let status = match cmd.status() {
        Ok(s) => s,
        Err(e) => fail(&format!("failed to launch ffmpeg: {}", e)),
    };
    std::process::exit(status.code().unwrap_or(1));
}

/// Derive the x265 2-pass stats file name: the output file name with its
/// extension stripped, plus a `.stats` suffix. Handles both `output.mkv` and
/// a raw `output` (no extension) uniformly.
fn stats_filename(outfile: &str) -> String {
    match outfile.rfind('.') {
        Some(pos) if pos > 0 => format!("{}.stats", &outfile[..pos]),
        _ => format!("{outfile}.stats"),
    }
}

/// Join already-tokenised args back into a single pasteable command line,
/// quoting tokens that contain whitespace or quotes.
fn shell_join(args: &[String]) -> String {
    args.iter()
        .map(|a| {
            if a.chars().any(|c| c.is_whitespace() || c == '"') {
                shell_escape(a)
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Return an appropriate HEVC level-idc string for a target output resolution,
/// so the stream's declared level scales with the size instead of being pinned
/// to level 4. Thresholds follow HEVC's maximum luma picture size per level.
fn hevc_level_for_size(width: u64, height: u64) -> &'static str {
    match width * height {
        0..=245_760 => "2.1",                 // up to ~360p
        245_761..=552_960 => "3",             // up to ~480p
        552_961..=983_040 => "3.1",           // up to ~720p
        983_041..=2_228_224 => "4",           // up to ~1080p
        2_228_225..=8_912_896 => "5",         // up to ~2160p
        8_912_897..=35_651_584 => "6",        // up to ~4320p
        _ => "6.2",
    }
}

/// Video encoder selection for `--recode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Encoder {
    X265,
    X264,
}

impl Encoder {
    fn parse(s: &str) -> Option<Encoder> {
        match s.to_ascii_lowercase().as_str() {
            "x265" => Some(Self::X265),
            "x264" => Some(Self::X264),
            _ => None,
        }
    }

    /// The ffmpeg `-c:v` library name for this encoder.
    fn lib_name(self) -> &'static str {
        match self {
            Self::X265 => "libx265",
            Self::X264 => "libx264",
        }
    }

    fn is_x264(self) -> bool {
        matches!(self, Self::X264)
    }
}

/// Build the `-x265-params` value for a given 2-pass libx265 encode. Common
/// BT.709 SDR metadata and quality flags are pinned; pass 1 adds the cheap
/// "turbo first-pass" suboptions so pass 1 runs much faster.
fn recode_x265_params(pass: u8, stats: &str, level: &str) -> String {
    let mut p = String::new();
    p.push_str("pass=");
    p.push_str(if pass == 1 { "1" } else { "2" });
    p.push_str(":stats=");
    p.push_str(stats);
    p.push_str(":level-idc=");
    p.push_str(level);
    p.push_str(":aq-mode=3:sar=1:range=limited");
    p.push_str(":colorprim=bt709:transfer=bt709:colormatrix=bt709");
    if pass == 1 {
        p.push_str(":subme=1:me=hex:rd=2:rect=0:amp=0");
    }
    p.push_str(":no-info=1");
    p
}

/// Build the `-x264-params` value for a given 2-pass libx264 encode, mirroring
/// the x265 path. Pass 1 uses the standard "turbo first-pass" overrides (cheap
/// ME, no psychovisual extras) while keeping `bframes` fixed; pass 2 applies the
/// full quality settings supplied by the user (ref/mixed-refs/b-adapt/subme/
/// trellis/partitions/8x8dct/me/weightb/direct).
fn recode_x264_params(pass: u8, stats: &str) -> String {
    let mut p = String::new();
    p.push_str("pass=");
    p.push_str(if pass == 1 { "1" } else { "2" });
    p.push_str(":stats=");
    p.push_str(stats);
    p.push_str(":colorprim=bt709:transfer=bt709:colormatrix=bt709");
    if pass == 1 {
        p.push_str(
            ":bframes=3:ref=1:subme=1:me=dia:partitions=none:trellis=0:\
             8x8dct=0:weightb=0:mixed-refs=0:direct=none:b-adapt=1",
        );
    } else {
        p.push_str(
            ":ref=4:mixed-refs=1:bframes=3:b-adapt=2:weightb=1:direct=auto:\
             subme=7:trellis=2:partitions=p8x8,b8x8,i4x4,i8x8:8x8dct=1:me=umh",
        );
    }
    p
}

/// Build the ffmpeg argument list for the preprocessing stage of the `-rc`
/// two-pass encode: decode the source, apply the filter chain, and emit an
/// 8-bit YUV4MPEG2 stream on stdout (no audio, no `-stats` noise).
///
/// The y4m stream is fed straight into the encoder process via a pipe (see
/// [`run_recode`]). Routing the filtered video through y4m first isolates the
/// encoder from ffmpeg 8.1's filtergraph-chaining regression, where link-level
/// HDR side data (MDCV/CLL) overwrites the per-frame side data that
/// vf_zscale/vf_lut cleared — which would otherwise make libx265 re-emit HDR
/// SEI (and "repeat-headers") on an already BT.709 SDR stream.
fn build_preprocess_tokens(
    input_path: &str,
    vf: Option<&str>,
    fps: Option<FrameRate>,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-hide_banner".to_string(),
        // Only real errors reach the console; progress is shown by the
        // encoder process instead.
        "-loglevel".to_string(),
        "error".to_string(),
    ];
    // The libplacebo path needs a Vulkan device initialised for its uploads.
    if vf.map(|f| f.contains("libplacebo")).unwrap_or(false) {
        args.push("-init_hw_device".to_string());
        args.push("vulkan=vk".to_string());
    }
    for tok in build_decode_prefix_tokens() {
        args.push(tok.to_string());
    }
    args.push("-i".to_string());
    args.push(input_path.to_string());
    if let Some(fps) = fps {
        args.push("-r".to_string());
        args.push(format!("{}/{}", fps.num, fps.den));
    }
    if let Some(f) = vf {
        args.push("-vf".to_string());
        args.push(f.to_string());
    }
    args.push("-an".to_string());
    // Normalise the intermediate to 8-bit YUV420 (the -rc output contract).
    args.push("-pix_fmt".to_string());
    args.push("yuv420p".to_string());
    args.push("-f".to_string());
    args.push("yuv4mpegpipe".to_string());
    args.push("-".to_string());
    args
}

/// Assemble the full ffmpeg argument list for one pass of a 2-pass
/// encode. The video comes from `-f yuv4mpegpipe -i pipe:0` (the preprocess
/// stage's stdout), so no `-vf`/`-r`/`-hwaccel` are needed here. Pass 1
/// targets the null muxer (stdout discard); pass 2 targets `output`. A raw
/// output (no extension) gets `-f hevc`/`-f h264` so ffmpeg knows to write an
/// elementary stream rather than guessing from the name.
fn build_recode_tokens(
    bitrate: u32,
    stats: &str,
    output: &str,
    pass: u8,
    encoder: Encoder,
    level: &str,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-hide_banner".to_string(),
        "-y".to_string(),
        // Force periodic `frame=` progress even when stderr is a pipe (ffmpeg
        // suppresses it by default when it is not a terminal).
        "-stats".to_string(),
    ];
    args.push("-f".to_string());
    args.push("yuv4mpegpipe".to_string());
    args.push("-i".to_string());
    args.push("pipe:0".to_string());
    // Pin output colour to BT.709 SDR. The libx265 wrapper reads the codec
    // context (seeded from the decoded input), so these output-level flags
    // override any residual BT.2020/PQ that would otherwise be misdetected
    // as HDR (the `setparams` in the -vf only rewrites per-frame side data,
    // which the wrapper does not consult).
    args.push("-color_primaries".to_string());
    args.push("bt709".to_string());
    args.push("-color_trc".to_string());
    args.push("bt709".to_string());
    args.push("-colorspace".to_string());
    args.push("bt709".to_string());
    args.push("-c:v".to_string());
    args.push(encoder.lib_name().to_string());
    args.push("-b:v".to_string());
    args.push(format!("{bitrate}k"));
    args.push("-preset".to_string());
    args.push("medium".to_string());
    match encoder {
        Encoder::X265 => {
            args.push("-x265-params".to_string());
            args.push(recode_x265_params(pass, stats, level));
        }
        Encoder::X264 => {
            args.push("-x264-params".to_string());
            args.push(recode_x264_params(pass, stats));
        }
    }
    args.push("-an".to_string());
    if pass == 1 {
        args.push("-f".to_string());
        args.push("null".to_string());
        args.push("-".to_string());
    } else {
        // Strip chapters and copied tags, and clear the per-stream encoder
        // identifier so the stream carries no encoder metadata.
        args.push("-map_chapters".to_string());
        args.push("-1".to_string());
        args.push("-map_metadata".to_string());
        args.push("-1".to_string());
        args.push("-metadata:s:v:0".to_string());
        args.push("ENCODER=".to_string());
        // x264 has no `no-info` equivalent, so strip its embedded
        // "x264 - core ... options: ..." user-data SEI (NAL type 6) to match
        // the x265 --no-info behaviour of a clean, metadata-free stream.
        if encoder.is_x264() {
            args.push("-bsf:v".to_string());
            args.push("filter_units=remove_types=6".to_string());
        }
        if std::path::Path::new(output).extension().is_none() {
            args.push("-f".to_string());
            args.push(if encoder.is_x264() { "h264" } else { "hevc" }.to_string());
        }
        args.push(output.to_string());
    }
    args
}

/// Parameters for one `--recode` run, bundled so [`run_recode`] stays below
/// clippy's argument-count limit.
struct RecodeRequest<'a> {
    input: &'a str,
    vf: Option<&'a str>,
    bitrate: u32,
    outfile: &'a str,
    encoder: Encoder,
    level: &'a str,
    fps: Option<FrameRate>,
    total_frames: u64,
}

/// Run the selected 2-pass encode: first pass writes only the stats file (to
/// the null muxer), second pass writes the actual output file. Exits with
/// ffmpeg's status.
///
/// Each pass runs two ffmpeg processes joined by a pipe:
///   1. a preprocess stage decoding the source, applying the `-vf` filter
///      chain and emitting an 8-bit YUV4MPEG2 stream on stdout (`-loglevel
///      error`, so it stays quiet and only real errors surface);
///   2. an encode stage reading that stream from `pipe:0` and doing the actual
///      libx265/libx264 2-pass encode.
///
/// Splitting the encode from the filtergraph via y4m avoids ffmpeg 8.1's
/// filtergraph-chaining regression, where link-level HDR side data (MDCV/CLL)
/// overwrites the frame-level side data that vf_zscale/vf_lut cleared, making
/// libx265 re-emit HDR SEI on an already BT.709 SDR stream. The encoder's
/// stderr is forwarded through [`drain_filtered_stderr`], so encode progress
/// and the encoder's own output stay visible.
fn run_recode(req: RecodeRequest) -> ! {
    use std::process::{Command, Stdio};

    let RecodeRequest { input: input_path, vf, bitrate, outfile, encoder, level, fps, total_frames } = req;
    let stats = stats_filename(outfile);
    let pre = build_preprocess_tokens(input_path, vf, fps);
    let pass1 = build_recode_tokens(bitrate, &stats, "", 1, encoder, level);
    let pass2 = build_recode_tokens(bitrate, &stats, outfile, 2, encoder, level);

    for (pass, enc_tokens) in [(1u8, &pass1), (2u8, &pass2)] {
        eprintln!(
            "  pass {pass}: ffmpeg {} | ffmpeg {}",
            shell_join(&pre),
            shell_join(enc_tokens)
        );

        // Preprocess: source -> filter chain -> y4m on stdout. Its stderr is
        // inherited so real errors (it runs with `-loglevel error`) still
        // reach the console.
        let mut preproc = match Command::new(resolve_binary("ffmpeg"))
            .args(&pre)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => fail(&format!("failed to launch ffmpeg (preprocess, pass {pass}): {e}")),
        };
        let Some(pre_stdout) = preproc.stdout.take() else {
            let _ = preproc.kill();
            fail("failed to capture ffmpeg preprocess output");
        };

        // Encode: read the y4m stream straight from the preprocess stage's
        // stdout (zero-copy pipe hand-off; no temporary file on disk).
        let mut enc = match Command::new(resolve_binary("ffmpeg"))
            .args(enc_tokens)
            .stdin(Stdio::from(pre_stdout))
            .stdout(Stdio::inherit())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                let _ = preproc.kill();
                fail(&format!("failed to launch ffmpeg (encode, pass {pass}): {e}"));
            }
        };

        if let Some(Err(e)) = enc
            .stderr
            .take()
            .map(|stderr| drain_filtered_stderr(stderr, total_frames))
        {
            let _ = enc.kill();
            let _ = preproc.kill();
            fail(&format!("failed to read ffmpeg output (pass {pass}): {e}"));
        }

        let enc_status = match enc.wait() {
            Ok(s) => s,
            Err(e) => {
                let _ = preproc.kill();
                fail(&format!("failed to wait for ffmpeg (pass {pass}): {e}"));
            }
        };
        if !enc_status.success() {
            // Stop the preprocess stage: it would get a broken pipe once the
            // encoder is gone, but kill it explicitly so we exit promptly.
            let _ = preproc.kill();
            std::process::exit(enc_status.code().unwrap_or(1));
        }
        // The encoder consumed the whole y4m stream, so the preprocess stage
        // must have finished successfully as well.
        match preproc.wait() {
            Ok(s) if s.success() => {}
            _ => fail(&format!("ffmpeg preprocess failed (pass {pass})")),
        }
    }
    eprintln!("recode complete: {outfile}");
    std::process::exit(0);
}

/// Reformat an ffmpeg `-stats` progress line (e.g. `frame= 1063 fps=136
/// q=28.0 size=... time=... bitrate=2959.0kbits/s speed=...`) into the compact
/// x265 CLI-style progress line:
///   `[70.8%] 1063/1502 frames, 136.46 fps, 2959.00 kb/s, eta 0:00:03`
/// Returns `None` when the line is not a `frame=` progress line.
fn reformat_progress(text: &str, total_frames: u64) -> Option<String> {
    let t = text.trim_start();
    if !t.starts_with("frame=") {
        return None;
    }

    let mut frame: u64 = 0;
    let mut fps: f64 = 0.0;
    let mut bitrate: Option<f64> = None;

    // ffmpeg right-aligns some values (`frame=  1063`), so a token like
    // `frame=` has an empty value and the real value is the next token.
    let mut tokens = t.split_whitespace();
    while let Some(tok) = tokens.next() {
        let Some((k, raw)) = tok.split_once('=') else {
            continue;
        };
        let v = if raw.is_empty() {
            tokens.next().unwrap_or("")
        } else {
            raw
        };
        match k {
            "frame" => frame = v.parse().unwrap_or(0),
            "fps" => fps = v.parse().unwrap_or(0.0),
            "bitrate" if v != "N/A" => {
                let num = v
                    .trim_end_matches("kbits/s")
                    .trim_end_matches("kb/s")
                    .trim();
                bitrate = num.parse::<f64>().ok();
            }
            _ => {}
        }
    }

    let pct = if total_frames > 0 {
        frame as f64 * 100.0 / total_frames as f64
    } else {
        0.0
    };
    let eta_secs = if fps > 0.0 && total_frames > frame {
        (total_frames - frame) as f64 / fps
    } else {
        0.0
    };
    let kb = match bitrate {
        Some(b) => format!("{b:.2} kb/s"),
        None => "N/A kb/s".to_string(),
    };

    Some(format!(
        "[{pct:.1}%] {frame}/{total_frames} frames, {fps:.2} fps, {kb}, eta {}",
        format_eta(eta_secs)
    ))
}

/// Format seconds as `h:mm:ss` (e.g. `0:00:03`), matching x265's eta display.
fn format_eta(secs: f64) -> String {
    let total = secs.max(0.0).round() as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    format!("{h}:{m:02}:{s:02}")
}

/// Decide whether a decoded ffmpeg stderr record should reach the console.
///
/// xwaf only processes the video stream, so besides the encoder's own output
/// (x265/x264 banner, warnings and progress) only video-relevant error lines
/// are forwarded. Everything else — container/subtitle/audio probe notices,
/// hints, stream-mapping and metadata dumps — is dropped.
fn should_forward(text: &str) -> bool {
    let t = text.trim_start();

    // Encoder library output: `x265 [info]/[warning]/[error]`, the x264 CLI's
    // `x264 [info]` lines, and ffmpeg's libx264 wrapper `[libx264 @ …]` lines
    // (profile/cpu/tool info, per-frame stats and errors) are all kept.
    if t.starts_with("x265 [") || t.starts_with("x264 [") || t.starts_with("[libx264") {
        return true;
    }
    // x265 2-pass progress ("[ 12.3%] 123/1000 frames …") and its final
    // "encoded N frames …" summary line.
    if t.starts_with('[') && t.contains("%]") {
        return true;
    }
    if t.starts_with("encoded ") && t.contains(" frames") {
        return true;
    }
    // ffmpeg's own `-stats` progress line ("frame= …").
    if t.starts_with("frame=") {
        return true;
    }

    // Error lines. Audio/subtitle-tagged errors are irrelevant to a video
    // re-encode and are hidden like any other non-video output.
    let lower = t.to_ascii_lowercase();
    let is_err = lower.contains("error")
        || lower.contains("failed")
        || lower.contains("unable")
        || lower.contains("no such file")
        || lower.contains("invalid data");
    if is_err {
        let audio_sub = t.starts_with("[ast#")
            || t.starts_with("[aost#")
            || t.starts_with("[sst#")
            || t.starts_with("[sost#")
            || t.starts_with("[af#");
        return !audio_sub;
    }
    false
}

/// Write one decoded stderr record to our stderr, forwarding it only when
/// [`should_forward`] says so. A record ending in `\r` is a progress update
/// that must overwrite the current line, so it is flushed immediately without
/// a newline (and reformatted via [`reformat_progress`]); a record ending in
/// `\n` is a regular log line.
fn write_record(
    record: &[u8],
    out: &mut std::io::Stderr,
    line_end: bool,
    total_frames: u64,
) -> std::io::Result<()> {
    use std::io::Write;

    let text = String::from_utf8_lossy(record);
    if text.is_empty() || !should_forward(&text) {
        return Ok(());
    }

    // Rewrite ffmpeg's `frame=...` stats line into the compact x265-style form
    // (applies to both the in-place `\r` updates and the final `\n`-terminated
    // line ffmpeg prints at the end of the encode).
    let terminator: &[u8] = if line_end { b"\n" } else { b"\r" };
    if let Some(formatted) = reformat_progress(&text, total_frames) {
        out.write_all(formatted.as_bytes())?;
        out.write_all(terminator)?;
        // `std::io::stderr()` is a line-buffered `LineWriter`, so a `\r`
        // record must be flushed explicitly to update in place.
        out.flush()?;
        return Ok(());
    }

    out.write_all(record)?;
    out.write_all(terminator)?;
    out.flush()?;
    Ok(())
}

/// Read a child process's stderr to EOF and forward it to our stderr through
/// [`write_record`], keeping only the encoder's own output (x265/x264 banner),
/// progress, and video-relevant error lines. Used by [`run_recode`] on the
/// encoder side of the pipeline.
///
/// Progress updates are separated by carriage returns (`\r`) so they overwrite
/// one another in-place on a terminal; they are detected here (distinguishing a
/// lone `\r` from a CRLF `\r\n` line ending) and written as in-place `\r`
/// updates rather than one newline per update.
fn drain_filtered_stderr(
    stderr: std::process::ChildStderr,
    total_frames: u64,
) -> std::io::Result<()> {
    use std::io::{BufReader, Read, Write};

    let mut reader = BufReader::new(stderr);
    let mut out = std::io::stderr();
    let mut record: Vec<u8> = Vec::new();
    let mut byte = [0u8; 1];

    loop {
        let n = match reader.read(&mut byte) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        if n == 0 {
            break;
        }
        match byte[0] {
            b'\n' => {
                write_record(&record, &mut out, true, total_frames)?;
                record.clear();
            }
            b'\r' => {
                // A `\r` may be a CRLF line ending (`\r\n`) or an in-place
                // progress separator. Peek the next byte to tell them apart.
                let mut next = [0u8; 1];
                let peeked = reader.read(&mut next).unwrap_or_default();
                if peeked == 0 {
                    // Trailing `\r` at end of stream.
                    write_record(&record, &mut out, true, total_frames)?;
                    record.clear();
                    break;
                } else if next[0] == b'\n' {
                    write_record(&record, &mut out, true, total_frames)?;
                    record.clear();
                } else {
                    write_record(&record, &mut out, false, total_frames)?;
                    record.clear();
                    record.push(next[0]);
                }
            }
            b => record.push(b),
        }
    }

    if !record.is_empty() {
        write_record(&record, &mut out, true, total_frames)?;
    }
    out.flush()?;
    Ok(())
}

/// Detect (cached, once per run) whether the `mpv` player is available on PATH.
fn mpv_available() -> bool {
    static HAS_MPV: OnceLock<bool> = OnceLock::new();
    *HAS_MPV.get_or_init(|| Command::new(resolve_binary("mpv")).arg("--version").output().is_ok())
}

/// Launch ffplay to play the (optionally preprocessed) video stream.
///
/// When a `-vf` filter is provided it is applied on the fly by ffplay, so the
/// user previews exactly what ffmpeg preprocessing would produce. ffplay runs
/// interactively, inheriting the current terminal's stdio; this function only
/// returns if ffplay could not be started.
fn play_with_ffplay(path: &str, vf: Option<&str>) -> ! {
    let mut cmd = Command::new(resolve_binary("ffplay"));
    if let Some(filter) = vf {
        cmd.arg("-vf").arg(filter);
    }
    cmd.arg(path);

    match cmd.status() {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(e) => fail(&format!("failed to launch ffplay: {}", e)),
    }
}

/// Play the video, preferring mpv's zero-copy GPU pipeline when available and
/// falling back to ffplay otherwise.
///
/// mpv performs HDR→SDR tone mapping on the GPU via `--vo=gpu-next`, so only
/// the geometry part (`scale`/`pad`) is handed to it; ffplay instead receives
/// the full `-vf` (geometry + colour) filter chain.
fn play_video(path: &str, full_vf: Option<&str>, geometry_vf: Option<&str>) -> ! {
    if mpv_available() {
        let mut cmd = Command::new(resolve_binary("mpv"));
        cmd.arg("--vo=gpu-next");
        cmd.arg("--tone-mapping=bt.2390");
        if let Some(geo) = geometry_vf {
            cmd.arg(format!("--vf=lavfi=[{geo}]"));
        }
        cmd.arg(path);

        match cmd.status() {
            Ok(status) => std::process::exit(status.code().unwrap_or(1)),
            // mpv failed to start/run — fall back to ffplay.
            Err(_) => play_with_ffplay(path, full_vf),
        }
    } else {
        play_with_ffplay(path, full_vf);
    }
}

/// Consume the value following a value-taking option, failing with a
/// `--option requires …` message when the value is missing.
fn require_arg(args: &[String], i: &mut usize, opt: &str, hint: &str) -> String {
    *i += 1;
    args.get(*i)
        .cloned()
        .unwrap_or_else(|| fail(&format!("{opt} requires {hint}")))
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut rescale: Option<RescaleTarget> = None;
    let mut letterbox = false;
    let mut pillarbox = false;
    let mut setfps: Option<FrameRate> = None;
    let mut play = false;
    let mut outpipe = false;
    let mut recode: Option<u32> = None;
    let mut outfile: Option<String> = None;
    let mut encoder = Encoder::X265;
    let mut positional = Vec::new();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-rs" | "--rescale" => {
                let val = require_arg(
                    &args,
                    &mut i,
                    "--rescale",
                    "a value (480p/720p/1080p/1440p/2160p/2880p/4320p)",
                );
                match RescaleTarget::parse(&val) {
                    Some(target) => rescale = Some(target),
                    None => fail(&format!(
                        "invalid --rescale value '{}' \
                         (expected 480p/720p/1080p/1440p/2160p/2880p/4320p)",
                        val
                    )),
                }
            }
            "-lb" | "--letterbox" => letterbox = true,
            "-pb" | "--pillarbox" => pillarbox = true,
            "-sf" | "--setfps" => {
                let val = require_arg(
                    &args,
                    &mut i,
                    "--setfps",
                    "a value (23.976/24/25/29.97/30/59.97/60)",
                );
                match parse_setfps(&val) {
                    Some(fps) => setfps = Some(fps),
                    None => fail(&format!(
                        "invalid --setfps value '{}' (expected 23.976/24/25/29.97/30/59.97/60)",
                        val
                    )),
                }
            }
            "-pp" | "--playpreview" => play = true,
            "-op" | "--outpipe" => outpipe = true,
            "-rc" | "--recode" => {
                let val =
                    require_arg(&args, &mut i, "--recode", "a bitrate value in kbps (e.g. 3000)");
                match val.parse::<u32>() {
                    Ok(n) if n > 0 => recode = Some(n),
                    _ => fail(&format!(
                        "invalid --recode bitrate '{}' (expected a positive integer kbps)",
                        val
                    )),
                }
            }
            "-of" | "--outfile" => {
                let val = require_arg(
                    &args,
                    &mut i,
                    "--outfile",
                    "an output filename (e.g. output.mkv)",
                );
                outfile = Some(val);
            }
            "-ec" | "--encoder" => {
                let val = require_arg(&args, &mut i, "--encoder", "a value (x265 or x264)");
                match Encoder::parse(&val) {
                    Some(enc) => encoder = enc,
                    None => fail(&format!(
                        "invalid --encoder value '{}' (expected x265 or x264)",
                        val
                    )),
                }
            }
            "-h" | "--help" => {
                print_usage();
                return;
            }
            other if other.starts_with('-') => fail(&format!("unknown option '{}'", other)),
            other => positional.push(other.to_string()),
        }
        i += 1;
    }

    if positional.len() != 1 {
        print_usage();
        std::process::exit(1);
    }

    if recode.is_some() != outfile.is_some() {
        fail("--recode and --outfile must be used together");
    }
    if outpipe && recode.is_some() {
        fail("--outpipe and --recode are mutually exclusive");
    }

    if rescale.is_some() || letterbox || pillarbox || setfps.is_some() || play || outpipe || recode.is_some() {
        // --letterbox/--pillarbox need a target canvas; -pp alone just plays
        // the source as-is, -op alone just pipes the source as-is.
        if rescale.is_none() && (letterbox || pillarbox) {
            fail("--letterbox/--pillarbox require --rescale to define the target canvas");
        }

        // The source's display aspect ratio (DAR) decides whether letterbox or
        // pillarbox padding applies, and drives the destination size.
        let info = match get_video_info(&positional[0]) {
            Ok(info) => info,
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        };

        // Determine the effective output frame rate (`retime`):
        //   - `--setfps` when given, validated against the source (±5%);
        //   - otherwise the source rate snapped to the closest standard CFR
        //     value, but only when that differs from the source (so a source
        //     already at 24000/1001 etc. is left untouched).
        // A non-None `retime` re-stamps timestamps via `setpts=N…` without
        // adding or dropping frames, producing clean CFR output.
        let mut retime = setfps;
        match retime {
            Some(dst) => {
                if !fps_close(info.frame_rate, dst) {
                    eprintln!(
                        "warning: --setfps {} ignored: source framerate {} differs too much (must be within ±5%)",
                        dst, info.frame_rate
                    );
                    retime = None;
                }
            }
            None => {
                // Retime when the normalised rate differs from the source, or
                // whenever the source is VFR — so a VFR source whose average
                // rate already equals a standard value is still flattened to
                // clean CFR via `setpts=N…` + `-r`.
                retime = normalize_fps(info.frame_rate)
                    .filter(|&r| r != info.frame_rate || !info.cfr);
            }
        }

        // Build the -vf filter (None when the source already matches). The
        // ffmpeg command / pipe prefers libplacebo when available; the ffplay
        // preview uses a CPU-only chain because ffplay cannot initialise a
        // Vulkan device (it has no `-init_hw_device` option).
        let vf = build_vf_filter(rescale, letterbox, pillarbox, retime, &info, true);
        let cpu_vf = build_vf_filter(rescale, letterbox, pillarbox, retime, &info, false);
        let geometry_vf = rescale.and_then(|target| build_geometry_vf(target, letterbox, pillarbox, &info));

        if play {
            play_video(&positional[0], cpu_vf.as_deref(), geometry_vf.as_deref());
        }

        if outpipe {
            // When no preprocessing is required we still emit a valid yuv4mpeg
            // stream for downstream consumers, with colour tags pinned and no
            // -vf filter attached.
            run_ffmpeg_pipe(&positional[0], vf.as_deref(), retime);
        }

        if let (Some(bitrate), Some(out)) = (recode, outfile.as_deref()) {
            // The HEVC level tracks the output resolution: the target canvas
            // when rescaling, otherwise the source dimensions.
            let (out_w, out_h) = match rescale {
                Some(target) => {
                    let (w, h) = target.size();
                    (w as u64, h as u64)
                }
                None => (info.width as u64, info.height as u64),
            };
            let level = hevc_level_for_size(out_w, out_h);
            // Actual output frame size after scale/pad (not the canvas): a
            // 2.39:1 film scaled to a 720p width with no -lb is 1280x~532.
            let (info_w, info_h) = match rescale {
                Some(target) => compute_geometry(target, letterbox, pillarbox, &info).2,
                None => (info.width, info.height),
            };
            // Preprocessed stream summary, in the style of x265's input line
            // (e.g. `avs [info]: 1920x1080p 1:1 @ 24000/1001 fps (cfr)`).
            // Reports the effective output rate: the --setfps target (always
            // CFR since it re-stamps uniformly) or the source rate otherwise.
            let out_fps = retime.unwrap_or(info.frame_rate);
            let out_cfr = retime.is_some() || info.cfr;
            eprintln!(
                "{} [info]: {}x{}p 1:1 @ {}/{} fps ({})",
                env!("CARGO_PKG_NAME"),
                info_w,
                info_h,
                out_fps.num,
                out_fps.den,
                if out_cfr { "cfr" } else { "vfr" }
            );
            run_recode(RecodeRequest {
                input: &positional[0],
                vf: vf.as_deref(),
                bitrate,
                outfile: out,
                encoder,
                level,
                fps: retime,
                total_frames: info.estimated_total_frames(),
            });
        }

        match vf {
            Some(vf) => {
                println!("-vf \"{}\"", vf);
                if let Some(cmd) = build_ffmpeg_command(&positional[0], &vf, &info, retime) {
                    println!();
                    println!("ffmpeg decode + preprocess command (pipe stdout to encoder):");
                    println!("  {cmd}");
                }
            }
            None => println!("No preprocessing needed: source already matches the target."),
        }
        return;
    }

    let path = &positional[0];
    match get_video_info(path) {
        Ok(info) => println!("{}", info),
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}