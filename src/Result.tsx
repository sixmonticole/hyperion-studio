// Result screen: the final image large, statistics, 1:1 inspection.
//
// Three tabs: the final image, the raw stack (without post-processing) and a
// side-by-side comparison. The App supplies the already-loaded previews.

import { useState } from "react";
import { Icon } from "./Icon";
import { skipText, type Translate } from "./i18n";
import type { SkippedFrame, StackReport } from "./types";

type Tab = "final" | "raw" | "compare";

export function Result({
  report,
  finalPreview,
  rawPreview,
  detail,
  detailLoading,
  skipped,
  savedAs,
  onInspect,
  onSaveAs,
  onOpenFolder,
  t,
}: {
  report: StackReport;
  finalPreview: string;
  rawPreview: string;
  detail: string;
  detailLoading: boolean;
  skipped: SkippedFrame[];
  savedAs: string;
  onInspect: (evt: React.MouseEvent<HTMLImageElement>) => void;
  onSaveAs: () => void;
  onOpenFolder: () => void;
  t: Translate;
}) {
  const [tab, setTab] = useState<Tab>("final");

  // If the raw stack is unavailable (post-processing off), fall back to the
  // final image rather than showing an empty frame.
  const hasRaw = rawPreview.length > 0;
  const effectiveTab: Tab = tab === "raw" && !hasRaw ? "final" : tab;

  return (
    <div className="result-grid">
      <div>
        <div className="panel animate-in" style={{ padding: 20, marginBottom: 18 }}>
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "flex-start",
              gap: 16,
              flexWrap: "wrap",
            }}
          >
            <div style={{ minWidth: 0 }}>
              <h2 style={{ color: "var(--ok)", display: "flex", alignItems: "center", gap: 8 }}>
                <Icon name="check" size={18} /> {t("res.done")}
              </h2>
              <p className="path">{report.outputPath}</p>
            </div>
            <div className="actions">
              <button className="tbtn" onClick={onSaveAs}>
                <Icon name="download" size={13} /> {t("res.saveAs")}
              </button>
              <button className="btn small" onClick={onOpenFolder}>
                <Icon name="folder-open" size={13} /> {t("res.openFolder")}
              </button>
            </div>
          </div>
        </div>

        {skipped.length > 0 && (
          <section className="panel warn-box animate-in" style={{ marginBottom: 18 }}>
            <div className="warn-head">
              <Icon name="alert" size={16} />
              <b>{t("res.skipped", { count: skipped.length })}</b>
            </div>
            <p className="note" style={{ marginTop: 4 }}>
              {t("res.skippedNote")}
            </p>
            <ul className="warn-list">
              {skipped.slice(0, 8).map((s, i) => (
                <li key={`${s.name}-${i}`}>
                  <b>{s.name}</b>
                  <span>{skipText(t, s.reason)}</span>
                </li>
              ))}
            </ul>
            {skipped.length > 8 && (
              <p className="note">{t("res.skippedMore", { count: skipped.length - 8 })}</p>
            )}
          </section>
        )}

        <div className="tabs">
          <button
            className={`tab ${effectiveTab === "final" ? "on" : ""}`}
            onClick={() => setTab("final")}
          >
            {t("res.tab.final")}
          </button>
          <button
            className={`tab ${effectiveTab === "raw" ? "on" : ""}`}
            onClick={() => setTab("raw")}
            disabled={!hasRaw}
            title={hasRaw ? "" : t("res.tab.rawDisabled")}
          >
            {t("res.tab.raw")}
          </button>
          <button
            className={`tab ${effectiveTab === "compare" ? "on" : ""}`}
            onClick={() => setTab("compare")}
            disabled={!hasRaw}
          >
            {t("res.tab.compare")}
          </button>
        </div>

        {effectiveTab === "final" && finalPreview && (
          <ImageView src={finalPreview} onInspect={onInspect} t={t} />
        )}

        {effectiveTab === "raw" && hasRaw && (
          <ImageView src={rawPreview} onInspect={onInspect} t={t} />
        )}

        {effectiveTab === "compare" && hasRaw && (
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 14 }}>
            <div className="reveal">
              <span className="group-title">{t("res.rawLabel")}</span>
              <img src={rawPreview} alt={t("res.rawLabel")} style={{ imageRendering: "pixelated" }} />
            </div>
            <div className="reveal">
              <span className="group-title">{t("res.finalLabel")}</span>
              <img
                src={finalPreview}
                alt={t("res.finalLabel")}
                onClick={onInspect}
                style={{ cursor: "crosshair" }}
              />
            </div>
          </div>
        )}

        {!finalPreview && (
          <div className="result-hero">
            <div className="result-empty">
              {t("res.previewPending")}
              <br />
              {t("res.previewPendingSub")}
            </div>
          </div>
        )}

        {detailLoading && <p className="muted">{t("res.detailLoading")}</p>}
        {detail && (
          <div className="reveal" style={{ marginTop: 18 }}>
            <span className="group-title">{t("res.detailTitle")}</span>
            <img src={detail} alt={t("res.detailTitle")} style={{ maxWidth: 520 }} />
          </div>
        )}
      </div>

      <aside style={{ display: "flex", flexDirection: "column", gap: 18 }}>
        <div className="stat-cards">
          <StatCard
            value={String(report.keptFrames)}
            sub={`/${report.totalFrames}`}
            label={t("res.stat.frames")}
          />
          <StatCard value={String(Math.round(report.meanQuality * 100))} label={t("res.stat.quality")} />
          {report.crop && <StatCard value={`${report.crop.width}²`} label={t("res.stat.crop")} />}
          <StatCard value="16" sub="bit" label={t("res.stat.depth")} />
        </div>

        <div className="panel side">
          <h3>{t("res.details")}</h3>
          <ul className="summary">
            <li>
              <span>{t("res.path")}</span>
              <b title={report.outputPath}>{shorten(report.outputPath, 22)}</b>
            </li>
            <li>
              <span>{t("res.kept")}</span>
              <b>
                {report.keptFrames}/{report.totalFrames}
              </b>
            </li>
          </ul>
        </div>

        <div className="panel side">
          <h3>{t("res.next")}</h3>
          <p className="note">{t("res.nextNote")}</p>
          <button
            className="btn small"
            style={{ marginTop: 14, width: "100%" }}
            onClick={onOpenFolder}
          >
            <Icon name="folder-open" size={13} /> {t("res.reveal")}
          </button>
          {savedAs && (
            <p className="note" style={{ marginTop: 10 }}>
              <Icon name="check" size={12} /> {shorten(savedAs, 30)}
            </p>
          )}
        </div>
      </aside>
    </div>
  );
}

/// Clickable image for 1:1 inspection, with its hint label.
function ImageView({
  src,
  onInspect,
  t,
}: {
  src: string;
  onInspect: (evt: React.MouseEvent<HTMLImageElement>) => void;
  t: Translate;
}) {
  return (
    <div className="result-hero">
      <img src={src} alt="" onClick={onInspect} />
      <span className="hint">{t("res.hint.inspect")}</span>
    </div>
  );
}

function StatCard({ value, sub, label }: { value: string; sub?: string; label: string }) {
  return (
    <div className="stat-card">
      <span className="stat-value">
        {value}
        {sub && <span className="stat-sub">{sub}</span>}
      </span>
      <span className="stat-label">{label}</span>
    </div>
  );
}

/// Shortens a path too long for the side column.
function shorten(path: string, max: number): string {
  if (path.length <= max) return path;
  const file = path.split("/").pop() ?? path;
  return `…/${file}`;
}
