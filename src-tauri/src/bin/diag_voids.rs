//! Reports where a stacked TIFF is empty (masked out), row by row and column by
//! column. A block-merge that pushes the object off the crop leaves whole bands
//! at exact zero; this locates them.
//!
//! Usage: diag_voids <stack.tif>

use hyperion_studio_lib::pipeline::decode;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(path) = args.get(1) else {
        eprintln!("usage: diag_voids <stack.tif>");
        std::process::exit(1);
    };
    let (rgb, w, h) = decode::read_rgb16_tiff(std::path::Path::new(path)).expect("read");
    println!("{w}x{h}");

    // A pixel counts as "empty" when all three channels are exactly zero.
    let empty_at = |x: usize, y: usize| -> bool {
        let i = (y * w + x) * 3;
        rgb[i] == 0 && rgb[i + 1] == 0 && rgb[i + 2] == 0
    };

    let row_empty: Vec<usize> = (0..h)
        .map(|y| (0..w).filter(|&x| empty_at(x, y)).count())
        .collect();
    let col_empty: Vec<usize> = (0..w)
        .map(|x| (0..h).filter(|&y| empty_at(x, y)).count())
        .collect();

    let report = |name: &str, v: &[usize], total: usize| {
        let full: Vec<usize> = v
            .iter()
            .enumerate()
            .filter(|(_, c)| **c == total)
            .map(|(i, _)| i)
            .collect();
        let partial = v.iter().filter(|&&c| c > 0 && c < total).count();
        println!("{name}: {} fully empty, {partial} partially empty", full.len());
        if !full.is_empty() {
            let (lo, hi) = (full[0], *full.last().unwrap());
            println!("   fully-empty range: {lo}..={hi}");
        }
    };

    report("rows   ", &row_empty, w);
    report("columns", &col_empty, h);

    // Coarse map: 20x20 blocks, '#' = mostly empty, '.' = mostly filled.
    println!("\nvoid map (# = empty, . = filled):");
    let bx = (w / 20).max(1);
    let by = (h / 20).max(1);
    for ty in 0..20 {
        let mut line = String::new();
        for tx in 0..20 {
            let x0 = tx * bx;
            let y0 = ty * by;
            let mut n = 0usize;
            let mut e = 0usize;
            for y in y0..(y0 + by).min(h) {
                for x in x0..(x0 + bx).min(w) {
                    n += 1;
                    if empty_at(x, y) {
                        e += 1;
                    }
                }
            }
            let frac = e as f64 / n.max(1) as f64;
            line.push(if frac > 0.5 {
                '#'
            } else if frac > 0.05 {
                '+'
            } else {
                '.'
            });
        }
        println!("  {line}");
    }
}
