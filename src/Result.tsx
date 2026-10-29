// Écran de résultat : l'image finale en grand, les statistiques, l'inspection 1:1.
//
// Trois onglets : l'image finale, le stack brut (sans post-traitement) et une
// comparaison côte à côte. L'App fournit les aperçus déjà chargés.

import { useState } from "react";
import { Icon } from "./Icon";
import type { SkippedFrame, StackReport } from "./types";

type Tab = "final" | "raw" | "compare";

export function Result({
  report,
  finalPreview,
  rawPreview,
  detail,
  detailLoading,
  skipped,
  onInspect,
  onSaveAs,
  onOpenFolder,
}: {
  report: StackReport;
  finalPreview: string;
  rawPreview: string;
  detail: string;
  detailLoading: boolean;
  skipped: SkippedFrame[];
  onInspect: (evt: React.MouseEvent<HTMLImageElement>) => void;
  onSaveAs: () => void;
  onOpenFolder: () => void;
}) {
  const [tab, setTab] = useState<Tab>("final");

  // Si le stack brut n'est pas disponible (post-traitement désactivé), on
  // retombe sur l'image finale plutôt que d'afficher un cadre vide.
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
                <Icon name="check" size={18} /> Stack terminé
              </h2>
              <p className="path">{report.outputPath}</p>
            </div>
            <div className="actions">
              <button className="tbtn" onClick={onSaveAs}>
                <Icon name="download" size={13} /> Enregistrer sous
              </button>
              <button className="btn small" onClick={onOpenFolder}>
                <Icon name="folder-open" size={13} /> Ouvrir le dossier
              </button>
            </div>
          </div>
        </div>

        {skipped.length > 0 && (
          <section className="panel warn-box animate-in" style={{ marginBottom: 18 }}>
            <div className="warn-head">
              <Icon name="alert" size={16} />
              <b>
                {skipped.length} frame{skipped.length > 1 ? "s" : ""} écartée
                {skipped.length > 1 ? "s" : ""}
              </b>
            </div>
            <p className="note" style={{ marginTop: 4 }}>
              Le reste de la session a été traité normalement. Ces fichiers sont probablement
              corrompus ou d'un format non pris en charge.
            </p>
            <ul className="warn-list">
              {skipped.slice(0, 8).map((s, i) => (
                <li key={`${s.name}-${i}`}>
                  <b>{s.name}</b>
                  <span>{s.reason}</span>
                </li>
              ))}
            </ul>
            {skipped.length > 8 && (
              <p className="note">… et {skipped.length - 8} autre(s).</p>
            )}
          </section>
        )}

        <div className="tabs">
          <button
            className={`tab ${effectiveTab === "final" ? "on" : ""}`}
            onClick={() => setTab("final")}
          >
            Image finale
          </button>
          <button
            className={`tab ${effectiveTab === "raw" ? "on" : ""}`}
            onClick={() => setTab("raw")}
            disabled={!hasRaw}
            title={hasRaw ? "" : "Post-traitement désactivé : le brut est identique."}
          >
            Stack brut
          </button>
          <button
            className={`tab ${effectiveTab === "compare" ? "on" : ""}`}
            onClick={() => setTab("compare")}
            disabled={!hasRaw}
          >
            Comparer
          </button>
        </div>

        {effectiveTab === "final" && finalPreview && (
          <ImageView src={finalPreview} onInspect={onInspect} />
        )}

        {effectiveTab === "raw" && hasRaw && <ImageView src={rawPreview} onInspect={onInspect} />}

        {effectiveTab === "compare" && hasRaw && (
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 14 }}>
            <div className="reveal">
              <span className="group-title">Stack brut</span>
              <img src={rawPreview} alt="Stack brut" style={{ imageRendering: "pixelated" }} />
            </div>
            <div className="reveal">
              <span className="group-title">Image finale</span>
              <img
                src={finalPreview}
                alt="Image finale"
                onClick={onInspect}
                style={{ cursor: "crosshair" }}
              />
            </div>
          </div>
        )}

        {!finalPreview && (
          <div className="result-hero">
            <div className="result-empty">
              Aperçu en cours de génération…
              <br />
              Le fichier TIFF est déjà enregistré sur le disque.
            </div>
          </div>
        )}

        {detailLoading && <p className="muted">Chargement de la vue 1:1…</p>}
        {detail && (
          <div className="reveal" style={{ marginTop: 18 }}>
            <span className="group-title">Vue 1:1 — netteté réelle</span>
            <img src={detail} alt="Détail 1:1" style={{ maxWidth: 520 }} />
          </div>
        )}
      </div>

      <aside style={{ display: "flex", flexDirection: "column", gap: 18 }}>
        <div className="stat-cards">
          <StatCard
            value={String(report.keptFrames)}
            sub={`/${report.totalFrames}`}
            label="frames empilées"
          />
          <StatCard value={String(Math.round(report.meanQuality * 100))} label="qualité moyenne" />
          {report.crop && (
            <StatCard value={`${report.crop.width}²`} label="taille du crop" />
          )}
          <StatCard value="16" sub="bit" label="profondeur" />
        </div>

        <div className="panel side">
          <h3>Détails du traitement</h3>
          <ul className="summary">
            <li>
              <span>Chemin</span>
              <b title={report.outputPath}>{shorten(report.outputPath, 22)}</b>
            </li>
            <li>
              <span>Frames gardées</span>
              <b>
                {report.keptFrames}/{report.totalFrames}
              </b>
            </li>
          </ul>
        </div>

        <div className="panel side">
          <h3>Et ensuite ?</h3>
          <p className="note">
            Le TIFF 16 bits est prêt pour Luminar, Photoshop ou GIMP. Pas de compression, pas de
            perte.
          </p>
          <button className="btn small" style={{ marginTop: 14, width: "100%" }} onClick={onOpenFolder}>
            <Icon name="folder-open" size={13} /> Révéler dans le Finder
          </button>
        </div>
      </aside>
    </div>
  );
}

/// Image cliquable pour l'inspection 1:1, avec son repère.
function ImageView({
  src,
  onInspect,
}: {
  src: string;
  onInspect: (evt: React.MouseEvent<HTMLImageElement>) => void;
}) {
  return (
    <div className="result-hero">
      <img src={src} alt="Aperçu du stack" onClick={onInspect} />
      <span className="hint">Cliquez pour inspecter à 100 %</span>
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

/// Raccourcit un chemin trop long pour la colonne latérale.
function shorten(path: string, max: number): string {
  if (path.length <= max) return path;
  const file = path.split("/").pop() ?? path;
  return `…/${file}`;
}
