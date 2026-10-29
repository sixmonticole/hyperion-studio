// Settings drawer — slides in from the right.
//
// All stacking settings live here: orientation, frames, crop, post-processing
// and advanced options, plus disk-cache management. The drawer is a plain
// panel: the App decides whether to show it.

import { useEffect, useState } from "react";
import { Icon } from "./Icon";
import { Slider } from "./Slider";
import { Toggle } from "./Toggle";
import { DEFAULT_SETTINGS, type CacheInfo, type Settings } from "./types";
import { formatBytes, type Lang, type Translate } from "./i18n";

export function SettingsDrawer({
  settings,
  cacheInfo,
  onCacheChange,
  onClearCache,
  onPruneCache,
  lang,
  onChange,
  onClose,
  t,
}: {
  settings: Settings;
  cacheInfo: CacheInfo | null;
  onCacheChange: () => void;
  onClearCache: () => void;
  onPruneCache: () => void;
  lang: Lang;
  onChange: (s: Settings) => void;
  onClose: () => void;
  t: Translate;
}) {
  // Shortcut to change one field without repeating the spread.
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

  // Refresh cache info when the drawer opens.
  useEffect(() => {
    onCacheChange();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const [busy, setBusy] = useState(false);
  const runCacheAction = async (fn: () => void) => {
    setBusy(true);
    fn();
    // The parent refreshes the info; clear the busy flag shortly after.
    window.setTimeout(() => setBusy(false), 400);
  };

  return (
    <>
      <div className="drawer-scrim" onClick={onClose} />
      <aside className="drawer" role="dialog" aria-label={t("set.title")}>
        <div className="drawer-head">
          <h2>{t("set.title")}</h2>
          <button className="btn icon" onClick={onClose} aria-label={t("set.close")}>
            <Icon name="x" size={16} />
          </button>
        </div>

        <div className="drawer-body">
          {/* --- Orientation --- */}
          <div className="block">
            <span className="group-title">{t("set.oriented")}</span>
            <p className="blk-desc">{t("set.orientedDesc")}</p>
            <div className="chips">
              <button className="chip" onClick={() => rotate(90)}>
                <Icon name="rotate" size={13} /> {t("orient.90")}
              </button>
              <button className="chip" onClick={() => rotate(180)}>
                <Icon name="rotate" size={13} /> {t("orient.180")}
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
                <Icon name="flip-h" size={13} /> {t("orient.flipH")}
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
                <Icon name="flip-v" size={13} /> {t("orient.flipV")}
              </button>
            </div>
          </div>

          {/* --- Frames & stacking --- */}
          <div className="block">
            <span className="group-title">{t("set.frames")}</span>
            <div className="sliders" style={{ marginTop: 8 }}>
              <Slider
                label={t("set.keepFraction")}
                value={Math.round(settings.keepFraction * 100)}
                min={1}
                max={100}
                step={1}
                format={(v) => `${Math.round(v)} %`}
                onChange={(v) => set("keepFraction", v / 100)}
              />
              <Slider
                label={t("set.blockSize")}
                value={settings.blockSize}
                min={0}
                max={40}
                step={1}
                format={(v) => (v === 0 ? t("set.blockOff") : `${v} ${t("run.frames")}`)}
                onChange={(v) => set("blockSize", Math.round(v))}
              />
              <p className="note">
                <Icon name="alert" size={12} /> {t("set.blockNote")}
              </p>
              <Slider
                label={t("set.crop")}
                value={settings.cropSize}
                min={0}
                max={4000}
                step={100}
                format={(v) => (v === 0 ? t("set.cropAuto") : `${v} px`)}
                onChange={(v) => set("cropSize", Math.round(v))}
              />
            </div>
            <div className="toggle" style={{ marginTop: 14 }}>
              <Toggle
                checked={settings.tileSelect}
                onChange={(v) => set("tileSelect", v)}
                label={t("set.tileSelect")}
                description={t("set.tileSelectDesc")}
              />
            </div>
            {settings.tileSelect && (
              <div className="sliders" style={{ marginTop: 12 }}>
                <Slider
                  label={t("set.tileSize")}
                  value={settings.tileSize}
                  min={16}
                  max={256}
                  step={16}
                  format={(v) => `${v} px`}
                  onChange={(v) => set("tileSize", Math.round(v))}
                />
              </div>
            )}
          </div>

          {/* --- Enhancement --- */}
          <div className="block">
            <span className="group-title">{t("set.enhance")}</span>
            <div className="toggle" style={{ margin: "10px 0 0" }}>
              <Toggle
                checked={settings.enhance.enabled}
                onChange={(v) => setEnhance("enabled", v)}
                label={t("set.enhanceOn")}
                description={t("set.enhanceDesc")}
              />
            </div>
            {settings.enhance.enabled && (
              <div className="sliders" style={{ marginTop: 16 }}>
                <Slider
                  label={t("set.waveletAmount")}
                  value={settings.enhance.waveletAmount}
                  min={0}
                  max={3}
                  step={0.05}
                  onChange={(v) => setEnhance("waveletAmount", v)}
                />
                <Slider
                  label={t("set.waveletLevels")}
                  value={settings.enhance.waveletLevels}
                  min={1}
                  max={6}
                  step={1}
                  onChange={(v) => setEnhance("waveletLevels", Math.round(v))}
                />
                <Slider
                  label={t("set.clahe")}
                  value={settings.enhance.claheAmount}
                  min={0}
                  max={1}
                  step={0.05}
                  onChange={(v) => setEnhance("claheAmount", v)}
                />
                <Slider
                  label={t("set.clarity")}
                  value={settings.enhance.clarity}
                  min={0}
                  max={1}
                  step={0.02}
                  onChange={(v) => setEnhance("clarity", v)}
                />
                <Slider
                  label={t("set.gamma")}
                  value={settings.enhance.gamma}
                  min={0.5}
                  max={1.8}
                  step={0.05}
                  onChange={(v) => setEnhance("gamma", v)}
                />
              </div>
            )}
          </div>

          {/* --- Advanced --- */}
          <div className="block">
            <span className="group-title">{t("set.advanced")}</span>
            <div className="toggles" style={{ marginTop: 10 }}>
              <Toggle
                checked={settings.align}
                onChange={(v) => set("align", v)}
                label={t("set.align")}
              />
              <Toggle
                checked={settings.normalize}
                onChange={(v) => set("normalize", v)}
                label={t("set.normalize")}
              />
              <Toggle
                checked={settings.weighted}
                onChange={(v) => set("weighted", v)}
                label={t("set.weighted")}
              />
              <Toggle
                checked={settings.apEnabled}
                onChange={(v) => set("apEnabled", v)}
                label={t("set.ap")}
                description={t("set.apDesc")}
              />
            </div>
          </div>

          {/* --- Disk cache --- */}
          {cacheInfo && (
            <div className="block">
              <span className="group-title">{t("cache.title")}</span>
              <div className="recap" style={{ marginTop: 10 }}>
                <div className="recap-row">
                  <span>{t("cache.size")}</span>
                  <b>{formatBytes(cacheInfo.size, lang)}</b>
                </div>
                <div className="recap-row">
                  <span>{t("cache.frames")}</span>
                  <b>{cacheInfo.frames}</b>
                </div>
                {cacheInfo.stale > 0 && (
                  <div className="recap-row">
                    <span>{t("cache.stale")}</span>
                    <b>{cacheInfo.stale}</b>
                  </div>
                )}
              </div>
              <div style={{ display: "flex", gap: 8, marginTop: 12, flexWrap: "wrap" }}>
                <button
                  className="tbtn small"
                  disabled={busy || cacheInfo.stale === 0}
                  onClick={() => runCacheAction(onPruneCache)}
                >
                  <Icon name="refresh" size={13} /> {t("cache.prune")}
                </button>
                <button
                  className="tbtn small"
                  disabled={busy || cacheInfo.size === 0}
                  onClick={() => {
                    if (window.confirm(t("cache.confirmClear"))) {
                      runCacheAction(onClearCache);
                    }
                  }}
                >
                  <Icon name="reset" size={13} /> {t("cache.clear")}
                </button>
              </div>
            </div>
          )}
        </div>

        <div className="drawer-foot">
          <button className="tbtn small" onClick={() => onChange(DEFAULT_SETTINGS)}>
            <Icon name="reset" size={13} /> {t("set.defaults")}
          </button>
          <button className="btn primary" onClick={onClose}>
            <Icon name="check" size={15} /> {t("set.apply")}
          </button>
        </div>
      </aside>
    </>
  );
}
