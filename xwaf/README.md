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
duration, bitrate).

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
| `-h, --help` | Show help. |

Constraints: `--letterbox`/`--pillarbox` require `--rescale`; `--recode` and
`--outfile` must be used together; `--outpipe` and `--recode` are mutually
exclusive.

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
