// Bilingual UI text (French and English).
//
// The codebase is written in English; all user-facing strings live here, in
// both languages, keyed by a stable identifier. The backend never sends prose:
// it sends stage/error codes (see `model.rs`), which are resolved here. That
// way a language switch needs no recompile and the two languages can never
// drift apart structurally.

import { useCallback, useEffect, useState } from "react";
import type { JobError, SkipReason, Stage } from "./types";

export type Lang = "fr" | "en";

const STORAGE_KEY = "hyperion:lang";

/// All translatable strings. `{name}`-style placeholders are interpolated.
type Dict = {
  // --- App shell ---
  "app.brand": string;
  "app.tagline": string;
  "app.theme.toDark": string;
  "app.theme.toLight": string;
  "app.theme.label": string;
  "app.lang.label": string;

  // --- Wizard ---
  "wiz.step.folder": string;
  "wiz.step.folder.sub": string;
  "wiz.step.settings": string;
  "wiz.step.settings.sub": string;
  "wiz.step.processing": string;
  "wiz.step.processing.sub": string;
  "wiz.folder.title": string;
  "wiz.folder.desc": string;
  "wiz.folder.pick": string;
  "wiz.folder.picked": string;
  "wiz.folder.hintClick": string;
  "wiz.folder.scanning": string;
  "wiz.folder.ready": string;
  "wiz.folder.selectFirst": string;
  "wiz.folder.noObject": string;
  "wiz.folder.objectDetected": string;
  "wiz.continue": string;
  "wiz.settings.title": string;
  "wiz.settings.desc": string;
  "wiz.settings.orientation": string;
  "wiz.settings.orientationHint": string;
  "wiz.settings.post": string;
  "wiz.settings.postLabel": string;
  "wiz.settings.postDesc": string;
  "wiz.recap.keep": string;
  "wiz.recap.blocks": string;
  "wiz.recap.align": string;
  "wiz.recap.crop": string;
  "wiz.recap.global": string;
  "wiz.recap.on": string;
  "wiz.recap.off": string;
  "wiz.recap.auto": string;
  "wiz.back": string;
  "wiz.allSettings": string;
  "wiz.run": string;

  // --- Orientation chips ---
  "orient.90": string;
  "orient.180": string;
  "orient.flipH": string;
  "orient.flipV": string;
  "orient.reset": string;

  // --- Running ---
  "run.elapsed": string;
  "run.remaining": string;
  "run.frames": string;
  "run.note": string;
  "run.cancel": string;
  "run.cancelling": string;

  // --- Result ---
  "res.done": string;
  "res.saveAs": string;
  "res.openFolder": string;
  "res.reveal": string;
  "res.tab.final": string;
  "res.tab.raw": string;
  "res.tab.compare": string;
  "res.tab.rawDisabled": string;
  "res.hint.inspect": string;
  "res.rawLabel": string;
  "res.finalLabel": string;
  "res.previewPending": string;
  "res.previewPendingSub": string;
  "res.detailLoading": string;
  "res.detailTitle": string;
  "res.stat.frames": string;
  "res.stat.quality": string;
  "res.stat.crop": string;
  "res.stat.depth": string;
  "res.details": string;
  "res.path": string;
  "res.kept": string;
  "res.next": string;
  "res.nextNote": string;
  "res.skipped": string;
  "res.skippedNote": string;
  "res.skippedMore": string;
  "res.restart": string;
  "res.saveExists": string;

  // --- Settings drawer ---
  "set.title": string;
  "set.close": string;
  "set.oriented": string;
  "set.orientedDesc": string;
  "set.frames": string;
  "set.keepFraction": string;
  "set.blockSize": string;
  "set.blockOff": string;
  "set.blockNote": string;
  "set.crop": string;
  "set.cropAuto": string;
  "set.tileSelect": string;
  "set.tileSelectDesc": string;
  "set.tileSize": string;
  "set.enhance": string;
  "set.enhanceOn": string;
  "set.enhanceDesc": string;
  "set.waveletAmount": string;
  "set.waveletLevels": string;
  "set.clahe": string;
  "set.clarity": string;
  "set.gamma": string;
  "set.advanced": string;
  "set.align": string;
  "set.normalize": string;
  "set.weighted": string;
  "set.ap": string;
  "set.apDesc": string;
  "set.defaults": string;
  "set.apply": string;

  // --- Cache management ---
  "cache.title": string;
  "cache.size": string;
  "cache.frames": string;
  "cache.stale": string;
  "cache.clear": string;
  "cache.prune": string;
  "cache.confirmClear": string;
  "cache.freed": string;
  "cache.pruned": string;
  "cache.diskWarn": string;

  // --- Stage messages (from the backend codes) ---
  "stage.scanningFiles": string;
  "stage.detectingObject": string;
  "stage.scoringFrames": string;
  "stage.aligning": string;
  "stage.stacking": string;
  "stage.enhancing": string;
  "stage.writingOutput": string;

  // --- Skip reasons ---
  "skip.unreadable": string;
  "skip.missingCache": string;
  "skip.notEnough": string;

  // --- Fatal errors ---
  "err.noRawFiles": string;
  "err.noUsableFrames": string;
  "err.notEnoughFrames": string;
  "err.objectNotDetected": string;
  "err.cancelled": string;
  "err.other": string;
  "err.generic": string;

  // --- Error prefixes from backend commands ---
  "err.fileExists": string;
  "err.sourceMissing": string;
  "err.jobRunning": string;
  "err.notEnoughMemory": string;
};

const fr: Dict = {
  "app.brand": "Hyperion Studio",
  "app.tagline": "Observatoire · Empilement astro",
  "app.theme.toDark": "Passer en thème sombre",
  "app.theme.toLight": "Passer en thème clair",
  "app.theme.label": "Changer de thème",
  "app.lang.label": "Changer de langue",

  "wiz.step.folder": "Dossier",
  "wiz.step.folder.sub": "à choisir",
  "wiz.step.settings": "Réglages",
  "wiz.step.settings.sub": "à ajuster",
  "wiz.step.processing": "Traitement",
  "wiz.step.processing.sub": "puis résultat",
  "wiz.folder.title": "Quel dossier empiler ?",
  "wiz.folder.desc":
    "Choisissez un dossier de RAW (ARW, NEF, CR2…) ou de TIFF 16 bits déjà convertis. Tout le dossier sera traité comme une seule session.",
  "wiz.folder.pick": "Choisir un dossier d'images",
  "wiz.folder.picked": "Dossier sélectionné",
  "wiz.folder.hintClick": "Cliquez pour parcourir vos fichiers",
  "wiz.folder.scanning": "Analyse du dossier…",
  "wiz.folder.ready": "Dossier analysé, tout est prêt.",
  "wiz.folder.selectFirst": "Sélectionnez un dossier pour continuer.",
  "wiz.folder.noObject": "objet non détecté",
  "wiz.folder.objectDetected": "objet détecté",
  "wiz.continue": "Continuer",
  "wiz.settings.title": "Réglages de l'empilement",
  "wiz.settings.desc":
    "Tout fonctionne déjà sans rien toucher. Ajustez seulement si vous le souhaitez, ou ouvrez le volet pour les réglages avancés.",
  "wiz.settings.orientation": "Orientation",
  "wiz.settings.orientationHint": "l'EXIF ignore le retournement du télescope",
  "wiz.settings.post": "Post-traitement",
  "wiz.settings.postLabel": "Améliorer l'image finale",
  "wiz.settings.postDesc": "Netteté par ondelettes et contraste. Désactivé = stack brut.",
  "wiz.recap.keep": "Frames conservées",
  "wiz.recap.blocks": "Empilement par blocs",
  "wiz.recap.align": "Alignement",
  "wiz.recap.crop": "Crop",
  "wiz.recap.global": "global",
  "wiz.recap.on": "activé",
  "wiz.recap.off": "désactivé",
  "wiz.recap.auto": "automatique",
  "wiz.back": "Retour",
  "wiz.allSettings": "Tous les réglages",
  "wiz.run": "Lancer le traitement",

  "orient.90": "90°",
  "orient.180": "180°",
  "orient.flipH": "Miroir H",
  "orient.flipV": "Miroir V",
  "orient.reset": "Réinitialiser",

  "run.elapsed": "Temps écoulé",
  "run.remaining": "restant",
  "run.frames": "images",
  "run.note": "Vous pouvez laisser la fenêtre ouverte : le traitement continue tant que l'application est lancée.",
  "run.cancel": "Annuler",
  "run.cancelling": "Annulation…",

  "res.done": "Stack terminé",
  "res.saveAs": "Enregistrer sous",
  "res.openFolder": "Ouvrir le dossier",
  "res.reveal": "Révéler dans le Finder",
  "res.tab.final": "Image finale",
  "res.tab.raw": "Stack brut",
  "res.tab.compare": "Comparer",
  "res.tab.rawDisabled": "Post-traitement désactivé : le brut est identique.",
  "res.hint.inspect": "Cliquez pour inspecter à 100 %",
  "res.rawLabel": "Stack brut",
  "res.finalLabel": "Image finale",
  "res.previewPending": "Aperçu en cours de génération…",
  "res.previewPendingSub": "Le fichier TIFF est déjà enregistré sur le disque.",
  "res.detailLoading": "Chargement de la vue 1:1…",
  "res.detailTitle": "Vue 1:1 — netteté réelle",
  "res.stat.frames": "frames empilées",
  "res.stat.quality": "qualité moyenne",
  "res.stat.crop": "taille du crop",
  "res.stat.depth": "profondeur",
  "res.details": "Détails du traitement",
  "res.path": "Chemin",
  "res.kept": "Frames gardées",
  "res.next": "Et ensuite ?",
  "res.nextNote": "Le TIFF 16 bits est prêt pour Luminar, Photoshop ou GIMP. Pas de compression, pas de perte.",
  "res.skipped": "{count} frame(s) écartée(s)",
  "res.skippedNote":
    "Le reste de la session a été traité normalement. Ces fichiers sont probablement corrompus ou d'un format non pris en charge.",
  "res.skippedMore": "… et {count} autre(s).",
  "res.restart": "Traiter un autre dossier",
  "res.saveExists": "Un fichier porte déjà ce nom. Choisissez un autre nom.",

  "set.title": "Réglages",
  "set.close": "Fermer",
  "set.oriented": "Orientation",
  "set.orientedDesc": "Le capteur ne sait pas comment le télescope était retourné.",
  "set.frames": "Frames & empilement",
  "set.keepFraction": "Frames conservées",
  "set.blockSize": "Empilement par blocs",
  "set.blockOff": "désactivé",
  "set.blockNote":
    "Sur une longue session, la Lune tourne : empiler par petits groupes consécutifs évite le flou final.",
  "set.crop": "Taille du crop",
  "set.cropAuto": "auto",
  "set.tileSelect": "Sélection par zone",
  "set.tileSelectDesc":
    "Tire chaque région de l'image des frames qui y sont nettes, au lieu du seul tri global. Meilleur au terminateur et sur le limbe.",
  "set.tileSize": "Taille des zones",
  "set.enhance": "Amélioration",
  "set.enhanceOn": "Activer le post-traitement",
  "set.enhanceDesc": "Sans effet sur le TIFF brut si désactivé.",
  "set.waveletAmount": "Netteté (ondelettes)",
  "set.waveletLevels": "Échelles d'ondelettes",
  "set.clahe": "Contraste local (CLAHE)",
  "set.clarity": "Relief (clarté)",
  "set.gamma": "Gamma",
  "set.advanced": "Avancé",
  "set.align": "Aligner les frames",
  "set.normalize": "Normaliser la luminosité",
  "set.weighted": "Pondérer par la qualité",
  "set.ap": "Points d'alignement locaux",
  "set.apDesc": "Expérimental — dégrade l'image actuellement.",
  "set.defaults": "Valeurs par défaut",
  "set.apply": "Appliquer",

  "cache.title": "Cache disque",
  "cache.size": "Taille du cache",
  "cache.frames": "Frames en cache",
  "cache.stale": "Entrées orphelines",
  "cache.clear": "Vider le cache",
  "cache.prune": "Nettoyer les orphelines",
  "cache.confirmClear":
    "Supprimer le cache de ce dossier ? Les TIFF seront régénérés au prochain traitement.",
  "cache.freed": "{size} libérés.",
  "cache.pruned": "{count} entrée(s) orpheline(s) supprimée(s).",
  "cache.diskWarn":
    "Espace disque faible : environ {need} requis pour le cache, {free} disponible.",

  "stage.scanningFiles": "Recherche des fichiers RAW…",
  "stage.detectingObject": "Détection de l'objet…",
  "stage.scoringFrames": "Mesure de la netteté de chaque image…",
  "stage.aligning": "Alignement des images…",
  "stage.stacking": "Empilement par blocs…",
  "stage.enhancing": "Renforcement de la netteté et du contraste…",
  "stage.writingOutput": "Écriture du fichier final…",

  "skip.unreadable": "illisible",
  "skip.missingCache": "cache manquant",
  "skip.notEnough": "trop peu de frames",

  "err.noRawFiles": "Aucun fichier RAW trouvé dans ce dossier.",
  "err.noUsableFrames": "Aucune frame exploitable : tous les fichiers sont illisibles.",
  "err.notEnoughFrames": "Trop peu de frames exploitables ({found} sur {total}).",
  "err.objectNotDetected": "Objet non détecté. Vérifiez le cadrage ou la mise au point.",
  "err.cancelled": "Traitement annulé.",
  "err.other": "Erreur : {message}",
  "err.generic": "Erreur inconnue.",

  "err.fileExists": "Un fichier porte déjà ce nom. Choisissez un autre nom.",
  "err.sourceMissing": "Le stack source est introuvable.",
  "err.jobRunning": "Un traitement est déjà en cours.",
  "err.notEnoughMemory":
    "Mémoire insuffisante : environ {need} requis, {free} disponible. Réduisez le nombre de frames, la taille du crop, ou augmentez la taille des blocs.",
};

const en: Dict = {
  "app.brand": "Hyperion Studio",
  "app.tagline": "Observatory · Astro stacking",
  "app.theme.toDark": "Switch to dark theme",
  "app.theme.toLight": "Switch to light theme",
  "app.theme.label": "Change theme",
  "app.lang.label": "Change language",

  "wiz.step.folder": "Folder",
  "wiz.step.folder.sub": "to choose",
  "wiz.step.settings": "Settings",
  "wiz.step.settings.sub": "to adjust",
  "wiz.step.processing": "Processing",
  "wiz.step.processing.sub": "then result",
  "wiz.folder.title": "Which folder should be stacked?",
  "wiz.folder.desc":
    "Pick a folder of RAW files (ARW, NEF, CR2…) or already-converted 16-bit TIFFs. The whole folder is treated as a single session.",
  "wiz.folder.pick": "Choose an image folder",
  "wiz.folder.picked": "Folder selected",
  "wiz.folder.hintClick": "Click to browse your files",
  "wiz.folder.scanning": "Scanning folder…",
  "wiz.folder.ready": "Folder analysed, everything is ready.",
  "wiz.folder.selectFirst": "Select a folder to continue.",
  "wiz.folder.noObject": "object not detected",
  "wiz.folder.objectDetected": "object detected",
  "wiz.continue": "Continue",
  "wiz.settings.title": "Stacking settings",
  "wiz.settings.desc":
    "Everything already works as-is. Adjust only if you want to, or open the drawer for advanced settings.",
  "wiz.settings.orientation": "Orientation",
  "wiz.settings.orientationHint": "EXIF does not know how the telescope was flipped",
  "wiz.settings.post": "Post-processing",
  "wiz.settings.postLabel": "Enhance the final image",
  "wiz.settings.postDesc": "Wavelet sharpening and contrast. Off = raw stack.",
  "wiz.recap.keep": "Frames kept",
  "wiz.recap.blocks": "Blockwise stacking",
  "wiz.recap.align": "Alignment",
  "wiz.recap.crop": "Crop",
  "wiz.recap.global": "global",
  "wiz.recap.on": "on",
  "wiz.recap.off": "off",
  "wiz.recap.auto": "automatic",
  "wiz.back": "Back",
  "wiz.allSettings": "All settings",
  "wiz.run": "Start processing",

  "orient.90": "90°",
  "orient.180": "180°",
  "orient.flipH": "Flip H",
  "orient.flipV": "Flip V",
  "orient.reset": "Reset",

  "run.elapsed": "Elapsed",
  "run.remaining": "remaining",
  "run.frames": "frames",
  "run.note": "You can leave the window open: processing continues as long as the app is running.",
  "run.cancel": "Cancel",
  "run.cancelling": "Cancelling…",

  "res.done": "Stack complete",
  "res.saveAs": "Save as",
  "res.openFolder": "Open folder",
  "res.reveal": "Reveal in Finder",
  "res.tab.final": "Final image",
  "res.tab.raw": "Raw stack",
  "res.tab.compare": "Compare",
  "res.tab.rawDisabled": "Post-processing off: the raw stack is identical.",
  "res.hint.inspect": "Click to inspect at 100%",
  "res.rawLabel": "Raw stack",
  "res.finalLabel": "Final image",
  "res.previewPending": "Preview being generated…",
  "res.previewPendingSub": "The TIFF file is already saved on disk.",
  "res.detailLoading": "Loading the 1:1 view…",
  "res.detailTitle": "1:1 view — real sharpness",
  "res.stat.frames": "stacked frames",
  "res.stat.quality": "mean quality",
  "res.stat.crop": "crop size",
  "res.stat.depth": "depth",
  "res.details": "Processing details",
  "res.path": "Path",
  "res.kept": "Frames kept",
  "res.next": "What next?",
  "res.nextNote": "The 16-bit TIFF is ready for Luminar, Photoshop or GIMP. No compression, no loss.",
  "res.skipped": "{count} frame(s) skipped",
  "res.skippedNote":
    "The rest of the session was processed normally. These files are probably corrupt or in an unsupported format.",
  "res.skippedMore": "… and {count} more.",
  "res.restart": "Process another folder",
  "res.saveExists": "A file with this name already exists. Choose another name.",

  "set.title": "Settings",
  "set.close": "Close",
  "set.oriented": "Orientation",
  "set.orientedDesc": "The sensor does not know how the telescope was flipped.",
  "set.frames": "Frames & stacking",
  "set.keepFraction": "Frames kept",
  "set.blockSize": "Blockwise stacking",
  "set.blockOff": "disabled",
  "set.blockNote":
    "Over a long session the Moon rotates: stacking in small consecutive groups avoids the final blur.",
  "set.crop": "Crop size",
  "set.cropAuto": "auto",
  "set.tileSelect": "Per-region selection",
  "set.tileSelectDesc":
    "Draws each region of the image from the frames that are sharp there, instead of a single global ranking. Better at the terminator and the limb.",
  "set.tileSize": "Region size",
  "set.enhance": "Enhancement",
  "set.enhanceOn": "Enable post-processing",
  "set.enhanceDesc": "No effect on the raw TIFF when off.",
  "set.waveletAmount": "Sharpness (wavelets)",
  "set.waveletLevels": "Wavelet scales",
  "set.clahe": "Local contrast (CLAHE)",
  "set.clarity": "Relief (clarity)",
  "set.gamma": "Gamma",
  "set.advanced": "Advanced",
  "set.align": "Align frames",
  "set.normalize": "Normalise brightness",
  "set.weighted": "Weight by quality",
  "set.ap": "Local alignment points",
  "set.apDesc": "Experimental — currently degrades the image.",
  "set.defaults": "Defaults",
  "set.apply": "Apply",

  "cache.title": "Disk cache",
  "cache.size": "Cache size",
  "cache.frames": "Cached frames",
  "cache.stale": "Orphaned entries",
  "cache.clear": "Clear cache",
  "cache.prune": "Clean orphans",
  "cache.confirmClear":
    "Delete the cache for this folder? The TIFFs will be rebuilt on the next run.",
  "cache.freed": "{size} freed.",
  "cache.pruned": "{count} orphaned entry/entries removed.",
  "cache.diskWarn":
    "Low disk space: about {need} needed for the cache, {free} available.",

  "stage.scanningFiles": "Looking for RAW files…",
  "stage.detectingObject": "Detecting the object…",
  "stage.scoringFrames": "Measuring the sharpness of each frame…",
  "stage.aligning": "Aligning frames…",
  "stage.stacking": "Blockwise stacking…",
  "stage.enhancing": "Sharpening and boosting contrast…",
  "stage.writingOutput": "Writing the final file…",

  "skip.unreadable": "unreadable",
  "skip.missingCache": "missing cache",
  "skip.notEnough": "too few frames",

  "err.noRawFiles": "No RAW file found in this folder.",
  "err.noUsableFrames": "No usable frame: every file is unreadable.",
  "err.notEnoughFrames": "Too few usable frames ({found} out of {total}).",
  "err.objectNotDetected": "Object not detected. Check framing or focus.",
  "err.cancelled": "Processing cancelled.",
  "err.other": "Error: {message}",
  "err.generic": "Unknown error.",

  "err.fileExists": "A file with this name already exists. Choose another name.",
  "err.sourceMissing": "The source stack could not be found.",
  "err.jobRunning": "A job is already running.",
  "err.notEnoughMemory":
    "Not enough memory: about {need} needed, {free} available. Reduce the frame count or the crop size, or increase the block size.",
};

const DICTS: Record<Lang, Dict> = { fr, en };

/// Language set available in the UI.
export const LANGS: { code: Lang; label: string }[] = [
  { code: "fr", label: "Français" },
  { code: "en", label: "English" },
];

/// Controls the app language, persisted across launches.
export function useI18n() {
  const [lang, setLangState] = useState<Lang>(() => {
    if (typeof window === "undefined") return "fr";
    const saved = window.localStorage.getItem(STORAGE_KEY);
    if (saved === "fr" || saved === "en") return saved;
    // Default to the browser language, falling back to French.
    return navigator.language?.toLowerCase().startsWith("en") ? "en" : "fr";
  });

  useEffect(() => {
    document.documentElement.setAttribute("lang", lang);
    try {
      window.localStorage.setItem(STORAGE_KEY, lang);
    } catch {
      // Storage unavailable (private mode): the language stays for the session.
    }
  }, [lang]);

  const setLang = useCallback((l: Lang) => setLangState(l), []);

  /// Translates a key, interpolating `{name}` placeholders.
  const t = useCallback(
    (key: keyof Dict, params?: Record<string, string | number>) => {
      let s = DICTS[lang][key] ?? String(key);
      if (params) {
        for (const [k, v] of Object.entries(params)) {
          s = s.split(`{${k}}`).join(String(v));
        }
      }
      return s;
    },
    [lang],
  );

  return { lang, setLang, t };
}

export type Translate = ReturnType<typeof useI18n>["t"];

/// Maps a backend `Stage` code to a localized sentence.
export function stageText(t: Translate, stage: Stage): string {
  const map: Record<Stage, Parameters<Translate>[0]> = {
    scanningFiles: "stage.scanningFiles",
    detectingObject: "stage.detectingObject",
    scoringFrames: "stage.scoringFrames",
    aligning: "stage.aligning",
    stacking: "stage.stacking",
    enhancing: "stage.enhancing",
    writingOutput: "stage.writingOutput",
  };
  return t(map[stage] ?? "stage.stacking");
}

/// Maps a skip reason code to a localized sentence.
export function skipText(t: Translate, reason: SkipReason): string {
  if (typeof reason === "string") {
    return reason === "missingCache" ? t("skip.missingCache") : t("skip.unreadable");
  }
  if ("notEnoughFrames" in reason) {
    return t("skip.notEnough", {
      found: reason.notEnoughFrames.found,
      total: reason.notEnoughFrames.total,
    });
  }
  return t("skip.unreadable");
}

/// Maps a fatal error code to a localized sentence.
export function errorText(t: Translate, err: JobError | string): string {
  if (typeof err === "string") {
    // Raw message from a Tauri command: match a few known ones, else show it.
    if (err.includes("file already exists")) return t("err.fileExists");
    if (err.includes("source stack not found")) return t("err.sourceMissing");
    if (err.includes("already running")) return t("err.jobRunning");
    if (err.includes("__cancelled__")) return t("err.cancelled");
    return err;
  }
  if ("message" in err) return t("err.other", { message: err.message });
  if ("found" in err) {
    return t("err.notEnoughFrames", { found: err.found, total: err.total });
  }
  switch (err.kind) {
    case "noRawFiles":
      return t("err.noRawFiles");
    case "noUsableFrames":
      return t("err.noUsableFrames");
    case "objectNotDetected":
      return t("err.objectNotDetected");
    case "cancelled":
      return t("err.cancelled");
    default:
      return t("err.generic");
  }
}

/// Formats a byte count for display (e.g. "29,4 Go" / "29.4 GB").
export function formatBytes(bytes: number, lang: Lang): string {
  const units = lang === "fr" ? ["o", "Ko", "Mo", "Go", "To"] : ["B", "KB", "MB", "GB", "TB"];
  if (bytes <= 0) return `0 ${units[0]}`;
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const v = bytes / Math.pow(1024, i);
  const s = v >= 10 || i === 0 ? Math.round(v).toString() : v.toFixed(1).replace(".", lang === "fr" ? "," : ".");
  return `${s} ${units[i]}`;
}
