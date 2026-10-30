import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { listen } from "@tauri-apps/api/event";
import {
  DEFAULT_SETTINGS,
  mergeSettings,
  type CacheInfo,
  type MemoryInfo,
  type Phase,
  type Progress,
  type ScanResult,
  type Settings,
  type SkippedFrame,
  type StackReport,
  type Stage,
} from "./types";
import { Icon } from "./Icon";
import { Logo } from "./Logo";
import { Toggle } from "./Toggle";
import { SettingsDrawer } from "./SettingsDrawer";
import { Running } from "./Running";
import { Result } from "./Result";
import { useTheme } from "./useTheme";
import { errorText, formatBytes, LANGS, useI18n, type Lang } from "./i18n";
import "./App.css";

/// Whether we are running inside a Tauri window (IPC is otherwise absent,
/// which allows previewing the UI in a browser).
const IN_TAURI = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/// Demo mode: lets the screens (running, result) be shown in a browser without
/// a backend. Active in development only, via the URL:
///   ?demo=running   -> simulates a run in progress
///   ?demo=result    -> shows a fake result screen
/// No effect in the packaged app.
const DEMO: "off" | "running" | "result" = (() => {
  if (!import.meta.env.DEV || typeof window === "undefined") return "off";
  const d = new URLSearchParams(window.location.search).get("demo");
  return d === "running" || d === "result" ? d : "off";
})();

const DEMO_REPORT: StackReport = {
  sourceDir: "/Users/simon/Pictures/Moon session",
  outputPath: "/Users/simon/Pictures/Moon session/hyperion_out/moon session_stack.tif",
  rawOutputPath: "/Users/simon/Pictures/Moon session/hyperion_out/moon session_stack_raw.tif",
  totalFrames: 200,
  keptFrames: 200,
  meanQuality: 0.26,
  crop: { x: 120, y: 140, width: 3620, height: 3620 },
};

/// Fake preview: a lunar disc drawn in SVG, encoded as a data URL.
/// Colors are literal (no `#` in the SVG) to avoid double-encoding.
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

const SETTINGS_KEY = "hyperion:settings";

/// Loads persisted settings, merged over the defaults so a partial or older
/// saved object can never break the app.
function loadSettings(): Settings {
  try {
    const raw = window.localStorage.getItem(SETTINGS_KEY);
    if (!raw) return DEFAULT_SETTINGS;
    return mergeSettings(JSON.parse(raw) as Partial<Settings>);
  } catch {
    return DEFAULT_SETTINGS;
  }
}

function App() {
  // --- Folder & scan state ---
  const [sourceDir, setSourceDir] = useState("");
  const [scan, setScan] = useState<ScanResult | null>(null);
  const [scanning, setScanning] = useState(false);
  const [cacheInfo, setCacheInfo] = useState<CacheInfo | null>(null);
  const [cacheMsg, setCacheMsg] = useState("");

  // --- Run state ---
  const [phase, setPhase] = useState<Phase>("idle");
  const [stage, setStage] = useState<Stage>("scanningFiles");
  const [done, setDone] = useState(0);
  const [total, setTotal] = useState(0);
  const [report, setReport] = useState<StackReport | null>(null);
  const [error, setError] = useState("");
  const [cancelling, setCancelling] = useState(false);
  const [settings, setSettings] = useState<Settings>(loadSettings);
  // Frames skipped during processing (unreadable, corrupt...).
  const [warnings, setWarnings] = useState<SkippedFrame[]>([]);

  // --- UI state ---
  const [step, setStep] = useState<0 | 1>(0); // 0 = folder, 1 = settings
  const [drawerOpen, setDrawerOpen] = useState(false);
  const { theme, toggle: toggleTheme } = useTheme();
  const { lang, setLang, t } = useI18n();

  // --- Previews ---
  const [preview, setPreview] = useState("");
  const [rawPreview, setRawPreview] = useState("");
  const [detail, setDetail] = useState("");
  const [detailLoading, setDetailLoading] = useState(false);
  const [savedAs, setSavedAs] = useState("");

  // --- Timer ---
  const [elapsed, setElapsed] = useState(0);
  const startedAt = useRef<number | null>(null);

  const running =
    phase === "importing" ||
    phase === "analyzing" ||
    phase === "stacking" ||
    phase === "enhancing";

  // Persist settings whenever they change.
  useEffect(() => {
    try {
      window.localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
    } catch {
      // Storage unavailable: settings stay for the session only.
    }
  }, [settings]);

  // Timer during processing.
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

  // --- Demo mode (development only) ----------------------------------------
  // Simulates a run to show the screens without a backend.
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

    // DEMO === "running": play the phases then finish. The counter follows the
    // current stage, as in a real run: it resets at each stage start and
    // reaches the total at its end.
    setSourceDir(DEMO_REPORT.sourceDir);
    setScan({
      count: 200,
      path: DEMO_REPORT.sourceDir,
      cached: 0,
      preview: { model: "ILCE-6000", width: 6000, height: 4000, planetDetected: true, planetRadius: 1240 },
    });
    setTotal(200);

    let interval = 0;
    // Ramps `done` toward `target` over `durationMs`, then chains on.
    const runPhase = (
      ph: Phase,
      st: Stage,
      from: number,
      target: number,
      durationMs: number,
      next: () => void,
    ) => {
      setPhase(ph);
      setStage(st);
      setDone(from);
      const start = Date.now();
      window.clearInterval(interval);
      interval = window.setInterval(() => {
        const p = Math.min(1, (Date.now() - start) / durationMs);
        setDone(Math.round(from + (target - from) * p));
        if (p >= 1) {
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
          runPhase("done", "writingOutput", 0, 200, 700, () => {});
          window.setTimeout(() => {
            setReport(DEMO_REPORT);
            setPreview(DEMO_PREVIEW);
            setRawPreview(DEMO_PREVIEW);
          }, 700);
        }, 900),
      );
    };

    const afterStack = () => runPhase("enhancing", "enhancing", 0, 0, 1400, afterEnhance);
    const afterAnalyze = () => runPhase("stacking", "stacking", 0, 200, 3200, afterStack);

    runPhase("importing", "scanningFiles", 0, 200, 2200, afterAnalyze);

    return () => {
      window.clearInterval(interval);
      timers.forEach(window.clearTimeout);
    };
  }, []);

  // Progress events.
  useEffect(() => {
    if (!IN_TAURI) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    listen<Progress>("hyperion://progress", (event) => {
      const p = event.payload;
      if (p.kind === "phase" && p.phase) {
        setPhase(p.phase);
        if (p.stage) setStage(p.stage);
      } else if (p.kind === "import" || p.kind === "analyze" || p.kind === "stack") {
        setDone(p.done ?? 0);
        setTotal(p.total ?? 0);
      } else if (p.kind === "warning" && p.name) {
        const name = p.name;
        const reason = p.reason ?? "unreadable";
        setWarnings((w) => [...w, { name, reason }]);
      } else if (p.kind === "notice" && p.notice?.kind === "subsampled") {
        const { total, kept } = p.notice.detail;
        setCacheMsg(t("notice.subsampled", { total, kept }));
      } else if (p.kind === "finished" && p.report) {
        setReport(p.report);
        setPhase("done");
        setStage("writingOutput");
      } else if (p.kind === "error" && p.error) {
        setError(errorText(t, p.error));
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
    // Re-subscribe when the language changes so error text uses the new one.
  }, [t]);

  // Loads a stack preview (whole image or 1:1 region).
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
        setError(errorText(t, String(e)));
      } finally {
        if (kind === "detail") setDetailLoading(false);
      }
    },
    [t],
  );

  // Refreshes cache information for the current folder.
  const refreshCache = useCallback(async () => {
    if (!sourceDir || !IN_TAURI) return;
    try {
      const info = await invoke<CacheInfo>("cache_info", { sourceDir });
      setCacheInfo(info);
    } catch {
      setCacheInfo(null);
    }
  }, [sourceDir]);

  const pickFolder = useCallback(async () => {
    setError("");
    setCacheMsg("");
    setReport(null);
    setPreview("");
    setRawPreview("");
    setDetail("");
    if (!IN_TAURI) {
      setError("Folder selection is unavailable outside the app.");
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
      // Warn early if the cache will not fit on disk.
      try {
        const info = await invoke<CacheInfo>("cache_info", { sourceDir: selected });
        setCacheInfo(info);
        const need = res.count * 145 * 1024 * 1024; // ~145 MB per frame
        const free = await invoke<number>("available_space", { path: selected }).catch(() => null);
        if (typeof free === "number" && free > 0 && need > free) {
          setCacheMsg(
            t("cache.diskWarn", { need: formatBytes(need, lang), free: formatBytes(free, lang) }),
          );
        }
      } catch {
        setCacheInfo(null);
      }
    } catch (e) {
      setError(errorText(t, String(e)));
    } finally {
      setScanning(false);
    }
  }, [t, lang]);

  const runStack = useCallback(async () => {
    if (!sourceDir) return;
    setError("");
    setCacheMsg("");
    setReport(null);
    setWarnings([]);
    setSavedAs("");
    setPreview("");
    setRawPreview("");
    setDetail("");
    setDone(0);
    setTotal(scan?.count ?? 0);

    // --- Memory guard -------------------------------------------------------
    // A run whose peak footprint exceeds available memory pushes macOS into
    // swap, which starves the kernel watchdog and can reboot the machine. This
    // was observed on a 900-frame session. Estimate first, then refuse rather
    // than risk a reboot.
    if (IN_TAURI) {
      try {
        const mem = await invoke<MemoryInfo>("memory_info");
        // Crop side: from the detected object radius (with the pipeline's 8%
        // margin), else a conservative fraction of the sensor.
        const radius = scan?.preview?.planetRadius ?? 0;
        const cropSide = radius > 0 ? Math.round(radius * 1.16) * 2 : 3000;
        const cropPixels = cropSide * cropSide;
        const need = await invoke<number>("estimate_memory", {
          frames: scan?.count ?? 0,
          cropPixels,
          blockSize: settings.blockSize,
        });
        if (mem.available != null && need > mem.available) {
          setError(
            t("err.notEnoughMemory", {
              need: formatBytes(need, lang),
              free: formatBytes(mem.available, lang),
            }),
          );
          return;
        }
      } catch {
        // Guard is best-effort: a failure to measure must not block the run.
      }
    }

    setDrawerOpen(false);
    setCancelling(false);
    setPhase("importing");
    setStage("scanningFiles");
    try {
      const res = await invoke<StackReport>("run_stack", {
        sourceDir,
        cacheDir: `${sourceDir}/.hyperion_cache`,
        settings: mergeSettings(settings),
      });
      setReport(res);
      setPhase("done");
      clearPreviews();
      loadPreview(res.outputPath, "full");
      if (res.rawOutputPath) loadPreview(res.rawOutputPath, "raw");
      refreshCache();
    } catch (e) {
      const msg = String(e);
      setError(errorText(t, msg));
      setPhase("failed");
      setCancelling(false);
    }
  }, [sourceDir, settings, scan, loadPreview, refreshCache, t]);

  const clearPreviews = () => {
    setPreview("");
    setRawPreview("");
    setDetail("");
  };

  const cancel = useCallback(async () => {
    if (!IN_TAURI) return;
    setCancelling(true);
    try {
      await invoke("cancel_stack");
    } catch {
      setCancelling(false);
    }
  }, []);

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
      // Offer a free default name: the output folder already contains the
      // stack, and the backend refuses to overwrite an existing file.
      const dest = await save({
        defaultPath: base.replace(/\.tiff?$/i, "_copy.tif"),
        filters: [{ name: "16-bit TIFF", extensions: ["tif", "tiff"] }],
      });
      if (typeof dest !== "string") return;
      await invoke("save_copy", { from: report.outputPath, to: dest });
      setError("");
      setSavedAs(dest);
    } catch (e) {
      setError(errorText(t, String(e)));
    }
  }, [report, t]);

  const openOutputFolder = useCallback(async () => {
    if (!report || !IN_TAURI) return;
    try {
      await revealItemInDir(report.outputPath);
    } catch (e) {
      setError(String(e));
    }
  }, [report]);

  const clearCache = useCallback(async () => {
    if (!sourceDir || !IN_TAURI) return;
    try {
      const freed = await invoke<number>("clear_cache", { sourceDir });
      setCacheMsg(t("cache.freed", { size: formatBytes(freed, lang) }));
      refreshCache();
    } catch (e) {
      setError(String(e));
    }
  }, [sourceDir, t, lang, refreshCache]);

  const pruneCache = useCallback(async () => {
    if (!sourceDir || !IN_TAURI) return;
    try {
      const removed = await invoke<number>("prune_cache", { sourceDir });
      setCacheMsg(t("cache.pruned", { count: removed }));
      refreshCache();
    } catch (e) {
      setError(String(e));
    }
  }, [sourceDir, t, refreshCache]);

  // A new run resets the flow to the start.
  const restart = useCallback(() => {
    setReport(null);
    setWarnings([]);
    setSavedAs("");
    setPreview("");
    setRawPreview("");
    setDetail("");
    setPhase("idle");
    setError("");
    setCacheMsg("");
    setStep(0);
    setSourceDir("");
    setScan(null);
    setCacheInfo(null);
  }, []);

  // --- Render ---------------------------------------------------------------
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
            <h1>{t("app.brand")}</h1>
            <div className="brand-sub">{t("app.tagline")}</div>
          </div>
        </div>
        <div className="header-right">
          {scan && !inProgress && !finished && (
            <div className="header-meta">
              <span className="badge">
                {scan.count} {t("run.frames")}
              </span>
              {scan.cached > 0 && <span className="badge info">cache</span>}
            </div>
          )}
          {finished && (
            <span className="badge ok">
              <Icon name="check" size={12} /> {t("res.done")}
            </span>
          )}
          <LangSwitch lang={lang} setLang={setLang} label={t("app.lang.label")} />
          <button
            className="theme-toggle"
            onClick={toggleTheme}
            title={theme === "dark" ? t("app.theme.toLight") : t("app.theme.toDark")}
            aria-label={t("app.theme.label")}
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

      {cacheMsg && (
        <section className="panel info-box" style={{ marginBottom: 18 }}>
          <Icon name="folder" size={16} />
          <span>{cacheMsg}</span>
        </section>
      )}

      {/* --- Screen 1: processing --- */}
      {inProgress && (
        <Running
          phase={phase}
          stage={stage}
          done={done}
          total={total}
          elapsed={elapsed}
          cancelling={cancelling}
          onCancel={cancel}
          t={t}
        />
      )}

      {/* --- Screen 2: result --- */}
      {finished && (
        <Result
          report={report}
          finalPreview={preview}
          rawPreview={rawPreview}
          detail={detail}
          detailLoading={detailLoading}
          skipped={warnings}
          savedAs={savedAs}
          onInspect={pickDetail}
          onSaveAs={saveAs}
          onOpenFolder={openOutputFolder}
          t={t}
        />
      )}

      {/* --- Screen 3: step-by-step wizard --- */}
      {!inProgress && !finished && (
        <div className="wizard">
          <div className="wiz-steps">
            <div className={`wiz-step ${step === 0 ? "active" : "done"}`}>
              <span className="bullet">
                {step === 0 ? "01" : <Icon name="check" size={14} />}
              </span>
              <span className="lbl">
                <b>{t("wiz.step.folder")}</b>
                <span>
                  {scan ? `${scan.count} ${t("run.frames")}` : sourceDir ? "—" : t("wiz.step.folder.sub")}
                </span>
              </span>
              <span className="rail" />
            </div>
            <div className={`wiz-step ${step === 1 ? "active" : "pending"}`}>
              <span className="bullet">02</span>
              <span className="lbl">
                <b>{t("wiz.step.settings")}</b>
                <span>{t("wiz.step.settings.sub")}</span>
              </span>
              <span className="rail" />
            </div>
            <div className="wiz-step">
              <span className="bullet">03</span>
              <span className="lbl">
                <b>{t("wiz.step.processing")}</b>
                <span>{t("wiz.step.processing.sub")}</span>
              </span>
            </div>
          </div>

          {/* Step 1: choose the folder */}
          {step === 0 && (
            <div className="wiz-body">
              <div className="wiz-title">{t("wiz.folder.title")}</div>
              <p className="wiz-desc">{t("wiz.folder.desc")}</p>

              <section className={`drop ${sourceDir ? "filled" : ""}`} onClick={pickFolder}>
                <div className="drop-icon">
                  <Icon name="folder" size={30} />
                </div>
                <h2>{sourceDir ? t("wiz.folder.picked") : t("wiz.folder.pick")}</h2>
                <p className="path">{sourceDir || t("wiz.folder.hintClick")}</p>
                {scanning && <p className="muted">{t("wiz.folder.scanning")}</p>}
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
                        <Icon name="check" size={12} /> {t("wiz.folder.objectDetected")} (~
                        {Math.round(scan.preview.planetRadius ?? 0)} px)
                      </span>
                    ) : (
                      scan.preview && (
                        <span className="badge warn">
                          <Icon name="alert" size={12} /> {t("wiz.folder.noObject")}
                        </span>
                      )
                    )}
                  </div>
                )}
              </section>

              <div className="wiz-foot">
                <span className="hint">
                  {scan ? t("wiz.folder.ready") : t("wiz.folder.selectFirst")}
                </span>
                <button className="btn primary" disabled={!sourceDir} onClick={() => setStep(1)}>
                  {t("wiz.continue")} <Icon name="arrow-right" size={15} />
                </button>
              </div>
            </div>
          )}

          {/* Step 2: settings */}
          {step === 1 && (
            <div className="wiz-body">
              <div className="wiz-title">{t("wiz.settings.title")}</div>
              <p className="wiz-desc">{t("wiz.settings.desc")}</p>

              <div className="group" style={{ marginTop: 0, borderTop: "none", paddingTop: 0 }}>
                <div className="group-head">
                  <span className="group-title">{t("wiz.settings.orientation")}</span>
                  <span className="hint">{t("wiz.settings.orientationHint")}</span>
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
                    <Icon name="rotate" size={13} /> {t("orient.90")}
                  </button>
                  <button
                    className="chip"
                    onClick={() =>
                      setSettings((s) => ({
                        ...s,
                        orientation: { ...s.orientation, rotate: (s.orientation.rotate + 180) % 360 },
                      }))
                    }
                  >
                    <Icon name="rotate" size={13} /> {t("orient.180")}
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
                    <Icon name="flip-h" size={13} /> {t("orient.flipH")}
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
                    <Icon name="flip-v" size={13} /> {t("orient.flipV")}
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
                    <Icon name="reset" size={13} /> {t("orient.reset")}
                  </button>
                </div>
              </div>

              <div className="group">
                <div className="group-head">
                  <span className="group-title">{t("wiz.settings.post")}</span>
                </div>
                <Toggle
                  checked={settings.enhance.enabled}
                  onChange={(v) =>
                    setSettings((s) => ({ ...s, enhance: { ...s.enhance, enabled: v } }))
                  }
                  label={t("wiz.settings.postLabel")}
                  description={t("wiz.settings.postDesc")}
                />
              </div>

              <div className="recap">
                <div className="recap-row">
                  <span>{t("wiz.recap.keep")}</span>
                  <b>{Math.round(settings.keepFraction * 100)} %</b>
                </div>
                <div className="recap-row">
                  <span>{t("wiz.recap.blocks")}</span>
                  <b>
                    {settings.blockSize > 0
                      ? `${settings.blockSize} ${t("run.frames")}`
                      : t("wiz.recap.global")}
                  </b>
                </div>
                <div className="recap-row">
                  <span>{t("wiz.recap.align")}</span>
                  <b>{settings.align ? t("wiz.recap.on") : t("wiz.recap.off")}</b>
                </div>
                <div className="recap-row">
                  <span>{t("wiz.recap.crop")}</span>
                  <b>{settings.cropSize === 0 ? t("wiz.recap.auto") : `${settings.cropSize} px`}</b>
                </div>
              </div>

              <div className="wiz-foot">
                <button className="tbtn" onClick={() => setStep(0)}>
                  <Icon name="arrow-left" size={15} /> {t("wiz.back")}
                </button>
                <div className="actions">
                  <button className="tbtn" onClick={() => setDrawerOpen(true)}>
                    <Icon name="sliders" size={15} /> {t("wiz.allSettings")}
                  </button>
                  <button className="btn primary" onClick={runStack}>
                    <Icon name="play" size={15} /> {t("wiz.run")}
                  </button>
                </div>
              </div>
            </div>
          )}
        </div>
      )}

      {/* --- Settings drawer --- */}
      {drawerOpen && (
        <SettingsDrawer
          settings={settings}
          cacheInfo={cacheInfo}
          onCacheChange={refreshCache}
          onClearCache={clearCache}
          onPruneCache={pruneCache}
          lang={lang}
          onChange={setSettings}
          onClose={() => setDrawerOpen(false)}
          t={t}
        />
      )}

      {/* After a result, allow starting over. */}
      {finished && (
        <div style={{ marginTop: 24, display: "flex", justifyContent: "center" }}>
          <button className="tbtn" onClick={restart}>
            <Icon name="reset" size={15} /> {t("res.restart")}
          </button>
        </div>
      )}
    </div>
  );
}

/// Compact language switcher in the header (FR / EN).
function LangSwitch({
  lang,
  setLang,
  label,
}: {
  lang: Lang;
  setLang: (l: Lang) => void;
  label: string;
}) {
  return (
    <div className="lang-switch" role="group" aria-label={label} title={label}>
      {LANGS.map((l) => (
        <button
          key={l.code}
          className={`lang-btn ${lang === l.code ? "on" : ""}`}
          onClick={() => setLang(l.code)}
        >
          {l.code.toUpperCase()}
        </button>
      ))}
    </div>
  );
}

export default App;
