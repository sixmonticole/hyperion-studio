//! Alignement des frames par correlation de phase (FFT).
//!
//! La correlation de phase donne le deplacement (dx, dy) entier entre deux
//! images. On l'applique sur la luminance de la fenetre de crop, apres une
//! fenetre de Hann pour limiter les effets de bord.
//!
//! En V1, l'alignement est global (une seule translation par frame). La grille
//! de points locaux (AP) viendra en V2.

use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlanner};
use std::sync::Arc;

/// Deplacement estime (dx, dy) entre une frame et la reference.
pub type Shift = (f64, f64);

/// Aligneur : garde la FFT de la reference en cache pour ne pas la recalculer.
pub struct Aligner {
    width: usize,
    height: usize,
    fft: Arc<dyn Fft<f32>>,
    ifft: Arc<dyn Fft<f32>>,
    /// Spectre de la frame de reference (luminance fenetree).
    ref_spectrum: Vec<Complex32>,
}

impl Aligner {
    /// Construit l'aligneur a partir de la frame de reference (luminance croppee).
    pub fn new(reference_lum: &[f32], width: usize, height: usize) -> Self {
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(width * height);
        let ifft = planner.plan_fft_inverse(width * height);

        let windowed = apply_hann(reference_lum, width, height);
        let mut buf: Vec<Complex32> = windowed.iter().map(|&v| Complex32::new(v, 0.0)).collect();
        fft.process(&mut buf);

        Self {
            width,
            height,
            fft,
            ifft,
            ref_spectrum: buf,
        }
    }

    /// Estime le deplacement de `frame_lum` par rapport a la reference.
    pub fn estimate(&self, frame_lum: &[f32]) -> Shift {
        let windowed = apply_hann(frame_lum, self.width, self.height);
        let mut buf: Vec<Complex32> = windowed.iter().map(|&v| Complex32::new(v, 0.0)).collect();
        self.fft.process(&mut buf);

        // Correlation croisee normalisee : (A * conj(B)) / |A * conj(B)|.
        for i in 0..buf.len() {
            let a = self.ref_spectrum[i];
            let b = buf[i];
            let cross = a * b.conj();
            let mag = cross.norm();
            buf[i] = if mag > 1e-9 {
                cross / mag
            } else {
                Complex32::new(0.0, 0.0)
            };
        }

        self.ifft.process(&mut buf);

        // Recherche du pic dans le domaine spatial = translation (sub-pixel).
        let (mut best_idx, mut best_val) = (0usize, f32::NEG_INFINITY);
        for (i, c) in buf.iter().enumerate() {
            if c.re > best_val {
                best_val = c.re;
                best_idx = i;
            }
        }

        let w = self.width;
        let h = self.height;
        let px = best_idx % w;
        let py = best_idx / w;

        // Ajustement parabolique pour un deplacement sub-pixel : sans ca, le
        // stack empile des frames decalees d'une fraction de pixel et l'image
        // finale est systematiquement adoucie.
        let (sx, sy) = subpixel_peak(&buf, w, h, px, py);

        // Repli du wrap-around FFT vers un deplacement signe.
        let wrap = |v: f64, n: usize| -> f64 {
            if v > n as f64 / 2.0 {
                v - n as f64
            } else {
                v
            }
        };
        let dx = wrap(px as f64 + sx, w);
        let dy = wrap(py as f64 + sy, h);

        // Le pic de correlation croisee donne directement la correction a
        // appliquer a la frame via `shift_rgb16` pour la ramener sur la reference.
        (dx, dy)
    }
}

/// Ajustement parabolique du pic de correlation, pour une estimation sub-pixel.
fn subpixel_peak(buf: &[Complex32], w: usize, h: usize, px: usize, py: usize) -> (f64, f64) {
    let at = |x: isize, y: isize| -> f64 {
        let xx = ((x + w as isize) % w as isize) as usize;
        let yy = ((y + h as isize) % h as isize) as usize;
        buf[yy * w + xx].re as f64
    };
    let f = |a: f64, b: f64, c: f64| -> f64 {
        let denom = a - 2.0 * b + c;
        if denom.abs() < 1e-12 {
            0.0
        } else {
            (0.5 * (a - c) / denom).clamp(-0.5, 0.5)
        }
    };
    let sx = f(
        at(px as isize - 1, py as isize),
        at(px as isize, py as isize),
        at(px as isize + 1, py as isize),
    );
    let sy = f(
        at(px as isize, py as isize - 1),
        at(px as isize, py as isize),
        at(px as isize, py as isize + 1),
    );
    (sx, sy)
}

/// Applique une fenetre de Hann 2D a une luminance plane.
fn apply_hann(lum: &[f32], width: usize, height: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; lum.len()];
    let w = width.max(1) as f32;
    let h = height.max(1) as f32;
    for y in 0..height {
        let wy = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * y as f32 / (h - 1.0).max(1.0)).cos();
        for x in 0..width {
            let wx = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * x as f32 / (w - 1.0).max(1.0)).cos();
            out[y * width + x] = lum[y * width + x] * wx * wy;
        }
    }
    out
}

/// Extrait la luminance plane d'une image RGB 16-bit.
pub fn luminance(rgb16: &[u16], pixels: usize) -> Vec<f32> {
    let mut lum = vec![0.0f32; pixels];
    for (i, out) in lum.iter_mut().enumerate() {
        let idx = i * 3;
        *out = 0.299 * rgb16[idx] as f32
            + 0.587 * rgb16[idx + 1] as f32
            + 0.114 * rgb16[idx + 2] as f32;
    }
    lum
}

/// Applique une transformation d'orientation a une image RGB 16-bit.
///
/// Ordre d'application : miroir horizontal, puis miroir vertical, puis rotation
/// horaire. Renvoie l'image transformee et ses nouvelles dimensions.
pub fn apply_orientation(
    src: &[u16],
    width: usize,
    height: usize,
    o: crate::model::Orientation,
) -> (Vec<u16>, usize, usize) {
    let mut cur = src.to_vec();
    let mut w = width;
    let mut h = height;

    if o.flip_h {
        let flipped = flip_h(&cur, w, h);
        cur = flipped;
    }
    if o.flip_v {
        let flipped = flip_v(&cur, w, h);
        cur = flipped;
    }

    match o.rotate % 360 {
        90 => {
            let r = rotate90(&cur, w, h);
            std::mem::swap(&mut w, &mut h);
            cur = r;
        }
        180 => {
            cur = rotate180(&cur, w, h);
        }
        270 => {
            let r = rotate270(&cur, w, h);
            std::mem::swap(&mut w, &mut h);
            cur = r;
        }
        _ => {}
    }

    (cur, w, h)
}

fn flip_h(src: &[u16], w: usize, h: usize) -> Vec<u16> {
    let mut out = vec![0u16; src.len()];
    for y in 0..h {
        for x in 0..w {
            let si = (y * w + x) * 3;
            let di = (y * w + (w - 1 - x)) * 3;
            out[di..di + 3].copy_from_slice(&src[si..si + 3]);
        }
    }
    out
}

fn flip_v(src: &[u16], w: usize, h: usize) -> Vec<u16> {
    let mut out = vec![0u16; src.len()];
    for y in 0..h {
        let dy = h - 1 - y;
        let row = w * 3;
        out[dy * row..(dy + 1) * row].copy_from_slice(&src[y * row..(y + 1) * row]);
    }
    out
}

fn rotate180(src: &[u16], w: usize, h: usize) -> Vec<u16> {
    let n = w * h;
    let mut out = vec![0u16; src.len()];
    for i in 0..n {
        let si = i * 3;
        let di = (n - 1 - i) * 3;
        out[di..di + 3].copy_from_slice(&src[si..si + 3]);
    }
    out
}

/// Rotation horaire 90 : sortie (h, w). out(x, y) = src(y, h-1-x).
fn rotate90(src: &[u16], w: usize, h: usize) -> Vec<u16> {
    let mut out = vec![0u16; src.len()];
    let ow = h; // largeur de sortie
    for y in 0..h {
        for x in 0..w {
            let si = (y * w + x) * 3;
            let ox = h - 1 - y;
            let oy = x;
            let di = (oy * ow + ox) * 3;
            out[di..di + 3].copy_from_slice(&src[si..si + 3]);
        }
    }
    out
}

/// Rotation horaire 270 (anti-horaire 90). Sortie (h, w).
fn rotate270(src: &[u16], w: usize, h: usize) -> Vec<u16> {
    let mut out = vec![0u16; src.len()];
    let ow = h;
    for y in 0..h {
        for x in 0..w {
            let si = (y * w + x) * 3;
            let ox = y;
            let oy = w - 1 - x;
            let di = (oy * ow + ox) * 3;
            out[di..di + 3].copy_from_slice(&src[si..si + 3]);
        }
    }
    out
}

/// Translation d'une image RGB 16-bit avec interpolation sub-pixel.
///
/// `shift` est le vecteur retourne par `Aligner::estimate`, qui est deja
/// sub-pixel (ajustement parabolique du pic) : `out(x, y) = src(x - dx, y - dy)`.
///
/// L'interpolation est bicubique (Catmull-Rom) : un simple arrondi au pixel
/// entier empile des frames decalees d'une fraction de pixel et **adoucit
/// l'image** ; l'interpolation sub-pixel est indispensable pour un stack net.
///
/// Renvoie aussi un **masque** (1 = pixel echantillonne dans la frame, 0 = hors
/// cadre). Sans ce masque, l'empilement moyennerait du noir la ou la frame
/// decalee est sortie du cadre, ce qui ecrase les bords.
pub fn shift_rgb16_masked(
    src: &[u16],
    width: usize,
    height: usize,
    shift: Shift,
) -> (Vec<u16>, Vec<u8>) {
    let dx = shift.0;
    let dy = shift.1;

    // Deplacement quasi nul : aucun reechantillonnage, on garde l'image intacte.
    if dx.abs() < 0.01 && dy.abs() < 0.01 {
        return (src.to_vec(), vec![1u8; width * height]);
    }

    let mut out = vec![0u16; src.len()];
    let mut mask = vec![0u8; width * height];
    for y in 0..height {
        for x in 0..width {
            let sx = x as f64 - dx;
            let sy = y as f64 - dy;
            // Un pixel est valide si sa source (avec le noyau bicubique 4x4)
            // reste entierement dans l'image.
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

/// Translation d'une image RGB 16-bit, sans masque (les pixels hors cadre
/// deviennent noirs). Enveloppe de commodite autour de `shift_rgb16_masked`.
pub fn shift_rgb16(src: &[u16], width: usize, height: usize, shift: Shift) -> Vec<u16> {
    shift_rgb16_masked(src, width, height, shift).0
}

/// Echantillonnage bicubique (Catmull-Rom) d'une image RGB 16-bit.
fn sample_bicubic(src: &[u16], width: usize, height: usize, x: f64, y: f64) -> (u16, u16, u16) {
    // Hors cadre : pixel noir (les bords du stack viennent de la marge de crop).
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

    let wx = catmull_rom(fx);
    let wy = catmull_rom(fy);
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

/// Poids Catmull-Rom pour 4 points, etant donne la fraction `t` dans [0,1].
fn catmull_rom(t: f64) -> [f64; 4] {
    let t2 = t * t;
    let t3 = t2 * t;
    [
        -0.5 * t3 + t2 - 0.5 * t,
        1.5 * t3 - 2.5 * t2 + 1.0,
        -1.5 * t3 + 2.0 * t2 + 0.5 * t,
        0.5 * t3 - 0.5 * t2,
    ]
}
