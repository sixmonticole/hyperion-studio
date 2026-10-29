//! Empilement des frames gardees dans un accumulateur flottant.
//!
//! Rendement memoire : on ne garde jamais plus d'une frame en RAM ; on
//! accumule directement dans un buffer f32 (une frame empilee en V1 = crop
//! carre, donc ~9 Mo pour 1200x1200x3). Voir `pipeline/mod.rs` pour la boucle.

/// Per-tile admission mask for one frame during per-region stacking.
///
/// `gate[i] == 1` means "this frame contributes to tile i". It is what lets
/// each region of the final image be drawn from the frames that are sharp
/// *there*, rather than from a global keep list.
pub struct TileGate<'a> {
    /// One byte per tile, in row-major order (`ty * tiles_x + tx`).
    pub gate: &'a [u8],
    /// Number of tiles along x.
    pub tiles_x: usize,
    /// Side of a tile, in pixels.
    pub tile: usize,
}

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
        self.add_gated(rgb16, valid, weight, scale, None)
    }

    /// Ajoute une frame avec une **porte par tuile** : seuls les pixels des
    /// tuiles marquees actives contribuent.
    ///
    /// C'est le coeur de la selection par zone : une frame jugee nette au
    /// terminateur mais molle au limbe n'est admise que dans les tuiles du
    /// terminateur. Aucun pixel n'est moyenné avec du bruit.
    ///
    /// `gate` est un masque par tuile (1 = admise). Sa longueur doit valoir
    /// `tiles_x * tiles_y` ; `tile` est le cote d'une tuile en pixels. Un gate
    /// `None` equivaut a « toutes les tuiles actives ».
    pub fn add_gated(
        &mut self,
        rgb16: &[u16],
        valid: Option<&[u8]>,
        weight: f32,
        scale: f32,
        gate: Option<TileGate<'_>>,
    ) {
        if rgb16.len() != self.sum.len() {
            return;
        }
        if let Some(mask) = valid {
            if mask.len() != self.weight.len() {
                return;
            }
        }
        let w = weight.max(0.0);

        for i in 0..self.weight.len() {
            // Masque geometrique : pixel hors cadre ignore.
            if let Some(mask) = valid {
                if mask[i] == 0 {
                    continue;
                }
            }
            // Porte par tuile : pixel d'une tuile non retenue ignore pour
            // cette frame.
            if let Some(g) = &gate {
                let x = i % self.width;
                let y = i / self.width;
                let tx = (x / g.tile).min(g.tiles_x.saturating_sub(1));
                let ty = (y / g.tile).min(g.gate.len() / g.tiles_x.max(1));
                let ti = (ty * g.tiles_x + tx).min(g.gate.len() - 1);
                if g.gate[ti] == 0 {
                    continue;
                }
            }

            self.weight[i] += w;
            let base = i * 3;
            self.sum[base] += rgb16[base] as f32 * scale * w;
            self.sum[base + 1] += rgb16[base + 1] as f32 * scale * w;
            self.sum[base + 2] += rgb16[base + 2] as f32 * scale * w;
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
