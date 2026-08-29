# xwaf

**A tool to eXecute Audio/Video transcoding With FFmpeg**

[Download (Windows exe)](target/release/xwaf.exe?raw=true)

---

## Introduction

`xwaf` is a small command-line utility that wraps `ffmpeg` for common video
preprocessing and re-encoding tasks. It inspects a source file, builds the
correct `-vf` filter chain (scaling, black bars, HDR→SDR tone mapping, frame
rate normalisation) and either prints the ready-to-paste ffmpeg command, pipes
the processed stream straight into an encoder, or runs a 2-pass re-encode
itself.

Highlights:

- **Auto HDR→SDR** — prefers the `libplacebo` GPU pipeline (BT.2390 tone
  mapping, the same algorithm mpv uses), with automatic fallback to a CPU
  `zscale` + `tonemap=mobius` chain.
- **Auto hardware decoding** — `-hwaccel auto` (NVIDIA CUDA / AMD AMF / Intel
  QSV / DXVA2 / D3D11VA) with automatic fallback to software decode.
- **Consistent output** — everything is normalised to BT.709 SDR, 8-bit
  4:2:0 (`yuv420p`), square pixels (`SAR 1:1`).
- **Clean 2-pass re-encode** — routes the filtered video through an in-memory
  YUV4MPEG2 pipe between two ffmpeg processes, which sidesteps the ffmpeg 8.1
  filtergraph-chaining regression that otherwise re-injects HDR metadata
  (MDCV/CLL) into SDR output.
- **VFR/CFR handling** — automatic VFR→CFR normalisation when the source
  matches a standard rate, or explicit retiming via `--setfps` (timestamps only,
  no frame add/drop).

## Requirements

- **ffmpeg** (with `ffprobe`/`ffplay`) on `PATH`, or placed next to the `xwaf`
  executable (bundled copies are preferred).
- **mpv** (optional) — used for GPU-accelerated preview; falls back to `ffplay`.
- For `-op` piping into an encoder, a separate encoder CLI such as
  `x265 --y4m -` is expected as the downstream consumer.

## Build

```sh
cargo build --release
```

The executable is produced at `target/release/xwaf.exe`.

## Usage

```text
xwaf [options] <video file path>
```

With no preprocessing options, `xwaf` prints detailed information about the
video stream (codec, resolution, pixel format, bit depth, HDR/SDR, frame rate,
duration, bitrate) followed by every audio track. Pure-audio files (e.g. `.mp3`,
`.flac`, `.m4a`) are also accepted: they show their audio tracks, and
`--audiofile` can transcode them directly.

### Options

| Option | Description |
| --- | --- |
| `-rs, --rescale <preset>` | Rescale to a target 16:9 canvas. Presets: `480p` `720p` `1080p` `1440p` `2160p` `2880p` `4320p`. Scaling is bidirectional (Lanczos). |
| `-lb, --letterbox` | Pad with black bars (top/bottom) to the preset canvas. |
| `-pb, --pillarbox` | Pad with black bars (left/right) to the preset canvas. |
| `-sf, --setfps <fps>` | Retime (no frame drop/dup, duration changes) to a standard rate: `23.976`/`24`/`25`/`29.97`/`30`/`59.97`/`60`. Applied only when the source fps is within ±5% of the target, otherwise ignored with a warning. |
| `-pp, --playpreview` | Play the (preprocessed) stream — prefers mpv (`--vo=gpu-next --tone-mapping=bt.2390`), falls back to ffplay. |
| `-op, --outpipe` | Execute the preprocessing and write the YUV4MPEG2 stream to stdout for direct piping into an encoder. |
| `-rc, --recode <kbps>` | 2-pass re-encode; requires `--outfile`. |
| `-ec, --encoder <name>` | Encoder for `--recode`: `x265` (default) or `x264`. |
| `-of, --outfile <path>` | Output file for `--recode` (`.mkv`/`.mp4`/`.hevc`, or no dot = raw elementary stream). |
| `-avs, --avscript <path>` | Generate an AviSynth script (`DirectShowSource`, built-in) that reproduces the `-rs`/`-lb`/`-pb`/`-sf` preprocessing and HDR→SDR — exact-rational `AssumeFPS` retime (incl. automatic VFR→CFR), `LanczosResize` scale, `AddBorders` bars, `z_ConvertFormat`+`DGHable` tone mapping for HDR sources (avsresize/DGTonemap), and `ConvertToYV12` (YUV 4:2:0). Written silently to the file; may be combined with `--recode`. |
| `-af, --audiofile <path>` | Export/re-encode audio: `.ac3`/`.dts`/`.flac`/`.mp3`/`.aac`/`.m4a`/`.wav`/`.ogg`/`.opus`. Channels above 5.1 are downmixed to 5.1; when the source format and channels already match the target and no `--audiobitrate` is given, the stream is copied without re-encoding (specifying `--audiobitrate` forces a re-encode so the bitrate takes effect). |
| `-at, --audiotrack <n>` | Audio track to export with `--audiofile` (1-based, default 1). |
| `-ac, --audiochannel <n>` | Force the output channel count for `--audiofile` (1-8, e.g. `2`, `6`). |
| `-ab, --audiobitrate <k>` | Audio bitrate in kbps for `--audiofile` (default when omitted: ac3 by channel count — 192/384/448/640 for 1/2/3-5/6ch; dts 1536; mp3/aac/m4a/ogg/opus scale with the channel count — per-channel base × channels, 96/64/64/64/48 kbps/ch respectively, clamped to 64-512; flac/wav are lossless and need none). |
| `-an, --audionormalize` | Normalise peaks to -1 dB for `--audiofile` (measures the source with `volumedetect`, then applies `volume`). Forces a re-encode even when the stream would otherwise be copied. |
| `-al, --audioloudnorm` | Normalise loudness for `--audiofile` (EBU R128 `loudnorm`, target -16 LUFS). Forces a re-encode even when the stream would otherwise be copied. Mutually exclusive with `--audionormalize`. |
| `-as, --audiosample <khz>` | Output sample rate in kHz for `--audiofile` (e.g. `44.1`/`48`/`96`/`192`). Ignored when it equals the source rate; otherwise resamples (forces a re-encode). Default keeps the source rate. |
| `-ap, --audiotempo <x>` | Pitch-preserving speed change for `--audiofile` (0.5-100.0, same as ffmpeg `atempo`; e.g. `0.75` slower, `1.5` faster, `3/2` fraction accepted; `1.0` = unchanged). Runs as the last audio filter; forces a re-encode. |
| `-h, --help` | Show help. |

Constraints: `--letterbox`/`--pillarbox` require `--rescale`; `--recode` and
`--outfile` must be used together; `--outpipe` and `--recode` are mutually
exclusive; `--audiofile` cannot be combined with `--outpipe`/`--recode`/
`--playpreview`, and `--audiotrack`/`--audiochannel`/`--audiobitrate`/
`--audiosample`/`--audiotempo`/`--audionormalize`/`--audioloudnorm` require `--audiofile`;
`--audionormalize` and `--audioloudnorm` are mutually exclusive; `--avscript`
cannot be combined with `--outpipe`/`--playpreview`/`--audiofile`, but may be
combined with `--recode` (the script is written silently before the encode).

### Examples

Show video information:

```sh
xwaf video.mkv
```

Print the `-vf` filter string and the ready-to-paste ffmpeg command for
scaling a 2.39:1 film to 1080p with letterbox bars:

```sh
xwaf -rs 1080p -lb video.mkv
```

Pipe the preprocessed stream directly into x265:

```sh
xwaf -rs 1080p -op video.mkv | x265 --y4m -o out.hevc -
```

2-pass re-encode with x265 (3 Mbps, HDR→SDR conversion is automatic):

```sh
xwaf -rc 3000 -of out.mkv video.mkv
```

2-pass re-encode with x264 and retiming to 29.97 fps:

```sh
xwaf -ec x264 -sf 29.97 -rc 1500 -of out.mp4 video.mkv
```

Export the first audio track as 5.1 AC-3 (7.1 sources are downmixed; default
448 kbps):

```sh
xwaf -af out.ac3 video.mkv
```

Export as stereo AAC at 128 kbps, forcing the channel count:

```sh
xwaf -af out.aac -ac 2 -ab 128 video.mkv
```

Generate an AviSynth script that reproduces the 720p letterbox + 24 fps
preprocessing (open it in AviSynth/VirtualDub etc.):

```sh
xwaf -avs out.avs -rs 720p -lb -sf 24 video.mkv
```

## How it works

- **Frame rate** — VFR sources whose average rate matches a standard CFR value
  within ±0.1% are flattened to clean CFR via `setpts=N` + `-r`. `--setfps`
  re-stamps timestamps (`setpts=N*den/TB/num`) without adding or removing
  frames, so the frame count is preserved and only the duration changes.
- **HDR→SDR** — the filter chain linearises the signal before tone mapping
  (BT.2390 on the GPU pipeline, mobius on the CPU fallback), then re-encodes
  to BT.709. Output colour tags are pinned to BT.709 so encoders never
  misdetect the stream as HDR.
- **2-pass re-encode** — each pass runs two ffmpeg processes joined by a pipe:
  a quiet preprocess stage (decode + filter chain → y4m on stdout) and an
  encode stage reading `pipe:0`. The y4m isolation prevents ffmpeg 8.1's
  filtergraph-chaining regression, where link-level HDR side data overwrites
  the per-frame side data that `zscale`/`lut` cleared, from making libx265
  re-emit HDR SEI (and "repeat-headers") on SDR output. No temporary file is
  written, and wall-clock time is unchanged since encoding is the bottleneck.
- **HEVC level** — the x265 stream level scales with the output resolution
  (2.1 up to 6.2).

## Notes

- On Windows, PowerShell 5.1 rewrites binary output when piping, which corrupts
  the YUV4MPEG2 stream produced by `-op`. Use `cmd` or PowerShell 7.3+ when
  piping into x265.
- External tools (`ffmpeg`, `ffprobe`, `ffplay`, `mpv`) are resolved by first
  checking the directory of the `xwaf` executable, then the system `PATH`.

## License

This project is licensed under [MIT License](../LICENSE). All code in this
repository is free to use, modify, and distribute under the terms of this
license.

## Contact

**E-mail**: [Send Email](mailto:newxhbl@hotmail.com?subject=[RustApps]%20Inquiry)
**Issues**: [Open Issue](../../../issues)
