//! Score de nettete de chaque frame, pour ne garder que les meilleures.
//!
//! On combine deux indicateurs classiques en planetaire :
//! - la variance du Laplacien (sensibilite au flou)
//! - la mesure de Tenengrad (energie des gradients, via Sobel)
//!
//! Les deux sont calcules sur la luminance de la fenetre de crop uniquement, ce
//! qui evite que le fond de ciel bruite ne domine le score.

/// Calcule le score de nettete d'une frame croppee (luminance uniquement).
pub fn sharpness_score(cropped_rgb16: &[u16], width: usize, height: usize) -> f64 {
    if width < 3 || height < 3 {
        return 0.0;
    }

    // Luminance plane de la fenetre.
    let mut lum = vec![0.0f32; width * height];
    for (i, px) in lum.iter_mut().enumerate() {
        let idx = i * 3;
        *px = 0.299 * cropped_rgb16[idx] as f32
            + 0.587 * cropped_rgb16[idx + 1] as f32
            + 0.114 * cropped_rgb16[idx + 2] as f32;
    }

    let at = |x: usize, y: usize| -> f32 { lum[y * width + x] };

    // Variance du Laplacien (kernel 4-voisins).
    let mut lap_sum = 0.0f64;
    let mut lap_sq = 0.0f64;
    let mut count = 0.0f64;

    // Tenengrad via Sobel.
    let mut tenengrad = 0.0f64;

    for y in 1..height - 1 {
        for x in 1..width - 1 {
            let lap = -4.0 * at(x, y)
                + at(x - 1, y)
                + at(x + 1, y)
                + at(x, y - 1)
                + at(x, y + 1);
            lap_sum += lap as f64;
            lap_sq += (lap * lap) as f64;
            count += 1.0;

            let gx = -at(x - 1, y - 1) - 2.0 * at(x - 1, y) - at(x - 1, y + 1)
                + at(x + 1, y - 1)
                + 2.0 * at(x + 1, y)
                + at(x + 1, y + 1);
            let gy = -at(x - 1, y - 1) - 2.0 * at(x, y - 1) - at(x + 1, y - 1)
                + at(x - 1, y + 1)
                + 2.0 * at(x, y + 1)
                + at(x + 1, y + 1);
            tenengrad += (gx * gx + gy * gy) as f64;
        }
    }

    if count <= 0.0 {
        return 0.0;
    }

    let mean = lap_sum / count;
    let lap_var = (lap_sq / count - mean * mean).max(0.0);
    let teng = tenengrad / count;

    // Combinaison : les deux mesures varient de plusieurs ordres de grandeur,
    // on prend la racine pour compresser la dynamique.
    (lap_var.sqrt() + teng.sqrt()).sqrt()
}

/// Normalise les scores bruts en [0, 1] par min-max.
pub fn normalize_scores(scores: &[f64]) -> Vec<f64> {
    let min = scores.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = scores.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = max - min;
    if !range.is_finite() || range <= 0.0 {
        return vec![1.0; scores.len()];
    }
    scores.iter().map(|&s| (s - min) / range).collect()
}
