//! Alignement a points locaux (AP, "alignment points").
//!
//! L'alignement global (une seule translation par frame) ne corrige pas la
//! distorsion introduite par l'atmosphere : la turbulence deforme l'image
//! localement. AutoStakkert corrige cela en placant une grille de petits points
//! d'alignement, en estimant un deplacement **par point**, puis en deformant la
//! frame pour ramener chaque zone sur la reference.
//!
//! Implementation :
//! 1. Grille de points AP repartis sur la fenetre de crop, espaces regulierement.
//! 2. Pour chaque point, une petite vignette (patch) de la reference est mise de
//!    cote, et on estime par correlation de phase le deplacement local de la
//!    frame dans cette vignette.
//! 3. Les deplacements sont interpoles bilineairement sur toute l'image, et la
//!    frame est re-echantillonnee (interpolation bilineaire) avec ce champ.
//! 4. Un score de fiabilite par point permet d'ignorer les zones plates (mer,
//!    fond de ciel) ou la correlation est peu fiable.

use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlanner};
use std::sync::Arc;

/// Taille d'une vignette d'alignement (patch), en pixels.
const PATCH: usize = 64;

/// Un point d'alignement : position dans la fenetre + deplacement estime.
#[derive(Debug, Clone, Copy)]
struct Anchor {
    x: usize,
    y: usize,
    dx: f64,
    dy: f64,
}

/// Aligneur a points locaux : garde les vignettes de reference en cache.
pub struct ApAligner {
    width: usize,
    height: usize,
    patch: usize,
    /// Grille d'ancres avec leur vignette de reference (FFT pre-calculee).
    anchors: Vec<AnchorRef>,
    /// FFT partagee pour les vignettes de taille patch x patch.
    fft: Arc<dyn Fft<f32>>,
    ifft: Arc<dyn Fft<f32>>,
}

struct AnchorRef {
    x: usize,
    y: usize,
    /// Spectre de la vignette de reference.
    spectrum: Vec<Complex32>,
    /// Energie de la vignette (pour ecarter les zones plates).
    energy: f64,
}

impl ApAligner {
    /// Construit l'aligneur a partir de la luminance de la frame de reference.
    ///
    /// `final_shift` : deplacement global deja estime, applique pour que les
    /// vignettes de reference soient prises sur la reference recalee.
    pub fn new(reference_lum: &[f32], width: usize, height: usize, step: usize) -> Self {
        let patch = PATCH.min(width).min(height);
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(patch * patch);
        let ifft = planner.plan_fft_inverse(patch * patch);

        // Grille d'ancres. Le pas est au moins `step`, mais aussi elargi pour
        // borner le nombre total de points : au dela de ~400 points, le gain de
        // qualite est negligeable et le cout grimpe.
        let half = patch / 2;
        let base_step = step.max(patch / 2).max(8);
        let max_anchors = 400usize;
        let mut step = base_step;
        loop {
            let nx = (width.saturating_sub(patch)) / step + 1;
            let ny = (height.saturating_sub(patch)) / step + 1;
            if nx * ny <= max_anchors || step >= width.max(height) {
                break;
            }
            step = (step as f64 * 1.25) as usize + 1;
        }
        let mut anchors = Vec::new();
        let mut y = half;
        while y + half < height {
            let mut x = half;
            while x + half < width {
                let (spec, energy) = patch_spectrum(reference_lum, width, height, x, y, patch, &fft);
                anchors.push(AnchorRef {
                    x,
                    y,
                    spectrum: spec,
                    energy,
                });
                x += step;
            }
            y += step;
        }

        Self {
            width,
            height,
            patch,
            anchors,
            fft,
            ifft,
        }
    }

    /// Nombre de points d'alignement places.
    pub fn anchor_count(&self) -> usize {
        self.anchors.len()
    }

    /// Estime le champ de deplacement local de la frame, puis renvoie l'image
    /// deformee pour la ramener sur la reference.
    ///
    /// Renvoie `None` s'il n'y a pas assez d'ancres fiables (repli sur l'alignement
    /// global cote appelant).
    pub fn align_frame(
        &self,
        lum: &[f32],
        rgb: &[u16],
    ) -> Option<Vec<u16>> {
        let mut anchors: Vec<Anchor> = Vec::with_capacity(self.anchors.len());

        for a in &self.anchors {
            // Vignette de la frame a la meme position.
            let mut buf: Vec<Complex32> = (0..self.patch * self.patch)
                .map(|i| {
                    let px = i % self.patch;
                    let py = i / self.patch;
                    let sx = (a.x + px).min(self.width - 1);
                    let sy = (a.y + py).min(self.height - 1);
                    Complex32::new(lum[sy * self.width + sx], 0.0)
                })
                .collect();

            // Correlation croisee normalisee avec la reference.
            self.fft.process(&mut buf);
            for i in 0..buf.len() {
                let cross = buf[i] * a.spectrum[i].conj();
                let mag = cross.norm();
                buf[i] = if mag > 1e-6 { cross / mag } else { Complex32::new(0.0, 0.0) };
            }
            self.ifft.process(&mut buf);

            // Pic de correlation et fiabilite (pic / energie du patch).
            let (mut best_i, mut best_v) = (0usize, f32::NEG_INFINITY);
            for (i, c) in buf.iter().enumerate() {
                if c.re > best_v {
                    best_v = c.re;
                    best_i = i;
                }
            }

            // Estimation sub-pixel : ajustement parabolique autour du pic, dans
            // les deux directions. Sans cela, les deplacements sont entiers et
            // l'alignement local ne gagne rien sur le global.
            let px = best_i % self.patch;
            let py = best_i / self.patch;
            let (sub_x, sub_y) = subpixel_peak(&buf, self.patch, px, py);

            let p = self.patch as i64;
            let wrap = |v: f64, n: i64| -> f64 {
                if v > n as f64 / 2.0 {
                    v - n as f64
                } else {
                    v
                }
            };
            let dx = wrap(px as f64 + sub_x, p);
            let dy = wrap(py as f64 + sub_y, p);

            // Fiabilite : un pic net se detache de la moyenne. Sur une zone
            // plate le pic est noye, on ignore alors ce point.
            let mean: f32 = buf.iter().map(|c| c.re).sum::<f32>() / buf.len() as f32;
            let reliable = a.energy > 1.0 && (best_v - mean) as f64 > 0.15 * a.energy;

            if reliable {
                anchors.push(Anchor {
                    x: a.x,
                    y: a.y,
                    dx,
                    dy,
                });
            }
        }

        if anchors.len() < 4 {
            return None;
        }

        // Deformation : interpolation bilineaire des deplacements entre ancres.
        Some(warp_rgb16(rgb, self.width, self.height, &anchors))
    }
}

/// Calcule le spectre d'une vignette centree en (x, y) avec fenetre de Hann.
fn patch_spectrum(
    lum: &[f32],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    patch: usize,
    fft: &Arc<dyn Fft<f32>>,
) -> (Vec<Complex32>, f64) {
    let half = patch / 2;
    let mut buf: Vec<Complex32> = Vec::with_capacity(patch * patch);
    let mut energy = 0.0f64;

    for py in 0..patch {
        let wy = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * py as f32 / (patch - 1).max(1) as f32).cos();
        for px in 0..patch {
            let wx = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * px as f32 / (patch - 1).max(1) as f32).cos();
            let sx = (x + px).saturating_sub(half).min(width - 1);
            let sy = (y + py).saturating_sub(half).min(height - 1);
            let v = lum[sy * width + sx];
            energy += (v * v) as f64;
            buf.push(Complex32::new(v * wx * wy, 0.0));
        }
    }

    fft.process(&mut buf);
    (buf, energy / (patch * patch) as f64)
}

/// Ajustement parabolique du pic de correlation, pour une estimation sub-pixel.
///
/// Renvoie le decalage fractionnaire (dx, dy) dans [-0.5, 0.5] par rapport au
/// bin entier du pic.
fn subpixel_peak(buf: &[Complex32], patch: usize, px: usize, py: usize) -> (f64, f64) {
    let at = |x: isize, y: isize| -> f64 {
        // Repli torique (le domaine de correlation est periodique).
        let xx = ((x + patch as isize) % patch as isize) as usize;
        let yy = ((y + patch as isize) % patch as isize) as usize;
        buf[yy * patch + xx].re as f64
    };

    let xf = subpixel_1d(
        at(px as isize - 1, py as isize),
        at(px as isize, py as isize),
        at(px as isize + 1, py as isize),
    );
    let yf = subpixel_1d(
        at(px as isize, py as isize - 1),
        at(px as isize, py as isize),
        at(px as isize, py as isize + 1),
    );
    (xf, yf)
}

/// Position du sommet d'une parabole passant par (-1, a), (0, b), (1, c).
fn subpixel_1d(a: f64, b: f64, c: f64) -> f64 {
    let denom = a - 2.0 * b + c;
    if denom.abs() < 1e-12 {
        0.0
    } else {
        (0.5 * (a - c) / denom).clamp(-0.5, 0.5)
    }
}

/// Deforme une image RGB 16-bit selon le champ de deplacement des ancres.
///
/// Le champ est d'abord rasterise sur une grille reguliere (une valeur de
/// deplacement par cellule), puis la deformation lit ce champ par interpolation
/// bilineaire. Complexite O(pixels + ancres), et non O(pixels x ancres).
fn warp_rgb16(src: &[u16], width: usize, height: usize, anchors: &[Anchor]) -> Vec<u16> {
    let field = ShiftField::build(anchors, width, height);
    let mut out = vec![0u16; src.len()];

    for y in 0..height {
        for x in 0..width {
            let (dx, dy) = field.sample(x as f64, y as f64);
            let sx = x as f64 - dx;
            let sy = y as f64 - dy;
            let (r, g, b) = sample_bilinear(src, width, height, sx, sy);
            let di = (y * width + x) * 3;
            out[di] = r;
            out[di + 1] = g;
            out[di + 2] = b;
        }
    }
    out
}

/// Champ de deplacement rasterise sur une grille grossiere.
///
/// On stocke un (dx, dy) par cellule de la grille ; la resolution de la grille
/// est choisie pour rester fine devant les variations de la turbulence, tout en
/// gardant l'echantillonnage tres rapide.
struct ShiftField {
    cols: usize,
    rows: usize,
    cell: usize,
    #[allow(dead_code)]
    width: usize,
    #[allow(dead_code)]
    height: usize,
    dx: Vec<f32>,
    dy: Vec<f32>,
}

impl ShiftField {
    fn build(anchors: &[Anchor], width: usize, height: usize) -> Self {
        // Cellule d'environ 32 px : le champ de turbulence est lisse a cette
        // echelle, une grille plus fine n'apporte rien.
        let cell = 32usize;
        let cols = (width + cell - 1) / cell;
        let rows = (height + cell - 1) / cell;

        // Accumulation des ancres dans leur cellule (somme + compteur).
        let mut sx = vec![0.0f32; cols * rows];
        let mut sy = vec![0.0f32; cols * rows];
        let mut n = vec![0.0f32; cols * rows];

        for a in anchors {
            let cx = (a.x / cell).min(cols - 1);
            let cy = (a.y / cell).min(rows - 1);
            let i = cy * cols + cx;
            sx[i] += a.dx as f32;
            sy[i] += a.dy as f32;
            n[i] += 1.0;
        }

        // Moyenne par cellule. Les cellules vides sont remplies plus tard par
        // propagation des voisines non vides (plus proche voisin).
        let mut dx = vec![0.0f32; cols * rows];
        let mut dy = vec![0.0f32; cols * rows];
        let mut filled = vec![false; cols * rows];

        for i in 0..cols * rows {
            if n[i] > 0.0 {
                dx[i] = sx[i] / n[i];
                dy[i] = sy[i] / n[i];
                filled[i] = true;
            }
        }

        // Propagation en anneaux successifs depuis les cellules remplies.
        let mut frontier: Vec<usize> = (0..cols * rows).filter(|&i| filled[i]).collect();
        let mut all_filled = frontier.len() == cols * rows;
        while !all_filled {
            let mut next = Vec::new();
            for &i in &frontier {
                let cx = i % cols;
                let cy = i / cols;
                for (ox, oy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                    let nx = cx as i64 + ox;
                    let ny = cy as i64 + oy;
                    if nx < 0 || ny < 0 || nx >= cols as i64 || ny >= rows as i64 {
                        continue;
                    }
                    let j = ny as usize * cols + nx as usize;
                    if !filled[j] {
                        dx[j] = dx[i];
                        dy[j] = dy[i];
                        filled[j] = true;
                        next.push(j);
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            frontier = next;
            all_filled = filled.iter().all(|&f| f);
        }

        Self {
            cols,
            rows,
            cell,
            width,
            height,
            dx,
            dy,
        }
    }

    /// Echantillonne le champ par interpolation bilineaire.
    fn sample(&self, x: f64, y: f64) -> (f64, f64) {
        let gx = (x / self.cell as f64) - 0.5;
        let gy = (y / self.cell as f64) - 0.5;
        let x0 = gx.floor().max(0.0) as usize;
        let y0 = gy.floor().max(0.0) as usize;
        let x1 = (x0 + 1).min(self.cols - 1);
        let y1 = (y0 + 1).min(self.rows - 1);
        let fx = (gx - x0 as f64).clamp(0.0, 1.0);
        let fy = (gy - y0 as f64).clamp(0.0, 1.0);

        let lerp = |a: f32, b: f32, c: f32, d: f32| -> f64 {
            let top = a as f64 * (1.0 - fx) + b as f64 * fx;
            let bot = c as f64 * (1.0 - fx) + d as f64 * fx;
            top * (1.0 - fy) + bot * fy
        };

        let i00 = y0 * self.cols + x0;
        let i01 = y0 * self.cols + x1;
        let i10 = y1 * self.cols + x0;
        let i11 = y1 * self.cols + x1;

        let dx = lerp(self.dx[i00], self.dx[i01], self.dx[i10], self.dx[i11]);
        let dy = lerp(self.dy[i00], self.dy[i01], self.dy[i10], self.dy[i11]);
        (dx, dy)
    }
}

/// Echantillonnage bicubique (Catmull-Rom) d'une image RGB 16-bit.
///
/// Prefere au bilineaire pour la deformation : le bilineaire lisse fortement les
/// hautes frequences, ce qui detruirait le detail fin qu'on cherche justement a
/// preserver.
fn sample_bilinear(src: &[u16], width: usize, height: usize, x: f64, y: f64) -> (u16, u16, u16) {
    let x = x.clamp(0.0, (width - 1) as f64);
    let y = y.clamp(0.0, (height - 1) as f64);
    let x0 = x.floor() as isize;
    let y0 = y.floor() as isize;
    let fx = x - x0 as f64;
    let fy = y - y0 as f64;

    let at = |px: isize, py: isize, c: usize| -> f64 {
        let px = px.clamp(0, width as isize - 1) as usize;
        let py = py.clamp(0, height as isize - 1) as usize;
        src[(py * width + px) * 3 + c] as f64
    };

    let w = catmull_rom_weights(fx);
    let v = catmull_rom_weights(fy);
    let mut out = [0u16; 3];
    for c in 0..3 {
        let mut acc = 0.0;
        for (j, &vy) in v.iter().enumerate() {
            let mut row = 0.0;
            for (i, &wx) in w.iter().enumerate() {
                row += at(x0 - 1 + i as isize, y0 - 1 + j as isize, c) * wx;
            }
            acc += row * vy;
        }
        out[c] = acc.clamp(0.0, 65535.0).round() as u16;
    }
    (out[0], out[1], out[2])
}

/// Poids Catmull-Rom pour 4 points, etant donne la fraction `t` dans [0,1].
fn catmull_rom_weights(t: f64) -> [f64; 4] {
    let t2 = t * t;
    let t3 = t2 * t;
    [
        -0.5 * t3 + t2 - 0.5 * t,
        1.5 * t3 - 2.5 * t2 + 1.0,
        -1.5 * t3 + 2.0 * t2 + 0.5 * t,
        0.5 * t3 - 0.5 * t2,
    ]
}
