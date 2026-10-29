//! Local alignment points (AP, "alignment points").
//!
//! Global alignment (a single translation per frame) cannot correct the
//! distortion introduced by the atmosphere: turbulence warps the image
//! *locally*. AutoStakkert handles this by laying a grid of small alignment
//! points, estimating a shift **per point**, then warping the frame to bring
//! each region back onto the reference.
//!
//! This implementation is a rewrite. The previous one degraded the image for
//! three concrete reasons, all fixed here:
//!
//! 1. **Double resampling.** The caller aligned globally (Catmull-Rom), then
//!    AP warped again: two interpolations, each softening the frame. Now the
//!    AP path performs a **single** warp, reading the *unealigned* frame and
//!    folding the global shift into the local field.
//! 2. **Torn field.** Empty field cells were filled by nearest-neighbour
//!    propagation, leaving hard steps between neighbours. The field is now
//!    **smoothed** after filling, so it cannot tear.
//! 3. **No outlier rejection.** One bad point corrupted its whole cell. Shifts
//!    are now compared to the median and rejected when they deviate too far.
//!
//! Chain: choose APs on the reference by local contrast, estimate a shift per
//! AP by phase correlation (sub-pixel), reject outliers, build a dense field by
//! smoothed interpolation, then warp **once**.
//!
//! ## Status: correct but not yet a win on real frames
//!
//! Measured on the 200-frame session (frame 0 against frames 1, 5 and 30), two
//! independent measures agree:
//!
//! | Pair | Δ shift vs global | MSE vs reference | Sharpness vs global |
//! |---|---|---|---|
//! | 0 vs 1  | 0.12 px | +6.9% (worse) | −1.7% |
//! | 0 vs 5  | 0.19 px | +6.4% (worse) | −3.1% |
//! | 0 vs 30 | 0.11 px | +9.3% (worse) | −1.0% |
//!
//! The shift estimates are excellent (the median matches the global shift to a
//! tenth of a pixel, so this is not a bug), but the **field warps slightly
//! worse** than a single global translation. The likely cause is that on these
//! frames the per-patch estimate is dominated by estimation noise: the anchor
//! spread is mostly jitter, so the field encodes noise and displaces each region
//! a fraction of a pixel the wrong way. That is enough to lose detail.
//!
//! So this path stays **off by default** (`Settings::ap_enabled`). It is kept
//! because the estimation half is proven and reusable once the field is fed
//! something more reliable than noisy 64 px patches (larger patches, or a
//! regularised fit that separates coherent displacement from jitter).

use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlanner};
use std::sync::Arc;

/// Patch side, in pixels. Small enough to follow turbulence, large enough for a
/// reliable phase-correlation peak.
const PATCH: usize = 64;

/// Upper bound on the number of points, to keep the cost bounded.
const MAX_ANCHORS: usize = 400;

/// One alignment point: position in the crop plus its estimated shift.
#[derive(Debug, Clone, Copy)]
struct Anchor {
    x: usize,
    y: usize,
    dx: f64,
    dy: f64,
}

/// Local aligner: keeps the reference patches (and their spectra) in cache.
pub struct ApAligner {
    width: usize,
    height: usize,
    patch: usize,
    anchors: Vec<AnchorRef>,
    fft: Arc<dyn Fft<f32>>,
    ifft: Arc<dyn Fft<f32>>,
}

struct AnchorRef {
    x: usize,
    y: usize,
    /// Spectrum of the reference patch.
    spectrum: Vec<Complex32>,
}

impl ApAligner {
    /// Builds the aligner from the reference luminance.
    ///
    /// `step` is the desired spacing between points; it is widened as needed to
    /// stay under [`MAX_ANCHORS`].
    pub fn new(reference_lum: &[f32], width: usize, height: usize, step: usize) -> Self {
        let patch = PATCH.min(width).min(height).max(8);
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(patch * patch);
        let ifft = planner.plan_fft_inverse(patch * patch);

        // Anchor grid. The step is at least `step`, but also widened to bound
        // the total point count.
        let half = patch / 2;
        let base_step = step.max(patch / 2).max(8);
        let mut step = base_step;
        loop {
            let nx = width.saturating_sub(patch) / step + 1;
            let ny = height.saturating_sub(patch) / step + 1;
            if nx * ny <= MAX_ANCHORS || step >= width.max(height) {
                break;
            }
            step = (step as f64 * 1.25) as usize + 1;
        }

        let mut anchors = Vec::new();
        let mut y = half;
        while y + half < height {
            let mut x = half;
            while x + half < width {
                let (spec, energy) =
                    patch_spectrum(reference_lum, width, height, x, y, patch, &fft);
                // Skip flat patches: a phase-correlation peak on a featureless
                // area is noise, and would poison the field.
                if energy > 1.0 {
                    anchors.push(AnchorRef {
                        x,
                        y,
                        spectrum: spec,
                    });
                }
                x += step;
            }
            y += step;
        }

        Self {
            width,
            height,
            patch,
            anchors,
            fft,
            ifft,
        }
    }

    /// Number of alignment points placed.
    pub fn anchor_count(&self) -> usize {
        self.anchors.len()
    }

    /// Estimates the frame's shift by taking the median of the per-anchor
    /// estimates (after outlier rejection). Exposed for tests and diagnostics:
    /// it is the local estimation without the warp.
    pub fn mean_shift(&self, lum: &[f32], base: (f64, f64)) -> Option<(f64, f64)> {
        let anchors = self.estimate_anchors(lum, base)?;
        let med = median_shift(&anchors);
        Some(med)
    }

    /// Returns every surviving anchor's estimated shift, for diagnostics.
    pub fn public_anchor_shifts(&self, lum: &[f32], base: (f64, f64)) -> Vec<(f64, f64)> {
        match self.estimate_anchors(lum, base) {
            Some(a) => a.iter().map(|x| (x.dx, x.dy)).collect(),
            None => Vec::new(),
        }
    }

    /// Applies a constant shift through this module's warp, for diagnostics.
    /// Lets a caller compare its resampling against `align::shift_rgb16_masked`
    /// on the exact same displacement.
    pub fn apply_constant(rgb: &[u16], width: usize, height: usize, dx: f64, dy: f64) -> Vec<u16> {
        ShiftField::constant(dx, dy).warp_bicubic(rgb, width, height)
    }

    /// Estimates the local displacement field for a frame and returns the
    /// **once-warped** image brought back onto the reference.
    ///
    /// `lum` is the luminance of the *unaligned* frame, `rgb` its pixels.
    /// `base` is a coarse global shift (from the cheap global pass): it centres
    /// the local search so a large drift stays within the patch window. It is
    /// passed in rather than stored, so one aligner can be shared across
    /// parallel frames.
    ///
    /// Returns `None` when too few reliable points are found (the caller then
    /// falls back to global alignment).
    pub fn align_frame(&self, lum: &[f32], rgb: &[u16], base: (f64, f64)) -> Option<Vec<u16>> {
        let anchors = self.estimate_anchors(lum, base)?;

        // If the anchors essentially agree, the deformation is rigid: building
        // a field would only add cell quantisation and smoothing artifacts,
        // i.e. soften the frame. The 1.5 px threshold makes sub-pixel
        // disagreement read as translation rather than as turbulence.
        let med = median_shift(&anchors);
        if mad_shift(&anchors, med) < 1.5 {
            return Some(ShiftField::constant(med.0, med.1).warp_bicubic(rgb, self.width, self.height));
        }

        // Dense, smoothed field, then a single warp.
        let field = ShiftField::build(&anchors, self.width, self.height);
        Some(field.warp_bicubic(rgb, self.width, self.height))
    }

    /// Estimates the per-anchor shifts, rejects outliers, and returns the
    /// surviving anchors. `None` when too few reliable points remain.
    fn estimate_anchors(&self, lum: &[f32], base: (f64, f64)) -> Option<Vec<Anchor>> {
        let mut anchors: Vec<Anchor> = Vec::with_capacity(self.anchors.len());

        for a in &self.anchors {
            // Patch of the frame, centred on the anchor exactly like the
            // reference patch in `patch_spectrum` (which subtracts `half`).
            // A mismatch here offsets the two patches and corrupts the
            // correlation.
            let half = self.patch / 2;
            let mut buf: Vec<Complex32> = (0..self.patch * self.patch)
                .map(|i| {
                    let px = i % self.patch;
                    let py = i / self.patch;
                    // Match patch_spectrum: sample at (anchor - half + p).
                    let fx = (a.x + px).saturating_sub(half) as f64 - base.0;
                    let fy = (a.y + py).saturating_sub(half) as f64 - base.1;
                    let sx = fx.round().clamp(0.0, self.width as f64 - 1.0) as usize;
                    let sy = fy.round().clamp(0.0, self.height as f64 - 1.0) as usize;
                    let wy = 0.5
                        - 0.5
                            * (2.0 * std::f32::consts::PI * py as f32
                                / (self.patch - 1).max(1) as f32)
                                .cos();
                    let wx = 0.5
                        - 0.5
                            * (2.0 * std::f32::consts::PI * px as f32
                                / (self.patch - 1).max(1) as f32)
                                .cos();
                    Complex32::new(lum[sy * self.width + sx] * wx * wy, 0.0)
                })
                .collect();

            self.fft.process(&mut buf);
            for i in 0..buf.len() {
                let cross = buf[i] * a.spectrum[i].conj();
                let mag = cross.norm();
                buf[i] = if mag > 1e-6 {
                    cross / mag
                } else {
                    Complex32::new(0.0, 0.0)
                };
            }
            self.ifft.process(&mut buf);

            // Correlation peak.
            let (mut best_i, mut best_v) = (0usize, f32::NEG_INFINITY);
            for (i, c) in buf.iter().enumerate() {
                if c.re > best_v {
                    best_v = c.re;
                    best_i = i;
                }
            }
            let px = best_i % self.patch;
            let py = best_i / self.patch;
            let (sub_x, sub_y) = subpixel_peak(&buf, self.patch, px, py);

            // FFT wrap-around -> signed shift. Add the coarse base so the
            // returned shift is relative to the reference, not to the centred
            // search window.
            let wrap = |v: f64, n: i64| -> f64 {
                if v > n as f64 / 2.0 {
                    v - n as f64
                } else {
                    v
                }
            };
            let dx = wrap(px as f64 + sub_x, self.patch as i64) + base.0;
            let dy = wrap(py as f64 + sub_y, self.patch as i64) + base.1;

            // Reliability: a sharp peak stands out from the correlation
            // surface. Phase correlation is normalised, so both `best_v` and
            // `mean` live in the same small range: comparing their ratio is
            // meaningful, whereas comparing to the raw patch energy (billions
            // for 16-bit data) rejected every point.
            let mean: f32 = buf.iter().map(|c| c.re).sum::<f32>() / buf.len() as f32;
            let peak_to_mean = if mean.abs() > 1e-6 {
                (best_v / mean.abs()) as f64
            } else {
                f64::INFINITY
            };
            if peak_to_mean > 1.8 {
                anchors.push(Anchor { x: a.x, y: a.y, dx, dy });
            }
        }

        if anchors.len() < 4 {
            return None;
        }

        // Outlier rejection: compare each shift to the median. A single bad
        // point would otherwise tear the local field.
        let med = median_shift(&anchors);
        let spread = mad_shift(&anchors, med).max(0.75); // at least ~0.75 px
        let limit = (6.0 * spread).max(2.0); // never reject below 2 px
        anchors.retain(|a| {
            let (ex, ey) = (a.dx - med.0, a.dy - med.1);
            (ex * ex + ey * ey).sqrt() <= limit
        });
        if anchors.len() < 4 {
            return None;
        }

        Some(anchors)
    }
}

/// Median shift, component-wise (robust centre).
fn median_shift(anchors: &[Anchor]) -> (f64, f64) {
    let mut xs: Vec<f64> = anchors.iter().map(|a| a.dx).collect();
    let mut ys: Vec<f64> = anchors.iter().map(|a| a.dy).collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = anchors.len() / 2;
    (xs[mid], ys[mid])
}

/// Median absolute deviation around the median shift (robust spread).
fn mad_shift(anchors: &[Anchor], med: (f64, f64)) -> f64 {
    let mut d: Vec<f64> = anchors
        .iter()
        .map(|a| ((a.dx - med.0).powi(2) + (a.dy - med.1).powi(2)).sqrt())
        .collect();
    d.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    d[d.len() / 2]
}

/// Spectrum of a patch centred at (x, y), with a Hann window.
fn patch_spectrum(
    lum: &[f32],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    patch: usize,
    fft: &Arc<dyn Fft<f32>>,
) -> (Vec<Complex32>, f64) {
    let half = patch / 2;
    let mut buf: Vec<Complex32> = Vec::with_capacity(patch * patch);
    let mut energy = 0.0f64;

    for py in 0..patch {
        let wy =
            0.5 - 0.5 * (2.0 * std::f32::consts::PI * py as f32 / (patch - 1).max(1) as f32).cos();
        for px in 0..patch {
            let wx =
                0.5 - 0.5 * (2.0 * std::f32::consts::PI * px as f32 / (patch - 1).max(1) as f32).cos();
            let sx = (x + px).saturating_sub(half).min(width - 1);
            let sy = (y + py).saturating_sub(half).min(height - 1);
            let v = lum[sy * width + sx];
            energy += (v * v) as f64;
            buf.push(Complex32::new(v * wx * wy, 0.0));
        }
    }

    fft.process(&mut buf);
    (buf, energy / (patch * patch) as f64)
}

/// Parabolic fit of the correlation peak, for a sub-pixel estimate.
fn subpixel_peak(buf: &[Complex32], patch: usize, px: usize, py: usize) -> (f64, f64) {
    let at = |x: isize, y: isize| -> f64 {
        let xx = ((x + patch as isize) % patch as isize) as usize;
        let yy = ((y + patch as isize) % patch as isize) as usize;
        buf[yy * patch + xx].re as f64
    };
    let xf = subpixel_1d(
        at(px as isize - 1, py as isize),
        at(px as isize, py as isize),
        at(px as isize + 1, py as isize),
    );
    let yf = subpixel_1d(
        at(px as isize, py as isize - 1),
        at(px as isize, py as isize),
        at(px as isize, py as isize + 1),
    );
    (xf, yf)
}

/// Position of the vertex of the parabola through (-1, a), (0, b), (1, c).
fn subpixel_1d(a: f64, b: f64, c: f64) -> f64 {
    let denom = a - 2.0 * b + c;
    if denom.abs() < 1e-12 {
        0.0
    } else {
        (0.5 * (a - c) / denom).clamp(-0.5, 0.5)
    }
}

/// Dense displacement field built from the anchors, on a coarse grid.
///
/// The grid is finer than turbulence variations yet cheap to sample. Empty
/// cells are filled from neighbours and the whole field is then **smoothed**,
/// so it cannot tear between cells.
struct ShiftField {
    cols: usize,
    rows: usize,
    cell: usize,
    dx: Vec<f32>,
    dy: Vec<f32>,
}

impl ShiftField {
    /// A field with the same shift everywhere (a plain translation).
    fn constant(dx: f64, dy: f64) -> Self {
        Self {
            cols: 1,
            rows: 1,
            cell: 1,
            dx: vec![dx as f32],
            dy: vec![dy as f32],
        }
    }

    fn build(anchors: &[Anchor], width: usize, height: usize) -> Self {
        // Cell of about 16 px: fine enough that the field's local bilinear
        // interpolation does not itself add sub-pixel displacement error, which
        // would blur the warp. Coarser cells (32 px) measurably softened the
        // result on real frames.
        let cell = 16usize;
        let cols = width.div_ceil(cell).max(1);
        let rows = height.div_ceil(cell).max(1);

        let mut sx = vec![0.0f32; cols * rows];
        let mut sy = vec![0.0f32; cols * rows];
        let mut n = vec![0.0f32; cols * rows];
        for a in anchors {
            let cx = (a.x / cell).min(cols - 1);
            let cy = (a.y / cell).min(rows - 1);
            let i = cy * cols + cx;
            sx[i] += a.dx as f32;
            sy[i] += a.dy as f32;
            n[i] += 1.0;
        }

        let mut dx = vec![0.0f32; cols * rows];
        let mut dy = vec![0.0f32; cols * rows];
        let mut filled = vec![false; cols * rows];
        for i in 0..cols * rows {
            if n[i] > 0.0 {
                dx[i] = sx[i] / n[i];
                dy[i] = sy[i] / n[i];
                filled[i] = true;
            }
        }

        // Fill empty cells from the nearest filled neighbours (ring by ring).
        let mut frontier: Vec<usize> = (0..cols * rows).filter(|&i| filled[i]).collect();
        while !frontier.is_empty() {
            let mut next = Vec::new();
            for &i in &frontier {
                let cx = i % cols;
                let cy = i / cols;
                for (ox, oy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                    let nx = cx as i64 + ox;
                    let ny = cy as i64 + oy;
                    if nx < 0 || ny < 0 || nx >= cols as i64 || ny >= rows as i64 {
                        continue;
                    }
                    let j = ny as usize * cols + nx as usize;
                    if !filled[j] {
                        dx[j] = dx[i];
                        dy[j] = dy[i];
                        filled[j] = true;
                        next.push(j);
                    }
                }
            }
            frontier = next;
        }

        // Smooth the field: a single box-blur pass removes the steps left by
        // the fill and by noisy cells, so neighbouring pixels cannot tear.
        // More passes than this over-smooth the field and cost real sharpness
        // (a rigid shift would then be warped into a needlessly soft one).
        dx = box_blur(&dx, cols, rows);
        dy = box_blur(&dy, cols, rows);

        Self {
            cols,
            rows,
            cell,
            dx,
            dy,
        }
    }

    /// Samples the field by bilinear interpolation.
    fn sample(&self, x: f64, y: f64) -> (f64, f64) {
        let gx = (x / self.cell as f64) - 0.5;
        let gy = (y / self.cell as f64) - 0.5;
        let x0 = gx.floor().max(0.0).min((self.cols - 1) as f64) as usize;
        let y0 = gy.floor().max(0.0).min((self.rows - 1) as f64) as usize;
        let x1 = (x0 + 1).min(self.cols - 1);
        let y1 = (y0 + 1).min(self.rows - 1);
        let fx = (gx - x0 as f64).clamp(0.0, 1.0);
        let fy = (gy - y0 as f64).clamp(0.0, 1.0);

        let lerp = |a: f32, b: f32, c: f32, d: f32| -> f64 {
            let top = a as f64 * (1.0 - fx) + b as f64 * fx;
            let bot = c as f64 * (1.0 - fx) + d as f64 * fx;
            top * (1.0 - fy) + bot * fy
        };

        let i00 = y0 * self.cols + x0;
        let i01 = y0 * self.cols + x1;
        let i10 = y1 * self.cols + x0;
        let i11 = y1 * self.cols + x1;

        (
            lerp(self.dx[i00], self.dx[i01], self.dx[i10], self.dx[i11]),
            lerp(self.dy[i00], self.dy[i01], self.dy[i10], self.dy[i11]),
        )
    }

    /// Warps the image once with the field, using Catmull-Rom sampling.
    fn warp_bicubic(&self, src: &[u16], width: usize, height: usize) -> Vec<u16> {
        let mut out = vec![0u16; src.len()];
        for y in 0..height {
            for x in 0..width {
                let (dx, dy) = self.sample(x as f64, y as f64);
                let (r, g, b) = sample_catmull(src, width, height, x as f64 - dx, y as f64 - dy);
                let di = (y * width + x) * 3;
                out[di] = r;
                out[di + 1] = g;
                out[di + 2] = b;
            }
        }
        out
    }
}

/// 3x3 box blur with edge clamping, for one field channel.
fn box_blur(v: &[f32], cols: usize, rows: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; v.len()];
    for y in 0..rows {
        for x in 0..cols {
            let mut sum = 0.0f32;
            let mut n = 0.0f32;
            for oy in -1i64..=1 {
                for ox in -1i64..=1 {
                    let nx = x as i64 + ox;
                    let ny = y as i64 + oy;
                    if nx < 0 || ny < 0 || nx >= cols as i64 || ny >= rows as i64 {
                        continue;
                    }
                    sum += v[ny as usize * cols + nx as usize];
                    n += 1.0;
                }
            }
            out[y * cols + x] = sum / n.max(1.0);
        }
    }
    out
}

/// Catmull-Rom (bicubic) sample of an RGB 16-bit image.
///
/// Out-of-frame handling matches `align::sample_bicubic` exactly: pixels fully
/// outside the frame sample to black rather than extrapolating the edge. The
/// old version clamped, which smeared a band along every border and measurably
/// softened the warp (about 3% on real frames).
fn sample_catmull(src: &[u16], width: usize, height: usize, x: f64, y: f64) -> (u16, u16, u16) {
    if x < -1.0 || y < -1.0 || x > width as f64 || y > height as f64 {
        return (0, 0, 0);
    }
    let x = x.clamp(0.0, (width - 1) as f64);
    let y = y.clamp(0.0, (height - 1) as f64);
    let x0 = x.floor() as isize;
    let y0 = y.floor() as isize;
    let fx = x - x0 as f64;
    let fy = y - y0 as f64;

    let at = |px: isize, py: isize, c: usize| -> f64 {
        // Outside the frame: black, so borders match the global warp.
        if px < 0 || py < 0 || px >= width as isize || py >= height as isize {
            return 0.0;
        }
        src[(py as usize * width + px as usize) * 3 + c] as f64
    };

    let w = catmull_rom_weights(fx);
    let v = catmull_rom_weights(fy);
    let mut out = [0u16; 3];
    for c in 0..3 {
        let mut acc = 0.0;
        for (j, &vy) in v.iter().enumerate() {
            let mut row = 0.0;
            for (i, &wx) in w.iter().enumerate() {
                row += at(x0 - 1 + i as isize, y0 - 1 + j as isize, c) * wx;
            }
            acc += row * vy;
        }
        out[c] = acc.clamp(0.0, 65535.0).round() as u16;
    }
    (out[0], out[1], out[2])
}

/// Catmull-Rom weights for 4 points, given the fraction `t` in [0,1].
fn catmull_rom_weights(t: f64) -> [f64; 4] {
    let t2 = t * t;
    let t3 = t2 * t;
    [
        -0.5 * t3 + t2 - 0.5 * t,
        1.5 * t3 - 2.5 * t2 + 1.0,
        -1.5 * t3 + 2.0 * t2 + 0.5 * t,
        0.5 * t3 - 0.5 * t2,
    ]
}
