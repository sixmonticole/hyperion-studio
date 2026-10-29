//! Alignement par similarite (translation + rotation + echelle).
//!
//! L'alignement par translation seule ne corrige pas la **rotation de champ** :
//! sur un trepied fixe, la Lune tourne dans le capteur au fil de la session.
//! Empiler des frames qui ont tourne moyenne les zones eloignees du centre de
//! rotation et rend l'image floue.
//!
//! Methode : appariement de points de controle.
//! 1. Coins de Harris detectes sur la reference, repartis sur une grille.
//! 2. Suivi de chaque coin dans la frame cible par correlation locale.
//! 3. Appariements aberrants rejetes par RANSAC, avec un modele de similarite
//!    (translation + rotation + echelle).
//! 4. Transformation appliquee par echantillonnage bicubique.
//!
//! La Lune est riche en cratères (coins bien detectables), ce qui rend cette
//! approche fiable. Pour une planete lisse, il faudrait la correlation polaire.

/// Taille de la fenetre de suivi d'un point (patch), en pixels.
const PATCH: usize = 41;
/// Rayon maximal de recherche du deplacement d'un point, en pixels.
const SEARCH: usize = 48;

/// Transformation de similarite estimee.
#[derive(Debug, Clone, Copy)]
pub struct Similarity {
    /// Rotation en radians.
    pub angle: f64,
    /// Echelle (proche de 1 en pratique).
    pub scale: f64,
    /// Translation en pixels.
    pub tx: f64,
    pub ty: f64,
}

impl Similarity {
    pub fn identity() -> Self {
        Self {
            angle: 0.0,
            scale: 1.0,
            tx: 0.0,
            ty: 0.0,
        }
    }

    /// Point de la reference correspondant a un point de la frame.
    /// Modele : reference = s * R * frame + t (coordonnees centrees en (cx, cy)).
    pub fn frame_to_ref(&self, x: f64, y: f64, cx: f64, cy: f64) -> (f64, f64) {
        let cos = self.angle.cos();
        let sin = self.angle.sin();
        let dx = x - cx;
        let dy = y - cy;
        let rx = self.scale * (cos * dx - sin * dy);
        let ry = self.scale * (sin * dx + cos * dy);
        (rx + cx + self.tx, ry + cy + self.ty)
    }

    /// Coordonnee source (dans la frame) pour un pixel cible de la reference.
    /// Inverse du modele : frame = R^-1 * (reference - t) / s.
    pub fn source_of(&self, x: f64, y: f64, cx: f64, cy: f64) -> (f64, f64) {
        let cos = self.angle.cos();
        let sin = self.angle.sin();
        let dx = x - cx - self.tx;
        let dy = y - cy - self.ty;
        let sx = (cos * dx + sin * dy) / self.scale;
        let sy = (-sin * dx + cos * dy) / self.scale;
        (sx + cx, sy + cy)
    }

    pub fn angle_deg(&self) -> f64 {
        self.angle.to_degrees()
    }
}

/// Point de controle apparie entre la reference et une frame.
#[derive(Debug, Clone, Copy)]
struct Pair {
    rx: f64,
    ry: f64,
    fx: f64,
    fy: f64,
}

/// Aligneur par similarite : garde les coins de la reference en cache.
pub struct SimilarityAligner {
    width: usize,
    height: usize,
    refs: Vec<RefPoint>,
}

struct RefPoint {
    x: usize,
    y: usize,
    patch: Vec<f32>,
    energy: f64,
}

impl SimilarityAligner {
    /// Construit l'aligneur a partir de la luminance de la reference.
    pub fn new(reference_lum: &[f32], width: usize, height: usize, max_points: usize) -> Self {
        // Normalisation de la luminance : les seuils de Harris sont exprimes en
        // valeurs relatives, il faut donc une plage stable quelle que soit
        // l'exposition de la session.
        let (lo, hi) = min_max(reference_lum);
        let range = (hi - lo).max(1e-6);
        let norm: Vec<f32> = reference_lum.iter().map(|&v| (v - lo) / range).collect();

        let corners = harris_corners(&norm, width, height);

        // Repartition : un meilleur coin par cellule d'une grille, pour ne pas
        // concentrer tous les points sur une seule zone tres texturee.
        let grid = ((max_points as f64).sqrt().ceil() as usize).max(2);
        let cell_w = (width / grid).max(1);
        let cell_h = (height / grid).max(1);
        let mut best_per_cell: std::collections::HashMap<(usize, usize), (f64, usize)> =
            Default::default();
        for (i, &(x, y, score)) in corners.iter().enumerate() {
            let cell = ((x / cell_w).min(grid - 1), (y / cell_h).min(grid - 1));
            let e = best_per_cell.entry(cell).or_insert((f64::NEG_INFINITY, i));
            if score > e.0 {
                *e = (score, i);
            }
        }

        let mut refs = Vec::new();
        for (_, (_, i)) in best_per_cell {
            let (x, y, _) = corners[i];
            if x < PATCH / 2 || y < PATCH / 2 || x + PATCH / 2 >= width || y + PATCH / 2 >= height {
                continue;
            }
            let patch = extract_patch(&norm, width, height, x, y);
            // Energie du patch, en luminance normalisee : sert a ecarter les
            // zones quasi uniformes (fond de ciel) ou le suivi serait ambigu.
            let energy = patch.iter().map(|&v| (v * v) as f64).sum::<f64>() / patch.len() as f64;
            if energy > 1e-5 {
                refs.push(RefPoint {
                    x,
                    y,
                    patch,
                    energy,
                });
            }
        }

        Self {
            width,
            height,
            refs,
        }
    }

    pub fn point_count(&self) -> usize {
        self.refs.len()
    }

    /// Estime la similarite entre la reference et `frame_lum`.
    pub fn estimate(&self, frame_lum: &[f32]) -> Option<Similarity> {
        // Normalisation identique a celle de la reference, pour que les
        // gradients et le NCC soient comparables d'une session a l'autre.
        let (lo, hi) = min_max(frame_lum);
        let range = (hi - lo).max(1e-6);
        let norm: Vec<f32> = frame_lum.iter().map(|&v| (v - lo) / range).collect();

        let mut pairs: Vec<Pair> = Vec::new();

        for r in &self.refs {
            if let Some((dx, dy)) = self.match_point(&norm, r) {
                pairs.push(Pair {
                    rx: r.x as f64,
                    ry: r.y as f64,
                    fx: (r.x as f64 + dx).clamp(0.0, (self.width - 1) as f64),
                    fy: (r.y as f64 + dy).clamp(0.0, (self.height - 1) as f64),
                });
            }
        }

        if pairs.len() < 6 {
            return None;
        }

        ransac_similarity(&pairs, self.width as f64 / 2.0, self.height as f64 / 2.0)
    }

    /// Suit un point de reference dans la frame par correlation locale.
    ///
    /// Recherche le deplacement (dx, dy) dans un rayon de `SEARCH` pixels qui
    /// maximise la correlation croisee normalisee entre le patch de reference et
    /// la fenetre de la frame. Renvoie un deplacement sub-pixel.
    fn match_point(&self, frame_lum: &[f32], r: &RefPoint) -> Option<(f64, f64)> {
        let half = PATCH / 2;
        let mut best = (0i32, 0i32, f64::NEG_INFINITY);

        // Pre-calcul de la moyenne et de l'ecart-type du patch de reference.
        let (rm, rs) = mean_std(&r.patch);
        if rs < 1e-6 || r.energy < 1e-6 {
            return None;
        }

        for dy in -(SEARCH as i32)..=(SEARCH as i32) {
            let cy = r.y as i32 + dy;
            if cy - half as i32 - 1 < 0 || cy + half as i32 + 1 >= self.height as i32 {
                continue;
            }
            for dx in -(SEARCH as i32)..=(SEARCH as i32) {
                let cx = r.x as i32 + dx;
                if cx - half as i32 - 1 < 0 || cx + half as i32 + 1 >= self.width as i32 {
                    continue;
                }
                let score = ncc(frame_lum, self.width, cx as usize, cy as usize, &r.patch, rm, rs);
                if score > best.2 {
                    best = (dx, dy, score);
                }
            }
        }

        if best.2 < 0.5 {
            return None; // correlation trop faible : appariement douteux
        }

        // Affinage sub-pixel par parabole dans les deux directions.
        let (bx, by, _) = best;
        let s = |dx: i32, dy: i32| -> f64 {
            let cx = r.x as i32 + dx;
            let cy = r.y as i32 + dy;
            if cx - half as i32 - 1 < 0
                || cy - half as i32 - 1 < 0
                || cx + half as i32 + 1 >= self.width as i32
                || cy + half as i32 + 1 >= self.height as i32
            {
                return f64::NEG_INFINITY;
            }
            ncc(frame_lum, self.width, cx as usize, cy as usize, &r.patch, rm, rs)
        };
        let sx = s(bx - 1, by);
        let s0 = s(bx, by);
        let sx1 = s(bx + 1, by);
        let sy = s(bx, by - 1);
        let sy1 = s(bx, by + 1);
        let fx = subpix(sx, s0, sx1);
        let fy = subpix(sy, s0, sy1);

        Some((bx as f64 + fx, by as f64 + fy))
    }
}

/// Correlation croisee normalisee entre le patch et une fenetre centree en (cx, cy).
fn ncc(
    frame: &[f32],
    width: usize,
    cx: usize,
    cy: usize,
    patch: &[f32],
    rm: f64,
    rs: f64,
) -> f64 {
    let half = PATCH / 2;
    let ox = cx - half;
    let oy = cy - half;

    let mut sum = 0.0f64;
    let mut sum_sq = 0.0f64;
    let mut dot = 0.0f64;
    for py in 0..PATCH {
        for px in 0..PATCH {
            let v = frame[(oy + py) * width + (ox + px)] as f64;
            sum += v;
            sum_sq += v * v;
            dot += v * patch[py * PATCH + px] as f64;
        }
    }
    let n = (PATCH * PATCH) as f64;
    let m = sum / n;
    let var = (sum_sq / n - m * m).max(0.0);
    let sd = var.sqrt();
    if sd < 1e-6 {
        return -1.0;
    }
    // NCC = E[(a-ma)(b-mb)] / (sa sb), le terme en mb disparait car le patch est
    // de moyenne nulle (fenetre de Hann appliquee mais moyenne non nulle : on
    // soustrait rm explicitement).
    (dot - rm * sum) / (n * rs * sd)
}

/// Position du sommet de la parabole passant par (-1, a), (0, b), (1, c).
fn subpix(a: f64, b: f64, c: f64) -> f64 {
    if !a.is_finite() || !b.is_finite() || !c.is_finite() {
        return 0.0;
    }
    let denom = a - 2.0 * b + c;
    if denom.abs() < 1e-12 {
        0.0
    } else {
        (0.5 * (a - c) / denom).clamp(-0.5, 0.5)
    }
}

fn mean_std(v: &[f32]) -> (f64, f64) {
    let n = v.len() as f64;
    let sum: f64 = v.iter().map(|&x| x as f64).sum();
    let sum_sq: f64 = v.iter().map(|&x| (x as f64) * (x as f64)).sum();
    let m = sum / n;
    let var = (sum_sq / n - m * m).max(0.0);
    (m, var.sqrt())
}

/// Applique une transformation de similarite a une image RGB 16-bit.
///
/// Pour chaque pixel cible (dans la reference), on calcule la coordonnee source
/// dans la frame, puis on echantillonne en bicubique. Une seule passe : pas de
/// double reechantillonnage qui adoucirait l'image.
pub fn warp_similarity(
    src: &[u16],
    width: usize,
    height: usize,
    t: &Similarity,
) -> Vec<u16> {
    let cx = width as f64 / 2.0;
    let cy = height as f64 / 2.0;
    let mut out = vec![0u16; src.len()];

    // Cas identite : aucune transformation, on renvoie la source telle quelle
    // (evite un reechantillonnage inutile qui lisserait la reference).
    if t.angle.abs() < 1e-6 && (t.scale - 1.0).abs() < 1e-6 && t.tx.abs() < 0.01 && t.ty.abs() < 0.01 {
        out.copy_from_slice(src);
        return out;
    }

    for y in 0..height {
        for x in 0..width {
            let (sx, sy) = t.source_of(x as f64, y as f64, cx, cy);
            let (r, g, b) = sample_bicubic(src, width, height, sx, sy);
            let di = (y * width + x) * 3;
            out[di] = r;
            out[di + 1] = g;
            out[di + 2] = b;
        }
    }
    out
}

/// Applique une transformation de similarite a une image RGB 16-bit, en
/// renvoyant aussi un masque de validite (1 = pixel echantillonne dans la frame).
///
/// Pour chaque pixel cible (dans la reference), on calcule la coordonnee source
/// dans la frame, puis on echantillonne en bicubique. Une seule passe : pas de
/// double reechantillonnage qui adoucirait l'image.
pub fn warp_similarity_masked(
    src: &[u16],
    width: usize,
    height: usize,
    t: &Similarity,
) -> (Vec<u16>, Vec<u8>) {
    let cx = width as f64 / 2.0;
    let cy = height as f64 / 2.0;
    let mut out = vec![0u16; src.len()];
    let mut mask = vec![0u8; width * height];

    // Cas identite : aucune transformation, on renvoie la source telle quelle
    // (evite un reechantillonnage inutile qui lisserait la reference).
    if t.angle.abs() < 1e-6
        && (t.scale - 1.0).abs() < 1e-6
        && t.tx.abs() < 0.01
        && t.ty.abs() < 0.01
    {
        out.copy_from_slice(src);
        mask.fill(1);
        return (out, mask);
    }

    for y in 0..height {
        for x in 0..width {
            let (sx, sy) = t.source_of(x as f64, y as f64, cx, cy);
            if sx < 1.0 || sy < 1.0 || sx > (width - 2) as f64 || sy > (height - 2) as f64 {
                continue;
            }
            let (r, g, b) = sample_bicubic(src, width, height, sx, sy);
            let di = (y * width + x) * 3;
            out[di] = r;
            out[di + 1] = g;
            out[di + 2] = b;
            mask[y * width + x] = 1;
        }
    }
    (out, mask)
}

/// Echantillonnage bicubique (Catmull-Rom) d'une image RGB 16-bit.
fn sample_bicubic(src: &[u16], width: usize, height: usize, x: f64, y: f64) -> (u16, u16, u16) {
    if x < -1.0 || y < -1.0 || x > width as f64 || y > height as f64 {
        return (0, 0, 0);
    }
    let x = x.clamp(0.0, (width - 1) as f64);
    let y = y.clamp(0.0, (height - 1) as f64);
    let x0 = x.floor() as isize;
    let y0 = y.floor() as isize;
    let fx = x - x0 as f64;
    let fy = y - y0 as f64;

    let at = |px: isize, py: isize, c: usize| -> f64 {
        if px < 0 || py < 0 || px >= width as isize || py >= height as isize {
            return 0.0;
        }
        src[(py as usize * width + px as usize) * 3 + c] as f64
    };

    let wx = catmull(fx);
    let wy = catmull(fy);
    let mut out = [0u16; 3];
    for c in 0..3 {
        let mut acc = 0.0;
        for (j, &vy) in wy.iter().enumerate() {
            let mut row = 0.0;
            for (i, &wv) in wx.iter().enumerate() {
                row += at(x0 - 1 + i as isize, y0 - 1 + j as isize, c) * wv;
            }
            acc += row * vy;
        }
        out[c] = acc.clamp(0.0, 65535.0).round() as u16;
    }
    (out[0], out[1], out[2])
}

fn catmull(t: f64) -> [f64; 4] {
    let t2 = t * t;
    let t3 = t2 * t;
    [
        -0.5 * t3 + t2 - 0.5 * t,
        1.5 * t3 - 2.5 * t2 + 1.0,
        -1.5 * t3 + 2.0 * t2 + 0.5 * t,
        0.5 * t3 - 0.5 * t2,
    ]
}

/// Detection de coins de Harris.
///
/// Point cle : le tenseur de structure doit etre **somme sur un voisinage**.
/// Calcule sur un seul pixel, le tenseur est de rang 1 et son determinant est
/// toujours nul, ce qui empeche toute detection de coin (piege classique).
fn harris_corners(lum: &[f32], width: usize, height: usize) -> Vec<(usize, usize, f64)> {
    let mut corners = Vec::new();
    let margin = 3usize;
    for y in margin..height - margin {
        for x in margin..width - margin {
            // Tenseur de structure somme sur une fenetre 5x5.
            let (mut sxx, mut syy, mut sxy) = (0.0f64, 0.0f64, 0.0f64);
            for dy in -2i64..=2 {
                for dx in -2i64..=2 {
                    let px = (x as i64 + dx) as usize;
                    let py = (y as i64 + dy) as usize;
                    // Acces voisins bornes (les boucles garantissent la marge).
                    let i = py * width + px;
                    let gx = (lum[i + 1] - lum[i - 1]) as f64;
                    let gy = (lum[i + width] - lum[i - width]) as f64;
                    sxx += gx * gx;
                    syy += gy * gy;
                    sxy += gx * gy;
                }
            }
            let det = sxx * syy - sxy * sxy;
            let trace = sxx + syy;
            let r = det - 0.04 * trace * trace;
            if r > 1e-9 {
                corners.push((x, y, r));
            }
        }
    }
    corners
}

/// Bornes (min, max) d'un tableau.
fn min_max(v: &[f32]) -> (f32, f32) {
    let mut lo = f32::INFINITY;
    let mut hi = f32::NEG_INFINITY;
    for &x in v {
        if x < lo {
            lo = x;
        }
        if x > hi {
            hi = x;
        }
    }
    if lo.is_finite() && hi.is_finite() {
        (lo, hi)
    } else {
        (0.0, 1.0)
    }
}

/// Extrait un patch de luminance centre en (x, y), avec fenetre de Hann.
fn extract_patch(lum: &[f32], width: usize, height: usize, x: usize, y: usize) -> Vec<f32> {
    let half = PATCH / 2;
    let mut out = vec![0.0f32; PATCH * PATCH];
    for py in 0..PATCH {
        let sy = (y + py).saturating_sub(half).min(height - 1);
        let wy = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * py as f32 / (PATCH - 1) as f32).cos();
        for px in 0..PATCH {
            let sx = (x + px).saturating_sub(half).min(width - 1);
            let wx = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * px as f32 / (PATCH - 1) as f32).cos();
            out[py * PATCH + px] = lum[sy * width + sx] * wx * wy;
        }
    }
    out
}

/// Ajuste une similarite par RANSAC sur les appariements de points.
fn ransac_similarity(pairs: &[Pair], cx: f64, cy: f64) -> Option<Similarity> {
    let iters = 400;
    let threshold = 3.0f64;
    let mut best_inliers: Vec<usize> = Vec::new();

    let mut seed = 0x1234_5678_9abc_def0u64;
    let mut next = || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((seed >> 33) as f64) / ((1u64 << 31) as f64)
    };

    for _ in 0..iters {
        let i1 = ((next() * pairs.len() as f64) as usize) % pairs.len();
        let i2 = ((next() * pairs.len() as f64) as usize) % pairs.len();
        if i1 == i2 {
            continue;
        }
        let Some(t) = similarity_from_two(pairs[i1], pairs[i2], cx, cy) else {
            continue;
        };
        // Modele : reference = s*R*frame + t. On mesure l'erreur en projetant le
        // point frame et en comparant a son correspondant reference.
        let inliers: Vec<usize> = pairs
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                let (px, py) = t.frame_to_ref(p.fx, p.fy, cx, cy);
                ((px - p.rx).powi(2) + (py - p.ry).powi(2)).sqrt() < threshold
            })
            .map(|(i, _)| i)
            .collect();
        if inliers.len() > best_inliers.len() {
            best_inliers = inliers;
        }
    }

    if best_inliers.len() < 6 {
        return None;
    }

    let inlier_pairs: Vec<Pair> = best_inliers.iter().map(|&i| pairs[i]).collect();
    least_squares_similarity(&inlier_pairs, cx, cy)
}

/// Similarite exacte passant par deux appariements (avant -> reference).
fn similarity_from_two(p1: Pair, p2: Pair, cx: f64, cy: f64) -> Option<Similarity> {
    // Vecteurs frame et reference (centres).
    let (afx, afy) = (p1.fx - cx, p1.fy - cy);
    let (bfx, bfy) = (p2.fx - cx, p2.fy - cy);
    let (arx, ary) = (p1.rx - cx, p1.ry - cy);
    let (brx, bry) = (p2.rx - cx, p2.ry - cy);

    // Vecteur frame (b - a) et reference (b - a).
    let (dfx, dfy) = (bfx - afx, bfy - afy);
    let (drx, dry) = (brx - arx, bry - ary);
    let dfnorm2 = dfx * dfx + dfy * dfy;
    let drnorm2 = drx * drx + dry * dry;
    if dfnorm2 < 1e-6 || drnorm2 < 1e-6 {
        return None;
    }

    // Rotation + echelle : reference = s * R * frame (a->b).
    let scale = (drnorm2 / dfnorm2).sqrt();
    let angle = (dfx * dry - dfy * drx).atan2(dfx * drx + dfy * dry);

    // Translation : reference - transformee(frame), moyennee sur les 2 points.
    let rot = |x: f64, y: f64| -> (f64, f64) {
        let c = angle.cos();
        let s = angle.sin();
        (scale * (c * x - s * y), scale * (s * x + c * y))
    };
    let (r1x, r1y) = rot(afx, afy);
    let (r2x, r2y) = rot(bfx, bfy);
    // reference_c = rotation(frame_c) + t  =>  t = reference_c - rotation(frame_c)
    let tx = ((arx - r1x) + (brx - r2x)) / 2.0;
    let ty = ((ary - r1y) + (bry - r2y)) / 2.0;

    Some(Similarity {
        angle,
        scale,
        tx,
        ty,
    })
}

/// Raffinement par moindres carres sur les inliers.
fn least_squares_similarity(pairs: &[Pair], cx: f64, cy: f64) -> Option<Similarity> {
    let n = pairs.len() as f64;
    let sum_r2: f64 = pairs
        .iter()
        .map(|p| (p.fx - cx).powi(2) + (p.fy - cy).powi(2))
        .sum();
    if sum_r2 < 1e-6 {
        return None;
    }

    // reference = s*R*frame + t, centre.
    let sum_dot: f64 = pairs
        .iter()
        .map(|p| (p.fx - cx) * (p.rx - cx) + (p.fy - cy) * (p.ry - cy))
        .sum();
    let sum_cross: f64 = pairs
        .iter()
        .map(|p| (p.fx - cx) * (p.ry - cy) - (p.fy - cy) * (p.rx - cx))
        .sum();

    // a = s*cos, b = s*sin
    let a = sum_dot / sum_r2;
    let b = sum_cross / sum_r2;
    let scale = (a * a + b * b).sqrt();
    let angle = b.atan2(a);
    if !angle.is_finite() || !scale.is_finite() || scale <= 0.0 {
        return None;
    }

    let mut tx = 0.0;
    let mut ty = 0.0;
    for p in pairs {
        let x = p.fx - cx;
        let y = p.fy - cy;
        let rx = a * x - b * y;
        let ry = b * x + a * y;
        tx += (cx + rx) - p.rx;
        ty += (cy + ry) - p.ry;
    }
    tx /= n;
    ty /= n;

    Some(Similarity {
        angle,
        scale,
        tx,
        ty,
    })
}
