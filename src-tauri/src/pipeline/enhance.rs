//! Post-traitement de l'image finale : niveaux, ondelettes multi-echelles,
//! contraste local (clarte) et CLAHE.
//!
//! Objectif : transformer un stack brut (lineaire, un peu mou) en une image au
//! rendu proche d'un traitement Registax/Siril, avec du relief et des details
//! fins bien marques.
//!
//! **Ordre du pipeline, et pourquoi il est important :**
//!
//! 1. **Normalisation prealable** : le stack brut sort a un niveau bas (souvent
//!    ~0,1 en [0,1]) parce qu'il est lineaire. Si on applique les ondelettes
//!    avant de ramener l'image dans une plage exploitable, les coefficients de
//!    detail sont eux aussi minuscules, et le rehaussement n'a quasiment aucun
//!    effet visible (au mieux il amplifie le bruit).
//! 2. **Niveaux + gamma** : on etire l'histogramme pour utiliser toute la plage
//!    [0, 1]. C'est ce qui donne a l'image son contraste de base.
//! 3. **Ondelettes (a trous)** : on decompose l'image normalisee en echelles et
//!    on amplifie les coefficients de detail. Comme l'image est deja dans une
//!    plage utile, le gain se voit.
//! 4. **CLAHE** : egalisation adaptative par blocs, pour le contraste local.
//! 5. **Clarte** : accentuation du contraste local par un passe-haut large.
//!
//! Auparavant l'ordre etait ondelettes -> niveaux, ce qui annulait l'effet des
//! ondelettes : l'image finale etait plus molle que le stack brut.

use crate::model::EnhanceSettings;

/// Ameliore une image RGB 16-bit selon les reglages donnes.
///
/// Renvoie une nouvelle image de meme dimensions, en 16-bit.
pub fn enhance(rgb16: &[u16], width: usize, height: usize, s: &EnhanceSettings) -> Vec<u16> {
    if !s.enabled {
        return rgb16.to_vec();
    }
    let pixels = width * height;

    // Traitement sur trois plans flottants [0, 1].
    let mut planes: [Vec<f32>; 3] = [vec![0.0; pixels], vec![0.0; pixels], vec![0.0; pixels]];
    for (i, px) in rgb16.chunks_exact(3).enumerate() {
        planes[0][i] = px[0] as f32 / 65535.0;
        planes[1][i] = px[1] as f32 / 65535.0;
        planes[2][i] = px[2] as f32 / 65535.0;
    }

    // Analyse automatique des niveaux.
    //
    // Le stack brut d'une session lunaire est **tres sombre** : mesure sur une
    // session reelle de 200 poses, les valeurs utiles vivent vers 0,13-0,41 et
    // l'image ne contient **aucun pixel sature**. Un etirement qui mappe un
    // percentile haut sur 1,0 appliquerait un gain enorme (x3) et **ecraserait
    // les zones brillantes** (les rayons de Tycho) : c'est la cause de l'aplat
    // blanc observe a l'essai.
    //
    // On se contente donc d'un **etirement du blanc seul**, prudent et borne :
    // - point noir : **inchangé** (0). Toucher au point noir assombrit les mers,
    //   ce qui a ete constate comme nuisible : la dynamique basse est bonne
    //   telle quelle dans le stack brut.
    // - blanc : le maximum reel de l'image est ramene vers 0,90, jamais a 1,0.
    //   Les hautes lumieres gardent ainsi tout leur detail.
    let p_max = percentile(&planes[1], 1.0).max(1e-4);
    let auto_black = 0.0f32;
    let auto_white = p_max / 0.90;

    for plane in planes.iter_mut() {
        // 1-2. Normalisation + niveaux + gamma AVANT les ondelettes.
        apply_levels(plane, auto_black, auto_white, s.white_point, s.black_point, s.gamma);
        // 3. Ondelettes : maintenant que l'image occupe toute la plage.
        if s.wavelet_amount > 0.0 {
            wavelet_sharpen(plane, width, height, s.wavelet_amount, s.wavelet_levels);
        }
        // 4. CLAHE.
        if s.clahe_amount > 0.0 {
            clahe(plane, width, height, s.clahe_amount, s.clahe_tiles);
        }
        // 5. Clarte (contraste local).
        if s.clarity > 0.0 {
            clarity(plane, width, height, s.clarity);
        }
    }

    let mut out = vec![0u16; pixels * 3];
    for i in 0..pixels {
        out[i * 3] = to_u16(planes[0][i]);
        out[i * 3 + 1] = to_u16(planes[1][i]);
        out[i * 3 + 2] = to_u16(planes[2][i]);
    }
    out
}

/// Percentile d'un plan (0.0 a 1.0), sur un histogramme de 4096 classes.
fn percentile(img: &[f32], p: f64) -> f32 {
    let mut hist = vec![0u32; 4096];
    for &v in img {
        let b = (v.clamp(0.0, 1.0) * 4095.0) as usize;
        hist[b] += 1;
    }
    let total: u64 = hist.iter().map(|&c| c as u64).sum();
    if total == 0 {
        return 1.0;
    }
    let target = (total as f64 * p) as u64;
    let mut acc = 0u64;
    for (i, &c) in hist.iter().enumerate() {
        acc += c as u64;
        if acc >= target {
            return i as f32 / 4095.0;
        }
    }
    1.0
}

/// Rehaussement par ondelettes a trous (algorithme "a trous" de Starck).
///
/// Decompose l'image en plusieurs echelles par lissage gaussien successif, puis
/// amplifie les coefficients de detail de chaque echelle. C'est la base du
/// traitement Registax : les echelles fines portent les crateres et le grain
/// lunaire, les echelles moyennes les grandes structures.
///
/// Le gain est applique aux coefficients de detail (et non ajoute brutatement) :
/// les coefficients faibles (bruit) sont attenues, les coefficients forts
/// (vrais details) sont renforces. C'est ce qui distingue un rehaussement
/// utile d'une simple amplification de bruit.
fn wavelet_sharpen(img: &mut [f32], width: usize, height: usize, amount: f32, levels: usize) {
    let levels = levels.clamp(1, 6);
    let mut planes = Vec::with_capacity(levels);
    let mut current = img.to_vec();
    for l in 0..levels {
        let blurred = gaussian_blur_br(&current, width, height, 1 << l);
        planes.push(blurred.clone());
        current = blurred;
    }

    // Reconstruction : detail(l) = plane(l-1) - plane(l). On amplifie chaque
    // detail avec un poids qui decroit avec l'echelle (1.0, 0.6, 0.45, ...).
    let mut result = current.clone(); // echelle la plus grossiere : conservee
    for l in 0..levels {
        let fine = if l == 0 { img.to_vec() } else { planes[l - 1].clone() };
        let coarse = &planes[l];
        let w = amount / (1.0 + l as f32 * 0.8);
        for i in 0..result.len() {
            let detail = fine[i] - coarse[i];
            // Gain doux : compresse les petits coefficients (bruit) et pousse
            // les grands (structure). Tanh borne l'amplification.
            let scaled = (detail * w * 8.0).tanh() / 8.0;
            result[i] += detail * w + scaled;
        }
    }

    for i in 0..img.len() {
        img[i] = soft_shoulder(result[i].max(0.0));
    }
}

/// Flou gaussien separable par trois passes de moyenne (approximation rapide),
/// avec un decalage "a trous" (B3-spline dilatee) pour les ondelettes.
fn gaussian_blur_br(img: &[f32], width: usize, height: usize, step: usize) -> Vec<f32> {
    // Noyau B3-spline : [1, 4, 6, 4, 1] / 16, dilate de `step`.
    let kernel = [1.0f32, 4.0, 6.0, 4.0, 1.0];
    let ksum = 16.0f32;
    let kc = 2isize;
    let step = step as isize;

    let mut tmp = vec![0.0f32; img.len()];
    for y in 0..height {
        for x in 0..width {
            let mut acc = 0.0;
            for (k, &kv) in kernel.iter().enumerate() {
                let dx = (k as isize - kc) * step;
                let sx = (x as isize + dx).clamp(0, width as isize - 1) as usize;
                acc += img[y * width + sx] * kv;
            }
            tmp[y * width + x] = acc / ksum;
        }
    }
    let mut out = vec![0.0f32; img.len()];
    for y in 0..height {
        for x in 0..width {
            let mut acc = 0.0;
            for (k, &kv) in kernel.iter().enumerate() {
                let dy = (k as isize - kc) * step;
                let sy = (y as isize + dy).clamp(0, height as isize - 1) as usize;
                acc += tmp[sy * width + x] * kv;
            }
            out[y * width + x] = acc / ksum;
        }
    }
    out
}

/// Etirement des niveaux puis gamma, avec **protection des hautes lumieres**.
///
/// `auto_black`/`auto_white` sont mesures sur l'image (le stack est lineaire et
/// souvent tres sombre). `black_point`/`white_point` sont des reglages
/// additionnels de l'utilisateur (0 par defaut = neutre).
///
/// Le point delicat : une simple mise a l'echelle lineaire suivie d'un `clamp`
/// **ecrase les zones brillantes** (les rayons de Tycho, par exemple) en aplat
/// blanc, en detruisant tout le detail. On applique donc, apres l'etirement, une
/// **courbe de compression douce** des hautes lumieres : les valeurs proches du
/// blanc sont compressees progressivement, de sorte que le detail subsiste
/// (une valeur 0,95 et une valeur 1,0 restent distinctes) au lieu de saturer.
fn apply_levels(
    img: &mut [f32],
    auto_black: f32,
    auto_white: f32,
    white_point: f32,
    black_point: f32,
    gamma: f32,
) {
    let range = (auto_white - auto_black).max(1e-6);
    let inv_gamma = 1.0 / gamma.max(0.05);
    for v in img.iter_mut() {
        // Etirement automatique vers [0, 1] (pas de clamp dur ici).
        let n = (*v - auto_black) / range;
        // Reglages utilisateur : white_point modere l'etirement (0.85 = on
        // pousse le blanc a 85% de la plage). 0 => pas de changement.
        let n = if white_point > 0.0 {
            n / white_point.max(0.1)
        } else {
            n
        };
        let n = if black_point > 0.0 {
            (n - black_point) / (1.0 - black_point).max(1e-6)
        } else {
            n
        };
        let n = n.max(0.0);
        // Gamma.
        let n = n.powf(inv_gamma);
        // Compression douce des hautes lumieres (soft knee) : au lieu d'ecraser
        // a 1.0, on compresse la partie haute de la courbe. Le detail reste
        // present pres du blanc, ou une mise a l'echelle lineaire saturerait.
        *v = soft_shoulder(n);
    }
}

/// Compresse doucement les valeurs au-dela du genou (0,8) vers 1,0.
///
/// En dessous du genou : identite. Au-dessus : la valeur tend vers 1,0 sans
/// jamais l'atteindre brutalement, ce qui evite l'aplat blanc des zones
/// brillantes (rayons crateres, limbe) tout en gardant du contraste.
fn soft_shoulder(x: f32) -> f32 {
    const KNEE: f32 = 0.80;
    if x <= KNEE {
        return x.clamp(0.0, 1.0);
    }
    // Au-dessus du genou, on compresse avec une courbe rationnelle qui reste
    // strictement croissante : x = KNEE -> KNEE, x -> infini -> 1.
    let t = (x - KNEE) / (1.0 - KNEE);
    1.0 - (1.0 - KNEE) * (1.0 / (1.0 + t))
}

/// CLAHE simplifie : egalisation d'histogramme adaptative par blocs, avec
/// interpolation bilineaire entre blocs et limitation du contraste.
fn clahe(img: &mut [f32], width: usize, height: usize, amount: f32, tiles: usize) {
    let tiles = tiles.clamp(2, 16);
    let tile_w = (width + tiles - 1) / tiles;
    let tile_h = (height + tiles - 1) / tiles;
    let bins = 256usize;

    let mut luts: Vec<Vec<f32>> = Vec::with_capacity(tiles * tiles);
    for ty in 0..tiles {
        for tx in 0..tiles {
            let x0 = tx * tile_w;
            let y0 = ty * tile_h;
            let x1 = (x0 + tile_w).min(width);
            let y1 = (y0 + tile_h).min(height);
            if x1 <= x0 || y1 <= y0 {
                luts.push((0..bins).map(|b| b as f32 / 255.0).collect());
                continue;
            }

            let mut hist = vec![0u32; bins];
            let mut n = 0u32;
            for y in y0..y1 {
                for x in x0..x1 {
                    let v = img[y * width + x];
                    let b = (v * 255.0).clamp(0.0, 255.0) as usize;
                    hist[b] += 1;
                    n += 1;
                }
            }
            let clip = ((n as f32 / bins as f32) * (1.0 + amount * 3.0)).max(1.0) as u32;
            let mut excess = 0i64;
            for h in hist.iter_mut() {
                if *h > clip {
                    excess += (*h - clip) as i64;
                    *h = clip;
                }
            }
            let add = excess / bins as i64;
            for h in hist.iter_mut() {
                *h = (*h as i64 + add).max(0) as u32;
            }

            let total: u32 = hist.iter().sum();
            let mut lut = vec![0.0f32; bins];
            let mut cum = 0u32;
            for b in 0..bins {
                cum += hist[b];
                let eq = if total > 0 { cum as f32 / total as f32 } else { b as f32 / 255.0 };
                let identity = b as f32 / 255.0;
                lut[b] = identity + (eq - identity) * amount.clamp(0.0, 1.0);
            }
            luts.push(lut);
        }
    }

    let orig = img.to_vec();
    for y in 0..height {
        let fy = (y as f32 / tile_h as f32 - 0.5).clamp(0.0, (tiles - 1) as f32);
        let ty0 = fy.floor() as usize;
        let ty1 = (ty0 + 1).min(tiles - 1);
        let wy = fy - ty0 as f32;
        for x in 0..width {
            let fx = (x as f32 / tile_w as f32 - 0.5).clamp(0.0, (tiles - 1) as f32);
            let tx0 = fx.floor() as usize;
            let tx1 = (tx0 + 1).min(tiles - 1);
            let wx = fx - tx0 as f32;

            let v = orig[y * width + x];
            let b = (v * 255.0).clamp(0.0, 255.0) as usize;

            let l00 = luts[ty0 * tiles + tx0][b];
            let l01 = luts[ty0 * tiles + tx1][b];
            let l10 = luts[ty1 * tiles + tx0][b];
            let l11 = luts[ty1 * tiles + tx1][b];

            let top = l00 * (1.0 - wx) + l01 * wx;
            let bot = l10 * (1.0 - wx) + l11 * wx;
            let val = top * (1.0 - wy) + bot * wy;
            img[y * width + x] = val.clamp(0.0, 1.0);
        }
    }
}

/// Clarte : accentuation du contraste local via un passe-haut large.
///
/// La sortie passe par `soft_shoulder` pour ne pas recreer d'aplat blanc dans
/// les zones brillantes.
fn clarity(img: &mut [f32], width: usize, height: usize, amount: f32) {
    let blurred = gaussian_blur_br(img, width, height, 4);
    for i in 0..img.len() {
        let detail = img[i] - blurred[i];
        img[i] = soft_shoulder((img[i] + detail * amount * 2.0).max(0.0));
    }
}

fn to_u16(v: f32) -> u16 {
    (v.clamp(0.0, 1.0) * 65535.0).round() as u16
}
