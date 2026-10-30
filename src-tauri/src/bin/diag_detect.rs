//! Reports the detected object bounding box for each cached frame, to find the
//! outliers that inflate the union crop to the whole sensor.
//!
//! Usage: diag_detect <cache_dir> [--step N]

use hyperion_studio_lib::pipeline::{decode, detect};
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = PathBuf::from(args.get(1).cloned().unwrap_or_else(|| {
        eprintln!("usage: diag_detect <cache_dir> [--step N]");
        std::process::exit(1);
    }));
    let mut step = 20usize;
    let mut i = 2;
    while i < args.len() {
        if args[i] == "--step" {
            i += 1;
            step = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(20);
        }
        i += 1;
    }

    let mut tiffs: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("tif"))
                .unwrap_or(false)
        })
        .filter(|p| {
            !p.file_name()
                .map(|n| n.to_string_lossy().starts_with("._"))
                .unwrap_or(false)
        })
        .collect();
    tiffs.sort();
    println!("{} frames, every {step}", tiffs.len());

    let mut wide: Vec<(String, usize, usize, usize, usize, f64)> = Vec::new();
    for (n, t) in tiffs.iter().enumerate() {
        if n % step != 0 {
            continue;
        }
        let Ok((rgb, w, h)) = decode::read_rgb16_tiff(t) else {
            println!("{n:>4} {}: unreadable", t.file_name().unwrap().to_string_lossy());
            continue;
        };
        match detect::detect_planet(&rgb, w, h, None) {
            Some(d) => {
                let bw = d.bbox.2 - d.bbox.0;
                let bh = d.bbox.3 - d.bbox.1;
                println!(
                    "{n:>4} {:<26} bbox {bw:>5}x{bh:<5} at ({:>5},{:>5})  r={:.0}",
                    t.file_name().unwrap().to_string_lossy(),
                    d.bbox.0,
                    d.bbox.1,
                    d.radius
                );
                // Flag anything materially wider than the typical Moon.
                if bw > 2600 || bh > 2600 {
                    wide.push((
                        t.file_name().unwrap().to_string_lossy().to_string(),
                        d.bbox.0,
                        d.bbox.1,
                        bw,
                        bh,
                        d.radius,
                    ));
                }
            }
            None => println!(
                "{n:>4} {}: NOT DETECTED",
                t.file_name().unwrap().to_string_lossy()
            ),
        }
    }

    println!("\n=== outliers (bbox > 2600) ===");
    for (name, x, y, w, h, r) in &wide {
        println!("{name}: {w}x{h} at ({x},{y}) r={r:.0}");
    }
    if wide.is_empty() {
        println!("none");
    }
}
