//! Memory proof at scale, without a RAW session.
//!
//! Generates N synthetic cache-sized TIFFs (a lunar-looking disc on black, at
//! the real sensor size), then runs the REAL pipeline over them and samples
//! peak RSS. This reproduces the 900-frame geometry that rebooted an 8 GB
//! machine, so the streaming-block fix can be measured for what it is.
//!
//! Usage: diag_memory_synth [--frames N] [--size S] [--block B]

use hyperion_studio_lib::model::Settings;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut frames = 300usize;
    let mut side = 4024usize;
    let mut block = 10usize;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--frames" => {
                i += 1;
                frames = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(300);
            }
            "--size" => {
                i += 1;
                side = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(4024);
            }
            "--block" => {
                i += 1;
                block = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(10);
            }
            _ => {}
        }
        i += 1;
    }

    let src = std::env::temp_dir().join(format!("hyperion_synth_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&src);
    std::fs::create_dir_all(&src).unwrap();

    println!("generating {frames} frames of {side}x{side}...");
    let (w, h) = (side, side);
    for idx in 0..frames {
        let mut img = vec![0u16; w * h * 3];
        // A disc that drifts a little per frame, with fine texture: enough for
        // the detector and the quality scorer to behave realistically.
        let cx = w as f64 / 2.0 + (idx as f64 * 0.7);
        let cy = h as f64 / 2.0 + (idx as f64 * 0.4);
        let r = (side as f64) * 0.42;
        for y in 0..h {
            for x in 0..w {
                let d = ((x as f64 - cx).powi(2) + (y as f64 - cy).powi(2)).sqrt();
                let v: u16 = if d <= r {
                    let t = (x as u32 * 7 + y as u32 * 13) % 97;
                    (38000 + (t as u16) * 200).min(60000)
                } else {
                    1500
                };
                let p = (y * w + x) * 3;
                img[p] = v;
                img[p + 1] = v;
                img[p + 2] = v;
            }
        }
        let path: PathBuf = src.join(format!("{idx:05}.tif"));
        hyperion_studio_lib::pipeline::decode::write_rgb16_tiff(&path, &img, w, h).unwrap();
    }
    println!("generated.");

    let cache = src.join(".hyperion_cache");
    std::fs::create_dir_all(&cache).unwrap();

    let settings = Settings {
        block_size: block,
        keep_fraction: 1.0,
        ..Settings::default()
    };

    let peak = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    {
        let peak = peak.clone();
        let stop = stop.clone();
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                if let Some(rss) = self_rss_bytes() {
                    peak.fetch_max(rss, Ordering::Relaxed);
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        });
    }

    println!("running pipeline: block={block}");
    let progress = |_p: hyperion_studio_lib::model::Progress| {};
    let cancel = AtomicBool::new(false);
    let result =
        hyperion_studio_lib::pipeline::run_pipeline(&src, &cache, &settings, &progress, &cancel);

    stop.store(true, Ordering::Relaxed);
    std::thread::sleep(std::time::Duration::from_millis(150));

    match result {
        Ok(report) => println!(
            "OK: {} kept, crop {}x{}",
            report.kept_frames, report.crop.width, report.crop.height
        ),
        Err(e) => println!("FAILED: {e:#}"),
    }
    let m = peak.load(Ordering::Relaxed);
    println!("PEAK RSS = {:.0} MB", m as f64 / 1024.0 / 1024.0);

    let _ = std::fs::remove_dir_all(&src);
}

fn self_rss_bytes() -> Option<u64> {
    let pid = std::process::id();
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let kb: u64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
    Some(kb * 1024)
}
