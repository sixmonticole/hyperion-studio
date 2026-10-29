//! Binaire de validation manuelle : decode un RAW et ecrit un TIFF + un PNG
//! d'apercu, pour verifier le dematriçage sans passer par l'UI.
//!
//! Usage : cargo run --bin probe -- <fichier.ARW> [sortie.tif]

use hyperion_studio_lib::pipeline::decode;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(input) = args.get(1) else {
        eprintln!("usage: probe <fichier.ARW> [sortie.tif]");
        std::process::exit(2);
    };
    let out = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| "probe_out.tif".to_string());

    let path = std::path::Path::new(input);
    eprintln!("decodage de {input} ...");
    let t0 = std::time::Instant::now();

    let decoded = match decode::decode_raw_to_rgb16(path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("ECHEC: {e:#}");
            std::process::exit(1);
        }
    };

    eprintln!(
        "OK: {}x{} (modele {}), {} px RGB16, en {:?}",
        decoded.width,
        decoded.height,
        decoded.model,
        decoded.rgb16.len() / 3,
        t0.elapsed()
    );

    // Statistiques rapides par canal (pour reperer un desequilibre de balance).
    let n = (decoded.rgb16.len() / 3) as f64;
    let mut sum = [0.0f64; 3];
    let mut max = [0u16; 3];
    for px in decoded.rgb16.chunks_exact(3) {
        for c in 0..3 {
            sum[c] += px[c] as f64;
            if px[c] > max[c] {
                max[c] = px[c];
            }
        }
    }
    eprintln!(
        "moyenne R={:.0} V={:.0} B={:.0} | max R={} V={} B={}",
        sum[0] / n,
        sum[1] / n,
        sum[2] / n,
        max[0],
        max[1],
        max[2]
    );

    if let Err(e) = decode::write_rgb16_tiff(std::path::Path::new(&out), &decoded.rgb16, decoded.width, decoded.height) {
        eprintln!("ECHEC ecriture TIFF: {e:#}");
        std::process::exit(1);
    }
    eprintln!("ecrit: {out}");
}
