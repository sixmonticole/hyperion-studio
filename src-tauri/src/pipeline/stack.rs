//! Empilement des frames gardees dans un accumulateur flottant.
//!
//! Rendement memoire : on ne garde jamais plus d'une frame en RAM ; on
//! accumule directement dans un buffer f32 (une frame empilee en V1 = crop
//! carre, donc ~9 Mo pour 1200x1200x3). Voir `pipeline/mod.rs` pour la boucle.

/// Accumulateur de stack RGB en f32.
///
/// La somme et le **poids** sont suivis **par pixel** : quand une frame est
/// decalee et qu'une partie sort du cadre, ces pixels ne doivent pas etre
/// moyennes avec du noir. Avec une derive de plusieurs centaines de pixels en
/// trepied fixe, moyenner le noir ecrasait les bords et adoucissait l'image.
pub struct Accumulator {
    #[allow(dead_code)]
    pub width: usize,
    #[allow(dead_code)]
    pub height: usize,
    /// Somme ponderee des canaux (longueur = w * h * 3).
    pub sum: Vec<f32>,
    /// Poids cumule par pixel (longueur = w * h), 0 = aucun echantillon valide.
    pub weight: Vec<f32>,
    /// Somme des poids appliques (toutes frames).
    pub weight_sum: f32,
    /// nombre de frames ajoutees.
    pub count: usize,
}

impl Accumulator {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            sum: vec![0.0f32; width * height * 3],
            weight: vec![0.0f32; width * height],
            weight_sum: 0.0,
            count: 0,
        }
    }

    /// Ajoute une frame RGB 16-bit (deja croppee et alignee) avec un poids donne.
    ///
    /// `valid` (optionnel) vaut 1 pour un pixel echantillonne dans la frame, 0
    /// pour un pixel hors cadre (a ignorer). Sans masque, tous les pixels sont
    /// consideres valides.
    ///
    /// `scale` compense la luminance : chaque frame est ramenee a la meme
    /// brillance moyenne avant addition, ce qui evite les bandes de scintillation.
    pub fn add(&mut self, rgb16: &[u16], valid: Option<&[u8]>, weight: f32, scale: f32) {
        if rgb16.len() != self.sum.len() {
            return;
        }
        let w = weight.max(0.0);
        if let Some(mask) = valid {
            if mask.len() != self.weight.len() {
                return;
            }
            for (i, &m) in mask.iter().enumerate() {
                if m == 0 {
                    continue;
                }
                let pw = w;
                self.weight[i] += pw;
                let base = i * 3;
                self.sum[base] += rgb16[base] as f32 * scale * pw;
                self.sum[base + 1] += rgb16[base + 1] as f32 * scale * pw;
                self.sum[base + 2] += rgb16[base + 2] as f32 * scale * pw;
            }
        } else {
            for (i, &v) in rgb16.iter().enumerate() {
                self.sum[i] += v as f32 * scale * w;
            }
            for pw in self.weight.iter_mut() {
                *pw += w;
            }
        }
        self.weight_sum += w;
        self.count += 1;
    }

    /// Produit l'image finale RGB 16-bit normalisee.
    pub fn finish(&self) -> Vec<u16> {
        let mut out = vec![0u16; self.sum.len()];
        for (i, pw) in self.weight.iter().enumerate() {
            if *pw <= 0.0 {
                continue;
            }
            let base = i * 3;
            for c in 0..3 {
                let v = (self.sum[base + c] / pw).clamp(0.0, 65535.0);
                out[base + c] = v.round() as u16;
            }
        }
        out
    }
}

/// Calcule la luminance moyenne d'une frame RGB 16-bit (pour la normalisation).
pub fn mean_luminance(rgb16: &[u16]) -> f32 {
    if rgb16.is_empty() {
        return 0.0;
    }
    let mut sum = 0.0f64;
    for px in rgb16.chunks_exact(3) {
        sum += 0.299 * px[0] as f64 + 0.587 * px[1] as f64 + 0.114 * px[2] as f64;
    }
    (sum / (rgb16.len() / 3) as f64) as f32
}
