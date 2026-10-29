// Écran de traitement en cours.
//
// Affiche les cinq étapes de la chaîne, la progression globale, le compteur
// d'images et le temps écoulé avec estimation du restant.

import { Icon, type IconName } from "./Icon";
import type { Phase } from "./types";

const PHASES: { key: Phase; label: string; icon: IconName }[] = [
  { key: "importing", label: "Import RAW", icon: "download" },
  { key: "analyzing", label: "Analyse", icon: "search" },
  { key: "stacking", label: "Empilement", icon: "layers" },
  { key: "enhancing", label: "Post-traitement", icon: "sparkle" },
  { key: "done", label: "Export TIFF", icon: "file" },
];

function formatDuration(seconds: number): string {
  if (!isFinite(seconds) || seconds < 0) return "—";
  const s = Math.round(seconds);
  if (s < 60) return `${s} s`;
  const m = Math.floor(s / 60);
  const r = s % 60;
  if (m < 60) return `${m} min ${r.toString().padStart(2, "0")} s`;
  const h = Math.floor(m / 60);
  return `${h} h ${(m % 60).toString().padStart(2, "0")}`;
}

export function Running({
  phase,
  message,
  done,
  total,
  elapsed,
}: {
  phase: Phase;
  message: string;
  done: number;
  total: number;
  elapsed: number;
}) {
  const activeIndex = PHASES.findIndex((p) => p.key === phase);

  // Progression pondérée : chaque étape occupe une part du travail total.
  // Les parts doivent sommer à 1 : c'est le total du travail, pas une suite
  // cumulative. Import 30 %, analyse 25 %, empilement 35 %, post-traitement
  // et export 10 %. Cela évite d'afficher « 100 % » alors qu'on empile encore.
  const shares = [0.30, 0.25, 0.35, 0.06, 0.04];
  const idx = Math.max(0, activeIndex);
  const before = shares.slice(0, idx).reduce((a, b) => a + b, 0);
  // Part de l'étape courante, pilotée par les images quand c'est pertinent.
  const within =
    phase === "importing" || phase === "analyzing" || phase === "stacking"
      ? total > 0
        ? Math.min(1, done / total)
        : 0
      : phase === "enhancing"
        ? 0.5
        : phase === "done"
          ? 1
          : 0;
  const progressPct = Math.min(100, Math.round((before + shares[idx] * within) * 100));

  // Estimation du temps restant, calculée sur le rythme observé.
  let eta = "";
  if (done >= 3 && elapsed >= 5 && total > done) {
    const perItem = elapsed / done;
    eta = `~${formatDuration((total - done) * perItem)} restant`;
  }

  return (
    <div className="panel">
      <div className="steps">
        {PHASES.map((p, i) => {
          const state = i < activeIndex ? "done" : i === activeIndex ? "active" : "pending";
          return (
            <div key={p.key} className={`step ${state}`}>
              <span className="step-n">{String(i + 1).padStart(2, "0")}</span>
              {p.label}
            </div>
          );
        })}
      </div>

      <div className="running-hero">
        <span className="running-pct">{progressPct}%</span>
        <span className="running-title">{message || "Traitement en cours…"}</span>
        {total > 0 && (
          <span className="running-sub">
            {done} / {total} images
          </span>
        )}
      </div>

      <div className="progress-bar" style={{ marginTop: 18 }}>
        <div className="progress-fill" style={{ width: `${progressPct}%` }} />
      </div>

      <div className="progress-row">
        <span className="progress-text">
          <Icon name="refresh" size={12} /> Temps écoulé : {formatDuration(elapsed)}
        </span>
        {eta && <span className="progress-time">{eta}</span>}
      </div>

      <p className="note" style={{ marginTop: 10 }}>
        <Icon name="alert" size={12} /> Vous pouvez laisser la fenêtre ouverte : le traitement
        continue tant que l'application est lancée.
      </p>
    </div>
  );
}
