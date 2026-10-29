//! Analyse le profil d'intensite d'un stack brut, pour calibrer le
//! post-traitement sans ecraser les hautes lumieres (Tycho, limbe).
//!
//! Affiche la distribution (percentiles), et le niveau local dans une zone
//! brillante (Tycho) et une zone moyenne (une mer).
//!
//! Usage : analyze_levels <stack.tif> [x_tycho y_tycho]

use hyperion_studio_lib::pipeline::decode;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let path = a.get(1).cloned().expect("usage: analyze_levels <stack.tif>");
    let (rgb, w, h) = decode::read_rgb16_tiff(std::path::Path::new(&path)).expect("read");
    println!("{w}x{h}");

    // Distribution des niveaux sur le canal vert (proxy de luminance).
    let mut hist = vec![0u64; 4096];
    for px in rgb.chunks_exact(3) {
        hist[((px[1] as usize) * 4095) / 65535] += 1;
    }
    let total: u64 = hist.iter().sum();
    let pct = |p: f64| -> usize {
        let target = (total as f64 * p) as u64;
        let mut acc = 0u64;
        for (i, &c) in hist.iter().enumerate() {
            acc += c;
            if acc >= target {
                return (i * 65535) / 4095;
            }
        }
        65535
    };
    println!("\npercentiles de niveau (sur 65535) :");
    for p in [0.05, 0.25, 0.50, 0.75, 0.90, 0.95, 0.99, 0.995, 0.999, 0.9999] {
        println!("  p{:<7} = {:>6}  ({:.4})", format!("{:.2}", p * 100.0), pct(p), pct(p) as f64 / 65535.0);
    }

    // Part de pixels satures.
    let sat = hist[4095];
    let near_sat: u64 = hist[4085..].iter().sum();
    println!("\npixels au maximum : {sat} ({:.5}%)", sat as f64 * 100.0 / total as f64);
    println!("pixels > 99,7% du max : {near_sat} ({:.4}%)", near_sat as f64 * 100.0 / total as f64);

    // Moyenne locale dans une zone (Tycho si donne, sinon centre).
    let (cx, cy) = if a.len() >= 4 {
        (a[2].parse::<usize>().unwrap_or(w / 2), a[3].parse::<usize>().unwrap_or(h / 2))
    } else {
        (w / 2, h / 2)
    };
    let (mut s, mut n, mut mx) = (0u64, 0u64, 0u16);
    for y in cy.saturating_sub(60)..(cy + 60).min(h) {
        for x in cx.saturating_sub(60)..(cx + 60).min(w) {
            let v = rgb[(y * w + x) * 3 + 1];
            s += v as u64;
            n += 1;
            mx = mx.max(v);
        }
    }
    println!(
        "\nzone 120x120 en ({cx},{cy}) : moyenne={:.0} max={} (soit {:.3} en [0,1])",
        s as f64 / n.max(1) as f64,
        mx,
        mx as f64 / 65535.0
    );
}
