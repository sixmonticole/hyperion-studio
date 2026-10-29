use serde::{Deserialize, Serialize};

/// Etat global d'une session de traitement : un dossier de RAW = un empilement.
///
/// Reserve au suivi multi-sessions (V2) ; le pipeline V1 travaille directement
/// sur des chemins et ne persiste pas cet objet.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// Identifiant interne (uuid-like, base sur un compteur).
    pub id: String,
    /// Dossier source contenant les RAW.
    pub source_dir: String,
    /// Dossier de cache (TIFF 16-bit dematriçes).
    pub cache_dir: String,
    /// Liste des frames importees.
    pub frames: Vec<FrameInfo>,
    /// Taille (largeur, hauteur) commune des TIFF du cache, une fois connue.
    pub cache_dim: Option<(usize, usize)>,
    /// Etat courant du pipeline.
    pub phase: Phase,
}

/// Etat d'avancement d'un pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Idle,
    Importing,
    Analyzing,
    Stacking,
    Enhancing,
    Done,
    Failed,
}

/// Une frame individuelle, une fois dematriçee dans le cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameInfo {
    /// Chemin du RAW source.
    pub source_path: String,
    /// Chemin du TIFF 16-bit dans le cache.
    pub cache_path: String,
    /// Index de la frame dans l'ordre du dossier (0 = premiere).
    pub index: usize,
    /// Score de nettete normalise (0.0 = floue, 1.0 = nette). Rempli apres analyse.
    pub quality: Option<f64>,
    /// Deplacement estime (dx, dy) par rapport a la reference. Rempli apres alignement.
    pub shift: Option<(f64, f64)>,
    /// Faut-il garder cette frame dans l'empilement ?
    pub keep: bool,
    /// Score de nettete par tuile (index = ty * tiles_x + tx). Vide tant que
    /// l'analyse n'a pas tourne, ou si la selection par zone est desactivee.
    /// Permet de tirer chaque region de l'image des frames nettes *a cet
    /// endroit*, au lieu du seul score global.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tile_scores: Vec<f32>,
    /// Nombre de tuiles en largeur pour `tile_scores`.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub tiles_x: usize,
}

/// Petit predicat de serialisation : omet les champs nuls.
fn is_zero(v: &usize) -> bool {
    *v == 0
}

impl FrameInfo {
    pub fn new(source_path: String, cache_path: String, index: usize) -> Self {
        Self {
            source_path,
            cache_path,
            index,
            quality: None,
            shift: None,
            keep: true,
            tile_scores: Vec::new(),
            tiles_x: 0,
        }
    }
}

/// Transformations d'orientation appliquees a l'image finale.
///
/// L'EXIF ne peut pas savoir qu'un telescope (renvoi coude, Newton) retourne
/// l'image : ces reglages sont donc exposes dans l'UI et appliques a l'export.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Orientation {
    /// Rotation dans le sens horaire : 0, 90, 180 ou 270 degres.
    pub rotate: u16,
    /// Miroir gauche-droite.
    pub flip_h: bool,
    /// Miroir haut-bas.
    pub flip_v: bool,
}

impl Default for Orientation {
    fn default() -> Self {
        Self {
            rotate: 0,
            flip_h: false,
            flip_v: false,
        }
    }
}

/// Reglages du post-traitement de l'image finale.
///
/// Tous les effets sont desactives par defaut (stack brut) ; l'utilisateur les
/// active dans le panneau dedie. Les valeurs par defaut, une fois actives,
/// donnent un rendu proche d'un traitement lunaire classique.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EnhanceSettings {
    /// Active le post-traitement.
    pub enabled: bool,
    /// Force des ondelettes (0 = aucune). Defaut 0.6.
    pub wavelet_amount: f32,
    /// Nombre d'echelles d'ondelettes (1-6). Defaut 4.
    pub wavelet_levels: usize,
    /// Point noir des niveaux (0-1). Defaut 0.0.
    pub black_point: f32,
    /// Point blanc des niveaux (0-1). Defaut 1.0 (neutre : l'etirement
    /// automatique suffit ; en dessous de 1 on pousse le blanc plus loin).
    pub white_point: f32,
    /// Gamma (1 = neutre, > 1 eclaircit les tons moyens). Defaut 1.0.
    pub gamma: f32,
    /// Force du CLAHE (0 = aucun). Defaut 0.35.
    pub clahe_amount: f32,
    /// Nombre de blocs CLAHE par dimension (2-16). Defaut 8.
    pub clahe_tiles: usize,
    /// Force de la clarte / contraste local (0 = aucun). Defaut 0.3.
    pub clarity: f32,
}

impl Default for EnhanceSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            wavelet_amount: 0.6,
            wavelet_levels: 4,
            black_point: 0.0,
            white_point: 1.0,
            gamma: 1.0,
            clahe_amount: 0.35,
            clahe_tiles: 8,
            clarity: 0.3,
        }
    }
}

/// Reglages du pipeline. Tous auto par defaut ; exposes dans le panneau "avance".
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Pourcentage des meilleures frames a conserver (0.0 - 1.0). Defaut 0.25.
    pub keep_fraction: f64,
    /// Taille de la fenetre de crop (carre, en pixels). 0 = auto (detection planete).
    pub crop_size: usize,
    /// Aligner les frames (correlation de phase). Defaut true.
    pub align: bool,
    /// Normaliser la luminosite de chaque frame avant empilement. Defaut true.
    pub normalize: bool,
    /// Ponderer chaque frame par son score qualite. Defaut true.
    pub weighted: bool,
    /// Taille d'un point d'alignement local (AP size). 0 = auto.
    pub ap_size: usize,
    /// Active l'alignement a points locaux (corrige la turbulence locale).
    pub ap_enabled: bool,
    /// Active la derotation (alignement translation + rotation + echelle).
    pub derotate: bool,
    /// Taille des blocs pour l'empilement (nombre de frames consecutives
    /// empilees ensemble). 0 = desactive l'empilement par blocs (empilement
    /// global, adapte seulement aux sessions courtes).
    ///
    /// Sur une longue session en trepied fixe, la Lune tourne et son eclairage
    /// change : empiler des frames tres eloignees dans le temps moyenne des
    /// images qui ne se superposent plus (resultat flou). Empiler par blocs
    /// courts, puis moyenner les blocs, preserve la nettete.
    pub block_size: usize,
    /// Selection de frames **par zone** : chaque region de l'image est tiree
    /// des frames qui y sont nettes, au lieu du seul tri global. Defaut true.
    pub tile_select: bool,
    /// Taille d'une tuile de selection (cote, en pixels). Defaut 64.
    pub tile_size: usize,
    /// Orientation appliquee a l'image finale.
    pub orientation: Orientation,
    /// Post-traitement de l'image finale (ondelettes, niveaux, CLAHE, clarte).
    pub enhance: EnhanceSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            keep_fraction: 0.25,
            crop_size: 0,
            align: true,
            normalize: true,
            weighted: true,
            ap_size: 0,
            ap_enabled: false,
            derotate: false,
            block_size: 10,
            tile_select: true,
            tile_size: 64,
            orientation: Orientation::default(),
            enhance: EnhanceSettings::default(),
        }
    }
}

/// Metadonnees ecrites a cote du stack final, pour la tracabilite.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StackReport {
    pub source_dir: String,
    pub output_path: String,
    /// Chemin du stack brut (avant post-traitement). Absent si le
    /// post-traitement est desactive : le fichier serait identique.
    pub raw_output_path: Option<String>,
    pub total_frames: usize,
    pub kept_frames: usize,
    pub mean_quality: f64,
    pub crop: CropRect,
    pub settings: Settings,
    /// Frames ignorees pendant le traitement, avec la raison. Une frame
    /// illisible ne doit jamais faire echouer toute la session : elle est
    /// ecartee, signalee, et le reste continue.
    #[serde(default)]
    pub skipped: Vec<SkippedFrame>,
}

/// Une frame ecartee du traitement, avec la raison lisible.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedFrame {
    /// Source file name (not the full path: shorter to display).
    pub name: String,
    /// Why the frame was skipped, as a stable code.
    pub reason: SkipReason,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CropRect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

/// Evenements de progression envoyes a l'UI.
///
/// Les messages ne portent **pas de texte** : ils portent un `Stage` (code
/// stable) et, si besoin, des parametres. C'est le frontend qui traduit, ce qui
/// permet de changer de langue sans recompiler et sans dupliquer les chaines
/// cote Rust.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Progress {
    /// Current phase, identified by a stable stage code.
    Phase { phase: Phase, stage: Stage },
    /// Import : n frames dematriçees sur total.
    Import { done: usize, total: usize },
    /// Analyse : n frames scorees sur total.
    Analyze { done: usize, total: usize },
    /// Stacking : n frames empilees sur total.
    Stack { done: usize, total: usize },
    /// Une frame a ete ecartee (illisible, corrompue...). Non fatal.
    /// `reason` est un code, pas une phrase.
    Warning { name: String, reason: SkipReason },
    /// Termine avec le rapport final.
    Finished { report: Box<StackReport> },
    /// Erreur fatale, identifiee par un code.
    Error { error: JobError },
}

/// Stable, translatable stage codes emitted by the pipeline. The frontend maps
/// each to a localized sentence; `count` carries the only variable part.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    ScanningFiles,
    DetectingObject,
    ScoringFrames,
    Aligning,
    /// Stacking `count` frames (block size detail is in the message params).
    Stacking,
    Enhancing,
    WritingOutput,
}

/// Why a single frame was skipped. A code, not a sentence.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "detail")]
pub enum SkipReason {
    /// Could not be decoded or read.
    Unreadable,
    /// Its cached TIFF no longer exists or is empty.
    MissingCache,
    /// Not enough usable frames to stack.
    NotEnoughFrames { found: usize, total: usize },
}

/// A fatal pipeline error, identified by a code so the UI can localize it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "detail")]
pub enum JobError {
    NoRawFiles,
    NoUsableFrames,
    NotEnoughFrames { found: usize, total: usize },
    ObjectNotDetected,
    Cancelled,
    /// Anything else, with the raw technical message for diagnostics.
    Other { message: String },
}
