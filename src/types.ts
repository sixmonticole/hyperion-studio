// Types partagés avec le backend Rust (voir src-tauri/src/model.rs).
// Toutes les structures sont définies avec des valeurs par défaut côté TS, et le
// backend tolère les champs manquants (#[serde(default)]), pour qu'une évolution
// d'un seul côté ne casse jamais l'application.

export type Phase =
  | "idle"
  | "importing"
  | "analyzing"
  | "stacking"
  | "enhancing"
  | "done"
  | "failed";

export interface PlanetPreview {
  width?: number;
  height?: number;
  model?: string;
  planetDetected?: boolean;
  planetRadius?: number;
  error?: string;
}

export interface ScanResult {
  count: number;
  path: string;
  cached: number;
  preview?: PlanetPreview;
}

export interface Progress {
  kind: "phase" | "import" | "analyze" | "stack" | "warning" | "finished" | "error";
  phase?: Phase;
  message?: string;
  done?: number;
  total?: number;
  /// Renseigné pour `kind: "warning"` : la frame écartée et la raison.
  name?: string;
  reason?: string;
  report?: StackReport;
}

export interface CropRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface SkippedFrame {
  /// Nom du fichier source (pas le chemin complet).
  name: string;
  /// Pourquoi la frame a été écartée.
  reason: string;
}

export interface StackReport {
  sourceDir: string;
  outputPath: string;
  /// Chemin du stack brut (avant post-traitement). Absent si le
  /// post-traitement est désactivé : le fichier serait identique.
  rawOutputPath?: string | null;
  totalFrames: number;
  keptFrames: number;
  meanQuality: number;
  crop?: CropRect;
  /// Frames écartées pendant le traitement (illisibles, corrompues...).
  skipped?: SkippedFrame[];
}

export interface Orientation {
  rotate: number;
  flipH: boolean;
  flipV: boolean;
}

export interface EnhanceSettings {
  enabled: boolean;
  waveletAmount: number;
  waveletLevels: number;
  blackPoint: number;
  whitePoint: number;
  gamma: number;
  claheAmount: number;
  claheTiles: number;
  clarity: number;
}

export interface Settings {
  keepFraction: number;
  cropSize: number;
  align: boolean;
  normalize: boolean;
  weighted: boolean;
  apSize: number;
  apEnabled: boolean;
  derotate: boolean;
  blockSize: number;
  orientation: Orientation;
  enhance: EnhanceSettings;
}

export const DEFAULT_SETTINGS: Settings = {
  keepFraction: 0.25,
  cropSize: 0,
  align: true,
  normalize: true,
  weighted: true,
  apSize: 0,
  apEnabled: false,
  derotate: false,
  blockSize: 10,
  orientation: { rotate: 0, flipH: false, flipV: false },
  enhance: {
    enabled: false,
    waveletAmount: 1.0,
    waveletLevels: 4,
    blackPoint: 0,
    whitePoint: 1.0,
    gamma: 1.0,
    claheAmount: 0.15,
    claheTiles: 8,
    clarity: 0.12,
  },
};

// Fusion défensive : garantit que tous les champs existent, même si un objet
// partiel arrive d'ailleurs (état sauvegardé, ancienne version).
export function mergeSettings(partial: Partial<Settings> | undefined | null): Settings {
  const p = partial ?? {};
  return {
    ...DEFAULT_SETTINGS,
    ...p,
    orientation: { ...DEFAULT_SETTINGS.orientation, ...(p.orientation ?? {}) },
    enhance: { ...DEFAULT_SETTINGS.enhance, ...(p.enhance ?? {}) },
  };
}
