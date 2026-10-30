//! Ranks every RAW in a folder by sharpness, streaming, without filling the
//! disk.
//!
//! Why streaming: a demosaiced A6000 RAW is ~145 MB of cache. Decoding all 993
//! frames at once needs ~144 GB. Instead each frame is decoded, scored, and its
//! cache TIFF is **deleted immediately** unless the frame makes the running
//! top-N. Peak extra disk is then N x 145 MB.
//!
//! Detection (crop window) is done once, on the first frame, and reused: the
//! whole session shares a tripod, so the object moves only by drift.
//!
//! Usage: rank_raw <source_dir> <cache_dir> [--keep N] [--fraction F]
//!
//! Writes `<cache_dir>/ranking.json` (order, scores, kept flags) and leaves the
//! top-N cache TIFFs in place, so a normal run afterwards is instant.

use hyperion_studio_lib::model::CropRect;
use hyperion_studio_lib::pipeline::{decode, detect, quality};
use std::path::{Path, PathBuf};

struct Entry {
    index: usize,
    name: String,
    score: f64,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let src = PathBuf::from(args.get(1).cloned().unwrap_or_else(|| {
        eprintln!("usage: rank_raw <source_dir> <cache_dir> [--keep N] [--fraction F]");
        std::process::exit(1);
    }));
    let cache = PathBuf::from(args.get(2).cloned().unwrap_or_else(|| {
        eprintln!("usage: rank_raw <source_dir> <cache_dir> [--keep N] [--fraction F]");
        std::process::exit(1);
    }));

    let mut keep: Option<usize> = None;
    let mut fraction: f64 = 0.10;
    let mut i = 3;
    while i < args.len() {
        match args[i].as_str() {
            "--keep" => {
                i += 1;
                keep = args.get(i).and_then(|s| s.parse().ok());
            }
            "--fraction" => {
                i += 1;
                fraction = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(0.10);
            }
            _ => {}
        }
        i += 1;
    }

    std::fs::create_dir_all(&cache).expect("create cache dir");

    let mut files = decode::list_raw_files(&src).expect("list raw files");
    files.sort();
    let total = files.len();
    if total == 0 {
        eprintln!("no RAW file in {}", src.display());
        std::process::exit(1);
    }
    let keep_n = keep.unwrap_or_else(|| ((total as f64 * fraction).ceil() as usize).max(1));
    println!("{total} frames, keeping {keep_n}");

    // --- Detection on the first frame: one crop window for the whole session.
    let first = &files[0];
    let t0 = std::time::Instant::now();
    let d0 = decode::decode_raw_to_rgb16(first).expect("decode first frame");
    println!(
        "reference {} ({}x{}, {}) in {:.1}s",
        first.file_name().unwrap().to_string_lossy(),
        d0.width,
        d0.height,
        d0.model,
        t0.elapsed().as_secs_f64()
    );
    let det = detect::detect_planet(&d0.rgb16, d0.width, d0.height, None)
        .expect("object not detected on the first frame");
    let crop: CropRect = detect::crop_window(&det, d0.width, d0.height, 0, 0.0);
    println!(
        "crop {}x{} at ({}, {}), object radius {:.0} px",
        crop.width, crop.height, crop.x, crop.y, det.radius
    );
    drop(d0);

    // Keep the ranking in a min-heap-like structure: we only ever need the
    // best `keep_n`, so a Vec we sort at the end is fine, but we trim it as we
    // go to bound memory.
    let mut entries: Vec<Entry> = Vec::with_capacity(total);
    let start = std::time::Instant::now();

    for (index, path) in files.iter().enumerate() {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let Ok(d) = decode::decode_raw_to_rgb16(path) else {
            println!("[{index:>4}/{total}] {name}: decode failed, skipped");
            continue;
        };
        let cropped = crop_rgb16(&d.rgb16, d.width, d.height, &crop);
        let score = quality::sharpness_score(&cropped, crop.width, crop.height);
        drop(cropped);
        drop(d);

        entries.push(Entry {
            index,
            name: name.clone(),
            score,
        });

        // Progress, with an ETA, every 25 frames.
        if index % 25 == 0 {
            let done = index + 1;
            let per = start.elapsed().as_secs_f64() / done as f64;
            let eta = per * (total - done) as f64;
            println!(
                "[{done:>4}/{total}] {name}: score {score:>8.2}  ({per:.2}s/frame, ~{:.0} min left)",
                eta / 60.0
            );
        }
    }

    // Rank best first, then keep the top N.
    entries.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let kept_indices: std::collections::HashSet<usize> =
        entries.iter().take(keep_n).map(|e| e.index).collect();

    println!("\n=== ranking (best {keep_n}) ===");
    for (rank, e) in entries.iter().take(keep_n.min(30)).enumerate() {
        println!("{:>3}. {:<16} {:>8.2}{}", rank + 1, e.name, e.score, if rank == keep_n.min(30) - 1 && keep_n > 30 { "  ..." } else { "" });
    }
    println!("worst: {:<16} {:>8.2}", entries.last().unwrap().name, entries.last().unwrap().score);

    // --- Write the cache only for the kept frames, deleting any other TIFF.
    let mut written = 0usize;
    let mut removed = 0usize;
    for (index, path) in files.iter().enumerate() {
        let cache_path = hyperion_studio_lib::pipeline::cache::cache_path_for(&cache, path, index);
        let should_keep = kept_indices.contains(&index);
        if should_keep {
            let d = match decode::decode_raw_to_rgb16(path) {
                Ok(d) => d,
                Err(_) => continue,
            };
            decode::write_rgb16_tiff(&cache_path, &d.rgb16, d.width, d.height)
                .expect("write cache tiff");
            written += 1;
        } else if cache_path.exists() {
            let _ = std::fs::remove_file(&cache_path);
            removed += 1;
        }
    }
    println!("\ncache: {written} kept, {removed} removed");

    // --- Ranking report, so a later run can reuse the selection.
    let report = serde_json::json!({
        "source_dir": src.to_string_lossy(),
        "total": total,
        "keep_n": keep_n,
        "crop": { "x": crop.x, "y": crop.y, "width": crop.width, "height": crop.height },
        "ranking": entries.iter().enumerate().map(|(rank, e)| serde_json::json!({
            "rank": rank + 1,
            "index": e.index,
            "name": e.name,
            "score": e.score,
            "keep": kept_indices.contains(&e.index),
        })).collect::<Vec<_>>(),
    });
    let out = cache.join("ranking.json");
    std::fs::write(&out, serde_json::to_string_pretty(&report).unwrap()).expect("write ranking");
    println!("ranking written to {}", out.display());
}

/// Extracts a sub-window from an RGB 16-bit image.
fn crop_rgb16(rgb16: &[u16], width: usize, height: usize, crop: &CropRect) -> Vec<u16> {
    let mut out = vec![0u16; crop.width * crop.height * 3];
    for y in 0..crop.height {
        let sy = crop.y + y;
        if sy >= height {
            break;
        }
        for x in 0..crop.width {
            let sx = crop.x + x;
            if sx >= width {
                break;
            }
            let si = (sy * width + sx) * 3;
            let di = (y * crop.width + x) * 3;
            out[di] = rgb16[si];
            out[di + 1] = rgb16[si + 1];
            out[di + 2] = rgb16[si + 2];
        }
    }
    out
}

// Keep `Path` import used even if the tool changes.
#[allow(dead_code)]
fn _unused(p: &Path) {
    let _ = p;
}
