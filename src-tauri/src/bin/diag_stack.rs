//! Isolates a stacking regression by running the real pipeline over a cache of
//! frames with one setting varied, and writing a preview of each result.
//!
//! Usage: diag_stack <cache_dir> <out_prefix> [--tile on|off] [--crop N]
//!
//! The cache holds TIFFs already demosaiced; a temp source folder of symlinks
//! is built so `list_raw_files` sees them and the cache is reused as-is.

use hyperion_studio_lib::model::Settings;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: diag_stack <cache_dir> <out_prefix> [--tile on|off] [--crop N]");
        std::process::exit(1);
    }
    let cache_dir = PathBuf::from(&args[1]);
    let prefix = args[2].clone();

    let mut tile = true;
    let mut crop_force = 0usize;
    let mut keep = 0.25f64;
    let mut i = 3;
    while i < args.len() {
        match args[i].as_str() {
            "--tile" => {
                i += 1;
                tile = args.get(i).map(|s| s == "on").unwrap_or(true);
            }
            "--crop" => {
                i += 1;
                crop_force = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(0);
            }
            "--keep" => {
                i += 1;
                keep = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(0.25);
            }
            _ => {}
        }
        i += 1;
    }

    // Temp source folder of symlinks onto the cached TIFFs, so the pipeline
    // treats them as an already-converted folder (no re-demosaic).
    let src = std::env::temp_dir().join(format!("hyperion_stack_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&src);
    std::fs::create_dir_all(&src).unwrap();

    let mut tiffs: Vec<PathBuf> = std::fs::read_dir(&cache_dir)
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
    println!("{} cached TIFFs", tiffs.len());
    for (n, t) in tiffs.iter().enumerate() {
        std::os::unix::fs::symlink(t, src.join(format!("{n:05}.tif"))).unwrap();
    }

    let settings = Settings {
        tile_select: tile,
        crop_size: crop_force,
        keep_fraction: keep,
        ..Settings::default()
    };

    println!("running: tile_select={tile} crop_size={crop_force} keep={keep}");
    let progress = |_p: hyperion_studio_lib::model::Progress| {};
    let cancel = AtomicBool::new(false);
    let cache_for_run = src.join(".hyperion_cache");
    std::fs::create_dir_all(&cache_for_run).unwrap();

    match hyperion_studio_lib::pipeline::run_pipeline(
        &src,
        &cache_for_run,
        &settings,
        &progress,
        &cancel,
    ) {
        Ok(r) => {
            println!(
                "OK crop {}x{} at ({},{}), kept {}",
                r.crop.width, r.crop.height, r.crop.x, r.crop.y, r.kept_frames
            );
            // Preview the produced stack.
            let out = r.output_path.clone();
            let png = format!("{prefix}.png");
            let p = Path::new(&out);
            match hyperion_studio_lib::pipeline::preview::render_preview(p, 1200, None) {
                Ok(data) => {
                    // data is a data URL; strip the prefix and write the bytes.
                    use base64::Engine;
                    if let Some(b64) = data.strip_prefix("data:image/png;base64,") {
                        if let Ok(bytes) =
                            base64::engine::general_purpose::STANDARD.decode(b64)
                        {
                            std::fs::write(&png, bytes).unwrap();
                            println!("preview -> {png}");
                        }
                    }
                }
                Err(e) => println!("preview failed: {e:#}"),
            }
        }
        Err(e) => println!("FAILED: {e:#}"),
    }

    let _ = std::fs::remove_dir_all(&src);
}
