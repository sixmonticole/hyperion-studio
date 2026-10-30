//! Runs the real pipeline on a cached session while sampling peak RSS, to prove
//! the streaming-block fix keeps memory flat regardless of frame count.
//!
//! The old blockwise stacking kept every block alive in a Vec and merged at the
//! end: on 900 frames at a 3600x3600 crop that reached ~6.7 GB and rebooted an
//! 8 GB machine (kernel watchdog timeout after the swap storm). This harness
//! measures the current implementation.
//!
//! Usage: diag_memory <cache.tif...> [--frames N] [--block N] [--keep F]
//!
//! The TIFFs are used directly (no RAW demodata needed): pass a list of cached
//! frames, or use --frames to take only the first N.

use hyperion_studio_lib::model::Settings;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut files: Vec<PathBuf> = Vec::new();
    let mut max_frames = usize::MAX;
    let mut block = 10usize;
    let mut keep = 0.25f64;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--frames" => {
                i += 1;
                max_frames = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);
            }
            "--block" => {
                i += 1;
                block = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(10);
            }
            "--keep" => {
                i += 1;
                keep = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(0.25);
            }
            other => files.push(PathBuf::from(other)),
        }
        i += 1;
    }

    if files.is_empty() {
        eprintln!("usage: diag_memory <cache.tif...> [--frames N] [--block N] [--keep F]");
        std::process::exit(1);
    }
    files.truncate(max_frames);

    // A temp source dir holding symlinks, so list_raw_files sees the frames.
    let src = std::env::temp_dir().join(format!("hyperion_mem_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&src);
    std::fs::create_dir_all(&src).unwrap();
    for (idx, f) in files.iter().enumerate() {
        let link = src.join(format!("{idx:05}.tif"));
        std::os::unix::fs::symlink(f, &link).unwrap();
    }
    let cache = src.join(".hyperion_cache");
    std::fs::create_dir_all(&cache).unwrap();

    let settings = Settings {
        block_size: block,
        keep_fraction: keep,
        ..Settings::default()
    };

    // Sample our own RSS from a background thread.
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

    println!("{} frames, block={block}, keep={keep}", files.len());
    let progress = |_p: hyperion_studio_lib::model::Progress| {};
    let cancel = AtomicBool::new(false);
    let result = hyperion_studio_lib::pipeline::run_pipeline(&src, &cache, &settings, &progress, &cancel);

    stop.store(true, Ordering::Relaxed);
    std::thread::sleep(std::time::Duration::from_millis(120));

    match result {
        Ok(report) => println!(
            "OK: {} frames kept, crop {}x{}",
            report.kept_frames, report.crop.width, report.crop.height
        ),
        Err(e) => println!("FAILED: {e:#}"),
    }
    let m = peak.load(Ordering::Relaxed);
    println!("peak RSS = {:.0} MB", m as f64 / 1024.0 / 1024.0);

    let _ = std::fs::remove_dir_all(&src);
}

/// Our own resident set size, from `ps`. Best effort.
fn self_rss_bytes() -> Option<u64> {
    let pid = std::process::id();
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let kb: u64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
    Some(kb * 1024)
}
