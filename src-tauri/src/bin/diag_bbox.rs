//! Finds the bounding box of the bright object in a stack, and whether it
//! touches the frame border. Distinguishes "the Moon is cut by the crop edge"
//! from "the corner is just sky".
//!
//! Usage: diag_bbox <stack.tif> [--thresh F]

use hyperion_studio_lib::pipeline::decode;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(path) = args.get(1) else {
        eprintln!("usage: diag_bbox <stack.tif> [--thresh F]");
        std::process::exit(1);
    };
    let mut frac = 0.10f64;
    let mut i = 2;
    while i < args.len() {
        if args[i] == "--thresh" {
            i += 1;
            frac = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(0.10);
        }
        i += 1;
    }

    let (rgb, w, h) = decode::read_rgb16_tiff(std::path::Path::new(path)).expect("read");
    println!("{w}x{h}");

    let lum = |x: usize, y: usize| -> f64 {
        let i = (y * w + x) * 3;
        0.299 * rgb[i] as f64 + 0.587 * rgb[i + 1] as f64 + 0.114 * rgb[i + 2] as f64
    };

    // White point from the bright part of the image.
    let mut hist = vec![0u32; 65536];
    for px in rgb.chunks_exact(3) {
        let l = (0.299 * px[0] as f64 + 0.587 * px[1] as f64 + 0.114 * px[2] as f64) as usize;
        hist[l.min(65535)] += 1;
    }
    let total: u64 = hist.iter().map(|&c| c as u64).sum();
    let target = (total as f64 * 0.995) as u64;
    let mut acc = 0u64;
    let mut white = 1usize;
    for (k, &c) in hist.iter().enumerate() {
        acc += c as u64;
        if acc >= target {
            white = k.max(1);
            break;
        }
    }
    let thr = white as f64 * frac;
    println!("white point {white}, threshold {thr:.0}");

    let (mut minx, mut miny, mut maxx, mut maxy) = (usize::MAX, usize::MAX, 0usize, 0usize);
    for y in 0..h {
        for x in 0..w {
            if lum(x, y) > thr {
                minx = minx.min(x);
                miny = miny.min(y);
                maxx = maxx.max(x);
                maxy = maxy.max(y);
            }
        }
    }
    println!("bright bbox: x {minx}..{maxx}  y {miny}..{maxy}  ({w}x{h})");
    let m = 4;
    println!(
        "touches: left={} right={} top={} bottom={}",
        minx <= m,
        maxx >= w - 1 - m,
        miny <= m,
        maxy >= h - 1 - m
    );
}
