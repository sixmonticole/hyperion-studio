import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { listen } from "@tauri-apps/api/event";
import {
  DEFAULT_SETTINGS,
  mergeSettings,
  type Phase,
  type Progress,
  type ScanResult,
  type Settings,
  type SkippedFrame,
  type StackReport,
} from "./types";
import { Icon } from "./Icon";
import { Logo } from "./Logo";
import { Toggle } from "./Toggle";
import { SettingsDrawer } from "./SettingsDrawer";
import { Running } from "./Running";
import { Result } from "./Result";
import { useTheme } from "./useTheme";
import "./App.css";

/// Détecte si l'on tourne dans une fenêtre Tauri (l'IPC est sinon absent,
/// ce qui permet de prévisualiser l'UI dans un navigateur).
const IN_TAURI = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/// Mode démo : permet de visualiser les écrans (traitement, résultat) dans un
/// navigateur, sans backend. Actif seulement en développement, via l'URL :
///   ?demo=running   -> simule un empilement en cours
///   ?demo=result    -> affiche un écran de résultat factice
/// Aucun effet dans l'application empaquetée.
const DEMO: "off" | "running" | "result" = (() => {
  if (!import.meta.env.DEV || typeof window === "undefined") return "off";
  const d = new URLSearchParams(window.location.search).get("demo");
  return d === "running" || d === "result" ? d : "off";
})();

const DEMO_REPORT: StackReport = {
  sourceDir: "/Users/simon/Documents/Photo/lune 25 oct",
  outputPath: "/Users/simon/Documents/Photo/lune 25 oct/hyperion_out/lune 25 oct_stack.tif",
  rawOutputPath:
    "/Users/simon/Documents/Photo/lune 25 oct/hyperion_out/lune 25 oct_stack_brut.tif",
  totalFrames: 200,
  keptFrames: 200,
  meanQuality: 0.26,
  crop: { x: 120, y: 140, width: 3620, height: 3620 },
};

/// Aperçu factice : un disque lunaire dessiné en SVG, encodé en data URL.
/// On utilise des couleurs littérales (pas de `#` dans le SVG) pour éviter
/// tout double-encodage.
const DEMO_PREVIEW = `data:image/svg+xml;utf8,${encodeURIComponent(`
<svg xmlns='http://www.w3.org/2000/svg' width='900' height='620'>
  <defs>
    <radialGradient id='bg' cx='50%' cy='45%'><stop offset='0%' stop-color='rgb(17,24,38)'/><stop offset='100%' stop-color='rgb(5,8,15)'/></radialGradient>
    <radialGradient id='m' cx='44%' cy='38%'><stop offset='0%' stop-color='rgb(242,238,230)'/><stop offset='62%' stop-color='rgb(168,162,148)'/><stop offset='100%' stop-color='rgb(74,68,54)'/></radialGradient>
  </defs>
  <rect width='900' height='620' fill='url(#bg)'/>
  <circle cx='450' cy='300' r='215' fill='url(#m)'/>
  <circle cx='392' cy='246' r='34' fill='rgb(205,199,182)' opacity='.5'/>
  <circle cx='504' cy='330' r='22' fill='rgb(195,189,173)' opacity='.45'/>
  <circle cx='450' cy='150' r='15' fill='rgb(221,216,202)' opacity='.42'/>
  <circle cx='360' cy='392' r='12' fill='rgb(184,178,163)' opacity='.4'/>
</svg>`)}`;

function App() {
  // --- État du dossier et du scan ---
  const [sourceDir, setSourceDir] = useState("");
  const [scan, setScan] = useState<ScanResult | null>(null);
  const [scanning, setScanning] = useState(false);

  // --- État du traitement ---
  const [phase, setPhase] = useState<Phase>("idle");
  const [message, setMessage] = useState("");
  const [done, setDone] = useState(0);
  const [total, setTotal] = useState(0);
  const [report, setReport] = useState<StackReport | null>(null);
  const [error, setError] = useState("");
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  // Frames écartées pendant le traitement (illisibles, corrompues...).
  const [warnings, setWarnings] = useState<SkippedFrame[]>([]);

  // --- État de l'interface ---
  const [step, setStep] = useState<0 | 1>(0); // 0 = dossier, 1 = réglages
  const [drawerOpen, setDrawerOpen] = useState(false);
  const { theme, toggle: toggleTheme } = useTheme();

  // --- Aperçus ---
  const [preview, setPreview] = useState("");
  const [rawPreview, setRawPreview] = useState("");
  const [detail, setDetail] = useState("");
  const [detailLoading, setDetailLoading] = useState(false);

  // --- Chronomètre ---
  const [elapsed, setElapsed] = useState(0);
  const startedAt = useRef<number | null>(null);

  const running =
    phase === "importing" ||
    phase === "analyzing" ||
    phase === "stacking" ||
    phase === "enhancing";

  // Chronomètre pendant le traitement.
  useEffect(() => {
    if (!running) {
      startedAt.current = null;
      return;
    }
    startedAt.current = Date.now();
    setElapsed(0);
    const id = window.setInterval(() => {
      if (startedAt.current) setElapsed((Date.now() - startedAt.current) / 1000);
    }, 500);
    return () => window.clearInterval(id);
  }, [running]);

  // --- Mode démo (développement uniquement) --------------------------------
  // Simule un empilement pour visualiser les écrans sans backend.
  useEffect(() => {
    if (DEMO === "off") return;

    if (DEMO === "result") {
      setSourceDir(DEMO_REPORT.sourceDir);
      setScan({
        count: DEMO_REPORT.totalFrames,
        path: DEMO_REPORT.sourceDir,
        cached: DEMO_REPORT.totalFrames,
        preview: { model: "ILCE-6000", width: 6000, height: 4000, planetDetected: true, planetRadius: 1240 },
      });
      setReport(DEMO_REPORT);
      setPreview(DEMO_PREVIEW);
      setRawPreview(DEMO_PREVIEW);
      setDetail(DEMO_PREVIEW);
      setPhase("done");
      return;
    }

    // DEMO === "running" : on déroule les phases puis on termine.
    // Le compteur `done` suit l'étape courante, comme dans un vrai traitement :
    // il repart à zéro au début de chaque étape et atteint le total à la fin.
    setSourceDir(DEMO_REPORT.sourceDir);
    setScan({
      count: 200,
      path: DEMO_REPORT.sourceDir,
      cached: 0,
      preview: { model: "ILCE-6000", width: 6000, height: 4000, planetDetected: true, planetRadius: 1240 },
    });
    setTotal(200);

    let interval = 0;
    // Fait grimper `done` vers `target` pendant `durationMs`, puis enchaîne.
    const runPhase = (
      phase: Phase,
      message: string,
      from: number,
      target: number,
      durationMs: number,
      next: () => void,
    ) => {
      setPhase(phase);
      setMessage(message);
      setDone(from);
      const start = Date.now();
      window.clearInterval(interval);
      interval = window.setInterval(() => {
        const t = Math.min(1, (Date.now() - start) / durationMs);
        setDone(Math.round(from + (target - from) * t));
        if (t >= 1) {
          window.clearInterval(interval);
          next();
        }
      }, 80);
    };

    const timers: number[] = [];
    const afterEnhance = () => {
      timers.push(
        window.setTimeout(() => {
          setDone(0);
          runPhase("done", "Écriture du fichier final…", 0, 200, 700, () => {});
          window.setTimeout(() => {
            setReport(DEMO_REPORT);
            setPreview(DEMO_PREVIEW);
            setRawPreview(DEMO_PREVIEW);
          }, 700);
        }, 900),
      );
    };

    const afterStack = () =>
      runPhase("enhancing", "Renforcement de la netteté et du contraste…", 0, 0, 1400, afterEnhance);

    // L'analyse puis l'empilement : le compteur avance sur les images.
    const afterAnalyze = () =>
      runPhase("stacking", "Empilement par blocs : 11 blocs de 10 frames…", 0, 200, 3200, afterStack);

    runPhase("importing", "Décodage des RAW vers le cache 16 bits…", 0, 200, 2200, afterAnalyze);

    return () => {
      window.clearInterval(interval);
      timers.forEach(window.clearTimeout);
    };
  }, []);

  // Écoute des événements de progression.
  useEffect(() => {
    if (!IN_TAURI) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    listen<Progress>("hyperion://progress", (event) => {
      const p = event.payload;
      if (p.kind === "phase" && p.phase) {
        setPhase(p.phase);
        setMessage(p.message ?? "");
      } else if (p.kind === "import" || p.kind === "analyze" || p.kind === "stack") {
        setDone(p.done ?? 0);
        setTotal(p.total ?? 0);
      } else if (p.kind === "warning" && p.name) {
        // Une frame écartée : on la mémorise sans interrompre le traitement.
        const name = p.name;
        setWarnings((w) => [...w, { name, reason: p.reason ?? "illisible" }]);
      } else if (p.kind === "finished" && p.report) {
        setReport(p.report);
        setPhase("done");
        setMessage("Terminé");
      } else if (p.kind === "error") {
        setError(p.message ?? "Erreur inconnue");
        setPhase("failed");
      }
    })
      .then((f) => {
        if (disposed) f();
        else unlisten = f;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  // Charge un aperçu du stack (image entière ou zone 1:1).
  const loadPreview = useCallback(
    async (
      tifPath: string,
      kind: "full" | "detail" | "raw",
      crop?: { x: number; y: number; size: number },
    ) => {
      if (!IN_TAURI) return;
      if (kind === "detail") setDetailLoading(true);
      try {
        const url = await invoke<string>("preview_image", {
          path: tifPath,
          maxSize: kind === "detail" ? 900 : 1500,
          crop: crop ?? null,
        });
        if (kind === "detail") setDetail(url);
        else if (kind === "raw") setRawPreview(url);
        else setPreview(url);
      } catch (e) {
        setError(String(e));
      } finally {
        if (kind === "detail") setDetailLoading(false);
      }
    },
    [],
  );

  const pickFolder = useCallback(async () => {
    setError("");
    setReport(null);
    setPreview("");
    setRawPreview("");
    setDetail("");
    if (!IN_TAURI) {
      setError("Sélection de dossier indisponible hors de l'application.");
      return;
    }
    let selected: string | string[] | null = null;
    try {
      selected = await open({ directory: true, multiple: false });
    } catch (e) {
      setError(String(e));
      return;
    }
    if (typeof selected !== "string") return;
    setSourceDir(selected);
    setPhase("idle");
    setScan(null);
    setScanning(true);
    try {
      const res = await invoke<ScanResult>("scan_folder", { path: selected });
      setScan(res);
      if (res.preview?.error) setError(res.preview.error);
      setStep(1);
    } catch (e) {
      setError(String(e));
    } finally {
      setScanning(false);
    }
  }, []);

  const runStack = useCallback(async () => {
    if (!sourceDir) return;
    setError("");
    setReport(null);
    setWarnings([]);
    setPreview("");
    setRawPreview("");
    setDetail("");
    setDone(0);
    setTotal(scan?.count ?? 0);
    setDrawerOpen(false);
    setPhase("importing");
    setMessage("Préparation...");
    try {
      const res = await invoke<StackReport>("run_stack", {
        sourceDir,
        cacheDir: `${sourceDir}/.hyperion_cache`,
        settings: mergeSettings(settings),
      });
      setReport(res);
      setPhase("done");
      loadPreview(res.outputPath, "full");
      // Si un stack brut existe, on charge aussi son aperçu pour l'onglet
      // de comparaison.
      if (res.rawOutputPath) loadPreview(res.rawOutputPath, "raw");
    } catch (e) {
      setError(String(e));
      setPhase("failed");
    }
  }, [sourceDir, settings, scan, loadPreview]);

  const pickDetail = useCallback(
    (evt: React.MouseEvent<HTMLImageElement>) => {
      if (!report) return;
      const img = evt.currentTarget;
      const rect = img.getBoundingClientRect();
      const fx = (evt.clientX - rect.left) / rect.width;
      const fy = (evt.clientY - rect.top) / rect.height;
      const size = Math.min(report.crop?.width ?? 2000, report.crop?.height ?? 2000);
      const x = Math.max(0, Math.round(fx * size - 300));
      const y = Math.max(0, Math.round(fy * size - 300));
      loadPreview(report.outputPath, "detail", { x, y, size: 600 });
    },
    [report, loadPreview],
  );

  const saveAs = useCallback(async () => {
    if (!report || !IN_TAURI) return;
    try {
      const base = report.outputPath.split("/").pop() ?? "stack.tif";
      // On propose un nom libre par défaut : le dossier de sortie contient
      // déjà le stack, et le backend refuse d'écraser un fichier existant.
      const dest = await save({
        defaultPath: base.replace(/\.tiff?$/i, "_copie.tif"),
        filters: [{ name: "TIFF 16 bits", extensions: ["tif", "tiff"] }],
      });
      if (typeof dest !== "string") return;
      await invoke("save_copy", { from: report.outputPath, to: dest });
      setError("");
    } catch (e) {
      const msg = String(e);
      // Le backend refuse l'écrasement : on explique quoi faire.
      if (msg.includes("existe deja")) {
        setError("Un fichier porte déjà ce nom. Choisissez un autre nom.");
      } else {
        setError(msg);
      }
    }
  }, [report]);

  const openOutputFolder = useCallback(async () => {
    if (!report || !IN_TAURI) return;
    try {
      await revealItemInDir(report.outputPath);
    } catch (e) {
      setError(String(e));
    }
  }, [report]);

  // Un nouveau traitement remet le parcours au début.
  const restart = useCallback(() => {
    setReport(null);
    setWarnings([]);
    setPreview("");
    setRawPreview("");
    setDetail("");
    setPhase("idle");
    setError("");
    setStep(0);
    setSourceDir("");
    setScan(null);
  }, []);

  // --- Rendu -------------------------------------------------------------
  const inProgress = running || phase === "failed";
  const finished = report && phase === "done";

  return (
    <div className="app">
      <header className="header">
        <div className="brand">
          <div className="logo">
            <Logo size={44} />
          </div>
          <div className="brand-text">
            <h1>Hyperion Studio</h1>
            <div className="brand-sub">Observatoire · Empilement astro</div>
          </div>
        </div>
        <div className="header-right">
          {scan && !inProgress && !finished && (
            <div className="header-meta">
              <span className="badge">{scan.count} images</span>
              {scan.cached > 0 && <span className="badge info">cache prêt</span>}
            </div>
          )}
          {finished && (
            <span className="badge ok">
              <Icon name="check" size={12} /> terminé
            </span>
          )}
          <button
            className="theme-toggle"
            onClick={toggleTheme}
            title={theme === "dark" ? "Passer en thème clair" : "Passer en thème sombre"}
            aria-label="Changer de thème"
          >
            <Icon name={theme === "dark" ? "sun" : "moon"} size={16} />
          </button>
        </div>
      </header>

      {error && (
        <section className="panel error" style={{ marginBottom: 18 }}>
          <Icon name="alert" size={16} />
          <span>{error}</span>
        </section>
      )}

      {/* --- Écran 1 : traitement en cours --- */}
      {inProgress && (
        <Running phase={phase} message={message} done={done} total={total} elapsed={elapsed} />
      )}

      {/* --- Écran 2 : résultat --- */}
      {finished && (
        <Result
          report={report}
          finalPreview={preview}
          rawPreview={rawPreview}
          detail={detail}
          detailLoading={detailLoading}
          skipped={warnings}
          onInspect={pickDetail}
          onSaveAs={saveAs}
          onOpenFolder={openOutputFolder}
        />
      )}

      {/* --- Écran 3 : assistant pas à pas --- */}
      {!inProgress && !finished && (
        <div className="wizard">
          <div className="wiz-steps">
            <div className={`wiz-step ${step === 0 ? "active" : "done"}`}>
              <span className="bullet">
                {step === 0 ? "01" : <Icon name="check" size={14} />}
              </span>
              <span className="lbl">
                <b>Dossier</b>
                <span>
                  {scan ? `${scan.count} images` : sourceDir ? "—" : "à choisir"}
                </span>
              </span>
              <span className="rail" />
            </div>
            <div className={`wiz-step ${step === 1 ? "active" : "pending"}`}>
              <span className="bullet">02</span>
              <span className="lbl">
                <b>Réglages</b>
                <span>à ajuster</span>
              </span>
              <span className="rail" />
            </div>
            <div className="wiz-step">
              <span className="bullet">03</span>
              <span className="lbl">
                <b>Traitement</b>
                <span>puis résultat</span>
              </span>
            </div>
          </div>

          {/* Étape 1 : choisir le dossier */}
          {step === 0 && (
            <div className="wiz-body">
              <div className="wiz-title">Quel dossier empiler ?</div>
              <p className="wiz-desc">
                Choisissez un dossier de RAW (ARW, NEF, CR2…) ou de TIFF 16 bits déjà convertis.
                Tout le dossier sera traité comme une seule session.
              </p>

              <section className={`drop ${sourceDir ? "filled" : ""}`} onClick={pickFolder}>
                <div className="drop-icon">
                  <Icon name="folder" size={30} />
                </div>
                <h2>{sourceDir ? "Dossier sélectionné" : "Choisir un dossier d'images"}</h2>
                <p className="path">
                  {sourceDir || "Cliquez pour parcourir vos fichiers"}
                </p>
                {scanning && <p className="muted">Analyse du dossier…</p>}
                {scan && (
                  <div className="scan-meta">
                    {scan.preview?.model && (
                      <span className="badge">
                        <Icon name="camera" size={12} /> {scan.preview.model}
                      </span>
                    )}
                    {scan.preview?.width && (
                      <span className="badge">
                        {scan.preview.width}×{scan.preview.height}
                      </span>
                    )}
                    {scan.preview?.planetDetected ? (
                      <span className="badge ok">
                        <Icon name="check" size={12} /> objet détecté (~
                        {Math.round(scan.preview.planetRadius ?? 0)} px)
                      </span>
                    ) : (
                      scan.preview && (
                        <span className="badge warn">
                          <Icon name="alert" size={12} /> objet non détecté
                        </span>
                      )
                    )}
                  </div>
                )}
              </section>

              <div className="wiz-foot">
                <span className="hint">
                  {scan ? "Dossier analysé, tout est prêt." : "Sélectionnez un dossier pour continuer."}
                </span>
                <button className="btn primary" disabled={!sourceDir} onClick={() => setStep(1)}>
                  Continuer <Icon name="arrow-right" size={15} />
                </button>
              </div>
            </div>
          )}

          {/* Étape 2 : réglages */}
          {step === 1 && (
            <div className="wiz-body">
              <div className="wiz-title">Réglages de l'empilement</div>
              <p className="wiz-desc">
                Tout fonctionne déjà sans rien toucher. Ajustez seulement si vous le souhaitez, ou
                ouvrez le volet pour les réglages avancés.
              </p>

              <div className="group" style={{ marginTop: 0, borderTop: "none", paddingTop: 0 }}>
                <div className="group-head">
                  <span className="group-title">Orientation</span>
                  <span className="hint">l'EXIF ignore le retournement du télescope</span>
                </div>
                <div className="chips">
                  <button
                    className="chip"
                    onClick={() =>
                      setSettings((s) => ({
                        ...s,
                        orientation: { ...s.orientation, rotate: (s.orientation.rotate + 90) % 360 },
                      }))
                    }
                  >
                    <Icon name="rotate" size={13} /> 90°
                  </button>
                  <button
                    className="chip"
                    onClick={() =>
                      setSettings((s) => ({
                        ...s,
                        orientation: {
                          ...s.orientation,
                          rotate: (s.orientation.rotate + 180) % 360,
                        },
                      }))
                    }
                  >
                    <Icon name="rotate" size={13} /> 180°
                  </button>
                  <button
                    className={`chip ${settings.orientation.flipH ? "on" : ""}`}
                    onClick={() =>
                      setSettings((s) => ({
                        ...s,
                        orientation: { ...s.orientation, flipH: !s.orientation.flipH },
                      }))
                    }
                  >
                    <Icon name="flip-h" size={13} /> Miroir H
                  </button>
                  <button
                    className={`chip ${settings.orientation.flipV ? "on" : ""}`}
                    onClick={() =>
                      setSettings((s) => ({
                        ...s,
                        orientation: { ...s.orientation, flipV: !s.orientation.flipV },
                      }))
                    }
                  >
                    <Icon name="flip-v" size={13} /> Miroir V
                  </button>
                  <button
                    className="chip"
                    onClick={() =>
                      setSettings((s) => ({
                        ...s,
                        orientation: { rotate: 0, flipH: false, flipV: false },
                      }))
                    }
                  >
                    <Icon name="reset" size={13} /> Réinitialiser
                  </button>
                </div>
              </div>

              <div className="group">
                <div className="group-head">
                  <span className="group-title">Post-traitement</span>
                </div>
                <Toggle
                  checked={settings.enhance.enabled}
                  onChange={(v) =>
                    setSettings((s) => ({ ...s, enhance: { ...s.enhance, enabled: v } }))
                  }
                  label="Améliorer l'image finale"
                  description="Netteté par ondelettes et contraste. Désactivé = stack brut."
                />
              </div>

              <div className="recap">
                <div className="recap-row">
                  <span>Frames conservées</span>
                  <b>{Math.round(settings.keepFraction * 100)} %</b>
                </div>
                <div className="recap-row">
                  <span>Empilement par blocs</span>
                  <b>{settings.blockSize > 0 ? `${settings.blockSize} frames` : "global"}</b>
                </div>
                <div className="recap-row">
                  <span>Alignement</span>
                  <b>{settings.align ? "activé" : "désactivé"}</b>
                </div>
                <div className="recap-row">
                  <span>Crop</span>
                  <b>{settings.cropSize === 0 ? "automatique" : `${settings.cropSize} px`}</b>
                </div>
              </div>

              <div className="wiz-foot">
                <button className="tbtn" onClick={() => setStep(0)}>
                  <Icon name="arrow-left" size={15} /> Retour
                </button>
                <div className="actions">
                  <button className="tbtn" onClick={() => setDrawerOpen(true)}>
                    <Icon name="sliders" size={15} /> Tous les réglages
                  </button>
                  <button className="btn primary" onClick={runStack}>
                    <Icon name="play" size={15} /> Lancer le traitement
                  </button>
                </div>
              </div>
            </div>
          )}
        </div>
      )}

      {/* --- Volet de réglages --- */}
      {drawerOpen && (
        <SettingsDrawer
          settings={settings}
          onChange={setSettings}
          onClose={() => setDrawerOpen(false)}
        />
      )}

      {/* Après un résultat, permet de recommencer. */}
      {finished && (
        <div style={{ marginTop: 24, display: "flex", justifyContent: "center" }}>
          <button className="tbtn" onClick={restart}>
            <Icon name="reset" size={15} /> Traiter un autre dossier
          </button>
        </div>
      )}
    </div>
  );
}

export default App;
