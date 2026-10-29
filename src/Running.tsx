// In-progress screen.
//
// Shows the five pipeline steps, overall progress, the frame counter and the
// elapsed time with a remaining estimate. Also hosts the cancel button.

import { Icon, type IconName } from "./Icon";
import { type Translate, stageText } from "./i18n";
import type { Phase, Stage } from "./types";

const PHASES: { key: Phase; icon: IconName }[] = [
  { key: "importing", icon: "download" },
  { key: "analyzing", icon: "search" },
  { key: "stacking", icon: "layers" },
  { key: "enhancing", icon: "sparkle" },
  { key: "done", icon: "file" },
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
  stage,
  done,
  total,
  elapsed,
  cancelling,
  onCancel,
  t,
}: {
  phase: Phase;
  stage: Stage;
  done: number;
  total: number;
  elapsed: number;
  cancelling: boolean;
  onCancel: () => void;
  t: Translate;
}) {
  const activeIndex = PHASES.findIndex((p) => p.key === phase);

  // Weighted progress: each step takes a share of the total work. Shares must
  // sum to 1 (this is the total work, not a cumulative ramp). Import 30%,
  // analysis 25%, stacking 35%, post-processing and export 10%. This keeps the
  // bar from showing "100%" while still stacking.
  const shares = [0.30, 0.25, 0.35, 0.06, 0.04];
  const idx = Math.max(0, activeIndex);
  const before = shares.slice(0, idx).reduce((a, b) => a + b, 0);
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

  // Remaining-time estimate, based on the observed pace.
  let eta = "";
  if (done >= 3 && elapsed >= 5 && total > done) {
    const perItem = elapsed / done;
    eta = `~${formatDuration((total - done) * perItem)} ${t("run.remaining")}`;
  }

  return (
    <div className="panel">
      <div className="steps">
        {PHASES.map((p, i) => {
          const state = i < activeIndex ? "done" : i === activeIndex ? "active" : "pending";
          return (
            <div key={p.key} className={`step ${state}`}>
              <span className="step-n">{String(i + 1).padStart(2, "0")}</span>
              {stageLabel(t, p.key)}
            </div>
          );
        })}
      </div>

      <div className="running-hero">
        <span className="running-pct">{progressPct}%</span>
        <span className="running-title">{stageText(t, stage)}</span>
        {total > 0 && (
          <span className="running-sub">
            {done} / {total} {t("run.frames")}
          </span>
        )}
      </div>

      <div className="progress-bar" style={{ marginTop: 18 }}>
        <div className="progress-fill" style={{ width: `${progressPct}%` }} />
      </div>

      <div className="progress-row">
        <span className="progress-text">
          <Icon name="refresh" size={12} /> {t("run.elapsed")} : {formatDuration(elapsed)}
        </span>
        {eta && <span className="progress-time">{eta}</span>}
      </div>

      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          gap: 16,
          marginTop: 14,
          flexWrap: "wrap",
        }}
      >
        <p className="note" style={{ margin: 0, flex: 1, minWidth: 220 }}>
          <Icon name="alert" size={12} /> {t("run.note")}
        </p>
        <button className="tbtn" onClick={onCancel} disabled={cancelling}>
          <Icon name="x" size={14} />
          {cancelling ? t("run.cancelling") : t("run.cancel")}
        </button>
      </div>
    </div>
  );
}

/// Label for a pipeline step, shared with the step bar.
function stageLabel(t: Translate, phase: Phase) {
  switch (phase) {
    case "importing":
      return t("stage.scanningFiles").replace("…", "");
    case "analyzing":
      return t("stage.scoringFrames").replace("…", "");
    case "stacking":
      return t("stage.stacking").replace("…", "");
    case "enhancing":
      return t("stage.enhancing").replace("…", "");
    case "done":
      return t("stage.writingOutput").replace("…", "");
    default:
      return "";
  }
}
