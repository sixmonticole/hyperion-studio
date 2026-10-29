# Plan: per-region selection, then local alignment points

Two ambitious features, in order. Each one is measured on a real session
before the next begins. The guiding rule: **extend the existing pipeline,
never rewrite it**, so the current default output stays available as a
reference.

## C - Per-region selection (local quality)

### Why

Today a frame is scored **globally** (`quality::sharpness_score` over the whole
crop), and the best `keep_fraction` are kept as wholes. But a single frame can
be sharp at the terminator and soft at the limb. Keeping or discarding the
whole frame throws away good regions.

AutoStakkert scores *tiles* and, for each tile, keeps the frames that are
actually sharp *there*. Every part of the final image is then drawn from the
frames that were best for it.

### Shape of the change

1. **Tile grid.** Partition the crop into tiles (default 64x64 px, aligned on
   the crop). `tiles_x = crop_w / tile`, `tiles_y = crop_h / tile`.

2. **Per-tile score.** During analysis, for each frame compute a sharpness
   score **per tile**, not just globally. Store it in `FrameInfo` as a flat
   `Vec<f32>` of `tiles_x * tiles_y`.
   - Memory: 200 frames x (56x56 tiles) x 4 bytes = ~2.5 MB. Negligible.
   - Cost: the Laplacian + Tenengrad loops already visit every pixel; we just
     accumulate per tile instead of once. Near-free.

3. **Per-tile frame ranking.** After analysis, for each tile, rank frames by
   that tile's score and keep the best `keep_fraction` **for that tile**.
   A frame may therefore be kept for the terminator and dropped for the limb.

4. **Per-tile accumulation.** The accumulator gains a mode where, for each
   tile, only the frames kept for that tile contribute. Implementation: keep
   the existing per-pixel weighted accumulator, but the weight a frame
   contributes to a tile is zero when that frame is not kept for that tile.

5. **Global mode unchanged.** `Settings::tile_select` (new, default on) turns
   this off: then every kept frame contributes everywhere, as today.

### Why it is low risk

- The accumulator is already per-pixel weighted; a per-tile gate is a
  multiplication of the existing weight.
- The alignment path is untouched: frames are aligned exactly as now, then
  accumulated with a per-tile gate.
- If a tile has too few kept frames, we fall back to the global kept set for
  that tile, so no hole can appear.

### Measurable outcome

On the 200-frame session: compare global vs per-region, same settings. Expect
better detail in transition zones (terminator, limb) at equal noise. Judged
visually on a 1:1 crop, plus the sharpness score of the result.

## B - Local alignment points (AP)

### Why

Alignment is **global**: one translation per frame. Turbulence distorts
*locally*, so a single translation cannot fix it - it averages sharp and soft
together. `ap.rs` exists but degrades the image, most likely because it
resamples twice and has no regularised displacement field.

### Shape of the change

1. **Choose APs on the reference**: high local contrast, not on noise, spread
   over the object. Reject APs whose local variance is below a threshold.

2. **Estimate a shift per AP** (phase correlation on a small window), then
   build a **dense displacement field** by interpolating between APs.

3. **One warp per frame**: resample once using the field (Catmull-Rom already
   available). Never align then re-align.

4. **Regularise**: reject outlier AP shifts, smooth the field so it cannot
   tear. A field that jumps between neighbouring APs is worse than no AP.

5. Behind `Settings::ap_enabled` (default off) until it demonstrably beats the
   global path on the real session.

### Order

C first, because it reuses everything and is measurable immediately. B is a
real algorithm project and will reuse C's tile grid to place APs sensibly.

### Outcome of C

Done and merged. `quality::score_frame` scores per tile, frames are ranked per
tile, and the accumulator gates per tile. New setting `tile_select` (default on)
and `tile_size` (default 64 px). Tests: tile scores locate sharpness, and the
gate keeps a non-admitted tile empty.

### Outcome of B (first pass)

The module was rewritten and three real bugs fixed: a missing Hann window on the
frame patch, a half-patch offset between the reference and frame patches, and a
reliability test that compared a normalised peak to raw patch energy (rejecting
every point). After that the shift estimates are excellent: the median matches
the global shift to a tenth of a pixel.

But the field **warps worse** than a single global translation on real frames
(+6 to +9% MSE against the reference, and 1-3% softer). Measured on frame 0
against frames 1, 5 and 30 of the session.

Diagnosis: the per-patch estimate is dominated by jitter, so the field encodes
noise rather than coherent deformation. Two disproofs ruled out other causes: an
identical constant shift through both warps produces identical pixels (so the
resampling is fine), and the border handling was aligned to match.

So B stays off by default. Next concrete steps, in order:

1. **Larger patches** (128 px instead of 64) to raise the estimate's
   signal-to-noise ratio, then re-measure the same three pairs.
2. **Regularised fit**: fit a low-order model (translation + rotation + scale,
   or a smooth spline) to the anchor shifts, so coherent motion is kept and
   jitter is discarded. Only what the model cannot explain should vary locally.
3. Re-measure. AP ships only if it beats the global path on MSE *and*
   sharpness, on the real session.

