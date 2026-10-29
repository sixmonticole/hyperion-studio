# Hyperion Studio

A lunar and planetary astrophotography processing pipeline in one application:
it replaces the **PIPP + AutoStakkert** workflow.

It takes a folder of RAW files (Sony ARW and others, or already-converted
16-bit TIFFs) and produces a **16-bit TIFF stack**, doing all the work
automatically:

```
RAW -> demosaic (PPG) -> TIFF cache -> object detection -> crop
    -> sharpness score -> selection -> blockwise stacking
    -> post-processing -> final TIFF
```

## Status: V1 validated on a real 200-frame session

Test session: Sony A6000 (ILCE-6000), 6048x4024, 200 lunar frames in RAW
(DSC04743 to DSC04942), fixed tripod.

The result is **sharp**: crisp craters (Tycho and its rays, Copernicus,
Plato, Kepler, Aristarchus), maria with relief, clean limb, strongly reduced
noise.

### The key discovery: why a large stack was blurry

For a long time, stacking 200 frames produced a **blurrier** image than
stacking 20. Successive tests isolated the cause:

| Test | Result |
|---|---|
| 1 raw frame | soft (single-exposure noise) |
| **20 frames from the start of the session** | **sharp** |
| **10 frames from the middle** | **sharp** |
| 200 frames | **blurry** |

On this session the Moon **drifts by several hundred pixels** (up to ~630 px
measured) and, over ~30 minutes, it **rotates and changes lighting**.
Averaging frames far apart in time means averaging images that no longer
overlap: detail is lost.

So it was **neither turbulence, nor demosaicing, nor post-processing**:

- stacking 20 *identical* copies of a frame -> perfectly sharp (accumulation
  adds no blur);
- stacking with **no alignment and no resampling** -> still blurry (sub-pixel
  resampling is not the culprit);
- stacking a **block of consecutive frames** -> sharp.

### The adopted solution: blockwise stacking

```
session -> blocks of N consecutive frames -> stack each block
        -> mutually align the blocks -> average the blocks
```

Inside a block the rotation is negligible, so each block is sharp. The blocks
are few, so their mutual alignment stays robust despite the global drift.

This is the **default behaviour** (`blockSize = 10`, adjustable in the UI or
disabled with 0).

## Validated chain

| Step | Status |
|---|---|
| RAW import + 16-bit TIFF cache (resumable) | done |
| RAW demosaic to 16-bit RGB (rawler PPG) | done |
| Neutral white balance (gray-world) | done |
| Automatic object detection (high/low thresholds) | done |
| Crop containing the whole object, drift included | done |
| Sharpness score (Laplacian + Tenengrad) + selection | done |
| Alignment by phase correlation (sub-pixel, bicubic) | done |
| **Blockwise stacking** | done |
| Masked stacking (per-pixel weight, edges preserved) | done |
| Post-processing: levels, wavelets, CLAHE, clarity | done |
| Rotation / mirror (manual orientation) | done |
| 16-bit TIFF export + JSON report | done |
| Tauri UI (folder, progress, settings) | done |

### Post-processing: order and highlight protection

Two mistakes were fixed, both of which made the image **softer than the raw
stack**:

1. **Pipeline order.** Wavelets were applied **before** levels. The raw stack
   comes out very dark (~0.1 in [0,1]), so the detail coefficients were tiny
   and the sharpening had almost no visible effect. Correct order: **levels
   first, wavelets after**.
2. **Crushed highlights.** A linear stretch followed by a `clamp` flattened
   bright areas into white (Tycho's rays). A *soft shoulder* compression curve
   above 0.8 preserves detail near white.

## Architecture

```
src/                              React UI (Tauri)
  App.tsx                         folder, progress, settings; orchestrates the screens
  Running.tsx                     in-progress screen (steps, ETA, cancel)
  Result.tsx                      result screen (tabs, stats, 1:1 inspection)
  SettingsDrawer.tsx              advanced settings + cache management (drawer)
  i18n.ts                         bilingual UI text (FR/EN) + code-to-text mapping
  types.ts                        types shared with the Rust backend
  App.css                         design system, dark and light themes
  useTheme.ts                     theme persistence
src-tauri/src/
  lib.rs                          Tauri commands
  model.rs                        Settings, EnhanceSettings, Orientation, StackReport, Progress
  pipeline/
    mod.rs                        orchestrator: stack_blockwise / stack_global
    decode.rs                     RAW -> 16-bit RGB (rawler, PPG + gray-world)
    cache.rs                      TIFF cache, manifest, size/space helpers
    detect.rs                     object detection + crop window
    quality.rs                    sharpness score (Laplacian + Tenengrad)
    align.rs                      alignment (phase correlation) + orientation
    similarity.rs                 similarity alignment (derotation) - experimental
    ap.rs                         local alignment points - experimental
    stack.rs                      accumulator (per-pixel weight)
    enhance.rs                    levels, wavelets, CLAHE, clarity
    output.rs                     TIFF + JSON report writing
    preview.rs                    PNG previews (full and 1:1) for the UI
    tests.rs                      unit tests
src-tauri/src/bin/                development tools (--features devtools)
design/                           UI prototypes and icon generation script
```

### Design notes

- **Cancel a run.** `cancel_stack` flips an `AtomicBool` the pipeline polls at
  each loop checkpoint; a cancelled run reports a distinct `Cancelled` error
  instead of a failure.
- **Robust import.** An unreadable frame is skipped and reported; it never
  aborts the session. Cache TIFFs are written atomically (tmp file + rename),
  and a cache entry whose source RAW changed (same name, different size) is
  rebuilt.
- **No prose over the wire.** The backend sends stable stage/error/skip
  *codes*; `i18n.ts` maps them to French or English. Switching language needs
  no recompile and the two languages cannot drift apart.
- **Cache management.** The UI can report the cache size, purge orphaned
  entries, clear the cache, and warn early when disk space is too low.

## Build and run

```bash
bun install
bun run tauri dev      # development
bun run tauri build    # macOS application
cd src-tauri && cargo test
```

## Settings

Everything is automatic by default. The "All settings" drawer exposes:

- **Frames kept**: percentage of the best (default 25%).
- **Blockwise stacking**: block size, 0 = disabled (default 10).
- **Crop size**: auto (detection) or a pixel value.
- **Align frames**: phase correlation (default on).
- **Normalise brightness** / **Weight by quality** (default on).
- **Local alignment points**: experimental, degrades the image (default off).
- **Orientation**: rotate 90/180, flip H/V (default none).
- **Post-processing**: wavelets, CLAHE, clarity, gamma, black/white points.
- **Disk cache**: size, orphan cleanup, clear.

## Output

In the source folder's `hyperion_out/`:

- `*_stack.tif`: the final stack, uncompressed 16-bit RGB.
- `*_stack.json`: the report (frames kept, mean quality, crop, settings).
- `*_stack_raw.tif` + `*_stack_raw.json`: the raw stack and its report,
  written only when post-processing is enabled.

## Performance and disk usage

- 200 ARW: import + detection + blockwise stacking is about **7-15 min**.
- Warm cache: about **2 min** for 200 frames (analysis + stacking +
  post-processing).
- A demosaiced A6000 RAW is **~145 MB**; a 200-frame cache is ~29 GB, a
  428-frame cache ~62 GB. Safe to delete (rebuilt on demand).

## Development tools

Built with `--features devtools`:

```bash
cd src-tauri
cargo run --features devtools --release --bin runtest         -- <folder> [cache] [fraction] [orientation] [enhance] [wavelet] [clahe] [clarity] [gamma] [noap] [noalign]
cargo run --features devtools --release --bin runtest_blocks  -- <folder> <cache> [block] [keep_per_block]
cargo run --features devtools --release --bin pipeline_steps  -- <folder> <out_dir> [n] [fraction]
cargo run --features devtools --release --bin preview         -- <in.tif> <out.png>
cargo run --features devtools --release --bin crop_preview    -- <in.tif> <out.png> <x> <y> <size> [scale]
cargo run --features devtools --release --bin compare_sharp   -- <A.tif> <B.tif>
cargo run --features devtools --release --bin profile_session -- <cache_folder> [n]
cargo run --features devtools --release --bin diag_chain      -- <cache_folder> [n]
```

`pipeline_steps` writes one image per pipeline stage (raw frame, simple mean,
aligned frame, raw stack, levels, wavelets, final): it is the tool to **visually
validate** where detail is lost or gained.

## Roadmap

1. **Working local alignment (AP)**: the real answer to local turbulence
   distortion. High-contrast points, disc mask, regularised displacement field,
   a single resampling.
2. **Reliable derotation**: would allow stacking the whole session at once
   instead of in blocks.
3. **Drizzle** (oversampling) to reconstruct detail between pixels.
4. **Video stream reading** (SER/MOV/AVI): the planetary standard, where
   thousands of very short exposures freeze turbulence.

## Licensing note

RAW decoding relies on [`rawler`](https://crates.io/crates/rawler), under
**LGPL-2.1**. Harmless for personal use; worth keeping in mind for binary
distribution.
