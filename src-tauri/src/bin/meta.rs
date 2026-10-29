//! Affiche les metadonnees de couleur d'un RAW : balance des blancs encodee,
//! niveaux noir/blanc, CFA, taille, orientation. Pour diagnostiquer les
//! dominantes de couleur et le sens de l'image.
//!
//! Usage : meta <fichier.ARW>

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(input) = args.get(1) else {
        eprintln!("usage: meta <fichier.ARW>");
        std::process::exit(2);
    };
    let raw = match rawler::decode_file(std::path::Path::new(input)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("ECHEC: {e}");
            std::process::exit(1);
        }
    };

    println!("modele        : {} {}", raw.clean_make, raw.clean_model);
    println!(
        "taille        : {}x{}  cpp={} bps={}",
        raw.width, raw.height, raw.cpp, raw.bps
    );
    println!("cfa           : {}", raw.camera.cfa);
    println!("photometric   : {:?}", raw.photometric);
    println!("orientation   : {:?}", raw.orientation);
    println!("wb_coeffs     : {:?}", raw.wb_coeffs);
    println!("neutralwb     : {:?}", raw.neutralwb());
    println!("blacklevel    : {:?}", raw.blacklevel);
    println!("whitelevel    : {:?}", raw.whitelevel);
    if let Some(a) = raw.active_area {
        println!(
            "active_area   : x={} y={} w={} h={}",
            a.p.x, a.p.y, a.d.w, a.d.h
        );
    }
    if let Some(c) = raw.crop_area {
        println!(
            "crop_area     : x={} y={} w={} h={}",
            c.p.x, c.p.y, c.d.w, c.d.h
        );
    }
}
