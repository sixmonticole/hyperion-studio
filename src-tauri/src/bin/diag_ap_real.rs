//! Diagnostic on REAL cache frames: chase the Y bias in the AP aligner.
//!
//! Compares, on a pair of consecutive real frames:
//! - the global aligner's shift,
//! - the AP aligner's median shift,
//! - the AP per-anchor estimates (to see the spread and any systematic Y skew).
//!
//! Usage: cargo run --features devtools --bin diag_ap_real -- <cache.tif> <cache2.tif>

use hyperion_studio_lib::pipeline::{align, ap, decode};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (p1, p2) = match (args.get(1), args.get(2)) {
        (Some(a), Some(b)) => (a.clone(), b.clone()),
        _ => {
            eprintln!("usage: diag_ap_real <ref.tif> <frame.tif>");
            return;
        }
    };

    let (r1, w, h) = decode::read_rgb16_tiff(std::path::Path::new(&p1)).expect("read ref");
    let (r2, w2, h2) = decode::read_rgb16_tiff(std::path::Path::new(&p2)).expect("read frame");
    assert_eq!((w, h), (w2, h2));

    let ref_lum = align::luminance(&r1, w * h);
    let frame_lum = align::luminance(&r2, w * h);
    println!("frame {w}x{h}");

    // Global shift on the full image.
    let ga = align::Aligner::new(&ref_lum, w, h);
    let g = ga.estimate(&frame_lum);
    println!("global shift = ({:+.3}, {:+.3})", g.0, g.1);

    // Verify the global shift by warping and measuring sharpness.
    let (g_warp, _) = align::shift_rgb16_masked(&r2, w, h, g);
    let s_ref = sharp(&r1, w, h);
    let s_frame = sharp(&r2, w, h);
    let s_g = sharp(&g_warp, w, h);
    println!("sharpness: ref={s_ref:.1} frame={s_frame:.1} global-warp={s_g:.1}");

    // AP with the global shift as the coarse base.
    let apx = ap::ApAligner::new(&ref_lum, w, h, 64);
    println!("anchors = {}", apx.anchor_count());
    match apx.mean_shift(&frame_lum, g) {
        Some((dx, dy)) => {
            println!("AP median shift = ({dx:+.3}, {dy:+.3})");
            println!("delta vs global = ({:+.3}, {:+.3})", dx - g.0, dy - g.1);
        }
        None => println!("AP: no reliable anchors"),
    }

    // Warp with AP and compare sharpness with the global warp. This is the
    // number that decides whether AP is worth enabling.
    if let Some(ap_warp) = apx.align_frame(&frame_lum, &r2, g) {
        let s_ap = sharp(&ap_warp, w, h);
        println!("sharpness: AP-warp={s_ap:.3} (global-warp={s_g:.3})");
        println!("AP vs global = {:+.2}%", (s_ap / s_g - 1.0) * 100.0);
    } else {
        println!("AP warp: None");
    }

    // Isolate the warp: apply the SAME constant shift through both functions
    // and count differing pixels / mean absolute difference.
    let (g_const, _) = align::shift_rgb16_masked(&r2, w, h, (g.0, g.1));
    let ap_const = ap::ApAligner::apply_constant(&r2, w, h, g.0, g.1);
    let mut diff_px = 0usize;
    let mut abs_sum = 0f64;
    for i in 0..g_const.len() {
        let a = g_const[i] as i64;
        let b = ap_const[i] as i64;
        if a != b {
            diff_px += 1;
        }
        abs_sum += (a - b).abs() as f64;
    }
    println!(
        "warp mismatch: {diff_px} / {} values, mean|diff| = {:.3}",
        g_const.len(),
        abs_sum / g_const.len() as f64
    );

    // Per-anchor dump, to reveal a Y skew in the estimates themselves.
    let shifts = apx.public_anchor_shifts(&frame_lum, g);
    if !shifts.is_empty() {
        let xs: Vec<f64> = shifts.iter().map(|s| s.0).collect();
        let ys: Vec<f64> = shifts.iter().map(|s| s.1).collect();
        let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
        let min = |v: &[f64]| v.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = |v: &[f64]| v.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        println!("anchors: n={}", shifts.len());
        println!("  x: mean={:+.3} min={:+.3} max={:+.3}", mean(&xs), min(&xs), max(&xs));
        println!("  y: mean={:+.3} min={:+.3} max={:+.3}", mean(&ys), min(&ys), max(&ys));
    }

    // Decisive measure: mean squared error of each warp against the reference,
    // over the central region. Lower MSE means the frame landed closer to the
    // reference, i.e. better alignment. This is independent of the sharpness
    // metric, which is known to be fooled by edges and noise.
    if let Some(ap_warp) = apx.align_frame(&frame_lum, &r2, g) {
        let mse_g = central_mse(&r1, &g_const, w, h);
        let mse_ap = central_mse(&r1, &ap_warp, w, h);
        println!("MSE vs reference (central): global={mse_g:.1} AP={mse_ap:.1}");
        println!(
            "AP MSE change = {:+.3}% (negative = closer to the reference)",
            (mse_ap / mse_g - 1.0) * 100.0
        );
    }
}

/// Mean squared error between a reference RGB frame and another, over the
/// central region (borders excluded, since warps fill them differently).
fn central_mse(ref_rgb: &[u16], rgb: &[u16], w: usize, h: usize) -> f64 {
    let m = (w / 10).min(h / 10);
    let mut sum = 0f64;
    let mut n = 0f64;
    for y in m..h - m {
        for x in m..w - m {
            let i = (y * w + x) * 3;
            for c in 0..3 {
                let d = ref_rgb[i + c] as f64 - rgb[i + c] as f64;
                sum += d * d;
                n += 1.0;
            }
        }
    }
    sum / n.max(1.0)
}

fn sharp(rgb: &[u16], w: usize, h: usize) -> f64 {
    // Score the central 80% only: a warped frame has black bands at the
    // borders, and the black/image transition creates huge fake gradients that
    // would make a coarser warp look "sharper". Excluding the border measures
    // real detail.
    let m = (w / 10).min(h / 10).max(0);
    if w <= 2 * m || h <= 2 * m {
        return hyperion_studio_lib::pipeline::quality::sharpness_score(rgb, w, h);
    }
    let cw = w - 2 * m;
    let ch = h - 2 * m;
    let mut crop = vec![0u16; cw * ch * 3];
    for y in 0..ch {
        let src = ((y + m) * w + m) * 3;
        let dst = y * cw * 3;
        crop[dst..dst + cw * 3].copy_from_slice(&rgb[src..src + cw * 3]);
    }
    hyperion_studio_lib::pipeline::quality::sharpness_score(&crop, cw, ch)
}
