// Volet de réglages (drawer) — glisse depuis la droite.
//
// Tous les réglages de l'empilement sont ici : orientation, frames, crop,
// post-traitement et options avancées. Le volet est un simple panneau : c'est
// l'App qui décide de l'afficher ou non.

import { Icon } from "./Icon";
import { Slider } from "./Slider";
import { Toggle } from "./Toggle";
import { DEFAULT_SETTINGS, type Settings } from "./types";

export function SettingsDrawer({
  settings,
  onChange,
  onClose,
}: {
  settings: Settings;
  onChange: (s: Settings) => void;
  onClose: () => void;
}) {
  // Petit raccourci pour modifier un champ sans répéter le spread.
  const set = <K extends keyof Settings>(key: K, value: Settings[K]) =>
    onChange({ ...settings, [key]: value });

  const setEnhance = <K extends keyof Settings["enhance"]>(
    key: K,
    value: Settings["enhance"][K],
  ) => onChange({ ...settings, enhance: { ...settings.enhance, [key]: value } });

  const rotate = (deg: number) =>
    set("orientation", {
      ...settings.orientation,
      rotate: (settings.orientation.rotate + deg) % 360,
    });

  return (
    <>
      <div className="drawer-scrim" onClick={onClose} />
      <aside className="drawer" role="dialog" aria-label="Réglages">
        <div className="drawer-head">
          <h2>Réglages</h2>
          <button className="btn icon" onClick={onClose} aria-label="Fermer">
            <Icon name="x" size={16} />
          </button>
        </div>

        <div className="drawer-body">
          {/* --- Orientation --- */}
          <div className="block">
            <span className="group-title">Orientation</span>
            <p className="blk-desc">Le capteur ne sait pas comment le télescope était retourné.</p>
            <div className="chips">
              <button className="chip" onClick={() => rotate(90)}>
                <Icon name="rotate" size={13} /> 90°
              </button>
              <button className="chip" onClick={() => rotate(180)}>
                <Icon name="rotate" size={13} /> 180°
              </button>
              <button
                className={`chip ${settings.orientation.flipH ? "on" : ""}`}
                onClick={() =>
                  set("orientation", {
                    ...settings.orientation,
                    flipH: !settings.orientation.flipH,
                  })
                }
              >
                <Icon name="flip-h" size={13} /> Miroir H
              </button>
              <button
                className={`chip ${settings.orientation.flipV ? "on" : ""}`}
                onClick={() =>
                  set("orientation", {
                    ...settings.orientation,
                    flipV: !settings.orientation.flipV,
                  })
                }
              >
                <Icon name="flip-v" size={13} /> Miroir V
              </button>
            </div>
          </div>

          {/* --- Frames & empilement --- */}
          <div className="block">
            <span className="group-title">Frames &amp; empilement</span>
            <div className="sliders" style={{ marginTop: 8 }}>
              <Slider
                label="Frames conservées"
                value={Math.round(settings.keepFraction * 100)}
                min={1}
                max={100}
                step={1}
                format={(v) => `${Math.round(v)} %`}
                onChange={(v) => set("keepFraction", v / 100)}
              />
              <Slider
                label="Empilement par blocs"
                value={settings.blockSize}
                min={0}
                max={40}
                step={1}
                format={(v) => (v === 0 ? "désactivé" : `${v} frames`)}
                onChange={(v) => set("blockSize", Math.round(v))}
              />
              <p className="note">
                <Icon name="alert" size={12} /> Sur une longue session, la Lune tourne : empiler
                par petits groupes consécutifs évite le flou final.
              </p>
              <Slider
                label="Taille du crop"
                value={settings.cropSize}
                min={0}
                max={4000}
                step={100}
                format={(v) => (v === 0 ? "auto" : `${v} px`)}
                onChange={(v) => set("cropSize", Math.round(v))}
              />
            </div>
          </div>

          {/* --- Amélioration --- */}
          <div className="block">
            <span className="group-title">Amélioration</span>
            <div className="toggle" style={{ margin: "10px 0 0" }}>
              <Toggle
                checked={settings.enhance.enabled}
                onChange={(v) => setEnhance("enabled", v)}
                label="Activer le post-traitement"
                description="Sans effet sur le TIFF brut si désactivé."
              />
            </div>
            {settings.enhance.enabled && (
              <div className="sliders" style={{ marginTop: 16 }}>
                <Slider
                  label="Netteté (ondelettes)"
                  value={settings.enhance.waveletAmount}
                  min={0}
                  max={3}
                  step={0.05}
                  onChange={(v) => setEnhance("waveletAmount", v)}
                />
                <Slider
                  label="Échelles d'ondelettes"
                  value={settings.enhance.waveletLevels}
                  min={1}
                  max={6}
                  step={1}
                  onChange={(v) => setEnhance("waveletLevels", Math.round(v))}
                />
                <Slider
                  label="Contraste local (CLAHE)"
                  value={settings.enhance.claheAmount}
                  min={0}
                  max={1}
                  step={0.05}
                  onChange={(v) => setEnhance("claheAmount", v)}
                />
                <Slider
                  label="Relief (clarté)"
                  value={settings.enhance.clarity}
                  min={0}
                  max={1}
                  step={0.02}
                  onChange={(v) => setEnhance("clarity", v)}
                />
                <Slider
                  label="Gamma"
                  value={settings.enhance.gamma}
                  min={0.5}
                  max={1.8}
                  step={0.05}
                  onChange={(v) => setEnhance("gamma", v)}
                />
              </div>
            )}
          </div>

          {/* --- Avancé --- */}
          <div className="block">
            <span className="group-title">Avancé</span>
            <div className="toggles" style={{ marginTop: 10 }}>
              <Toggle
                checked={settings.align}
                onChange={(v) => set("align", v)}
                label="Aligner les frames"
              />
              <Toggle
                checked={settings.normalize}
                onChange={(v) => set("normalize", v)}
                label="Normaliser la luminosité"
              />
              <Toggle
                checked={settings.weighted}
                onChange={(v) => set("weighted", v)}
                label="Pondérer par la qualité"
              />
              <Toggle
                checked={settings.apEnabled}
                onChange={(v) => set("apEnabled", v)}
                label="Points d'alignement locaux"
                description="Expérimental — dégrade l'image actuellement."
              />
            </div>
          </div>
        </div>

        <div className="drawer-foot">
          <button className="tbtn small" onClick={() => onChange(DEFAULT_SETTINGS)}>
            <Icon name="reset" size={13} /> Valeurs par défaut
          </button>
          <button className="btn primary" onClick={onClose}>
            <Icon name="check" size={15} /> Appliquer
          </button>
        </div>
      </aside>
    </>
  );
}
