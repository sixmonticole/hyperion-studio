#!/usr/bin/env python3
"""Génère les prototypes HTML de l'UX Hyperion Studio.

Lit le logo réel (public/logo.svg) et le jeu d'icônes, puis écrit des pages
HTML autonomes (hors la feuille proto.css commune).

Usage : python3 design/build_protos.py
"""
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DESIGN = ROOT / "design"

LOGO = (ROOT / "public" / "logo.svg").read_text().split("?>", 1)[1].strip()
LOGO = re.sub(r'\swidth="\d+"\sheight="\d+"', "", LOGO)
ICONS = json.loads((DESIGN / "_icons.json").read_text())


def ic(name, size=15, stroke=2):
    return (
        f'<svg viewBox="0 0 24 24" width="{size}" height="{size}" fill="none" stroke="currentColor" '
        f'stroke-width="{stroke}" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">'
        f'<path d="{ICONS[name]}"/></svg>'
    )


def head(title, extra_css=""):
    return f"""<!doctype html>
<html lang="fr" data-theme="dark">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>{title}</title>
<link rel="stylesheet" href="proto.css" />
<style>{extra_css}</style>
</head>"""


def header(theme_icon="sun", badges=""):
    return f"""<header class="header">
  <div class="brand">
    <div class="logo">{LOGO}</div>
    <div>
      <h1>Hyperion Studio</h1>
      <div class="brand-sub">Observatoire · Empilement astro</div>
    </div>
  </div>
  <div class="header-right">
    {badges}
    <button class="icon-btn" onclick="toggleTheme()" title="Changer de thème">{ic(theme_icon,16,2)}</button>
  </div>
</header>"""


THEME_JS = """
<script>
// Bascule de thème des prototypes (indépendante de l'app).
function toggleTheme(){
  const r = document.documentElement;
  const t = r.getAttribute('data-theme') === 'dark' ? 'light' : 'dark';
  r.setAttribute('data-theme', t);
  document.querySelectorAll('.icon-btn[title="Changer de thème"] svg path').forEach(p=>{
    p.setAttribute('d', t==='dark'
      ? '""" + ICONS["sun"] + """'
      : '""" + ICONS["moon"] if "moon" in ICONS else ICONS["sun"] + """');
  });
}
</script>"""

THEME_JS = """
<script>
function toggleTheme(){
  const r=document.documentElement;
  r.setAttribute('data-theme', r.getAttribute('data-theme')==='dark'?'light':'dark');
}
</script>"""


# ---------------------------------------------------------------------------
# PAGE 1 — ASSISTANT PAS À PAS
# ---------------------------------------------------------------------------
def page_assistant():
    css = """
    /* Barre d'étapes de l'assistant */
    .wizard{max-width:820px;margin:0 auto}
    .wiz-steps{display:flex;align-items:center;gap:0;margin-bottom:34px}
    .wiz-step{display:flex;align-items:center;gap:12px;flex:1}
    .wiz-step .bullet{width:30px;height:30px;flex-shrink:0;border-radius:50%;display:grid;place-items:center;
      border:1px solid var(--line);background:var(--card);color:var(--text-4);font-family:var(--mono);font-size:12px;
      transition:all var(--t-slow)}
    .wiz-step .lbl{display:flex;flex-direction:column}
    .wiz-step .lbl b{font-size:13px;font-weight:600;color:var(--text-3);transition:color var(--t-slow)}
    .wiz-step .lbl span{font-size:11px;color:var(--text-4)}
    .wiz-step .rail{flex:1;height:1px;background:var(--line);margin:0 14px}
    .wiz-step.done .bullet{color:var(--ok);border-color:var(--ok-line);background:var(--ok-dim)}
    .wiz-step.done .lbl b{color:var(--ok)}
    .wiz-step.active .bullet{color:var(--gold-2);border-color:var(--gold-line);background:var(--gold-dim);
      box-shadow:0 0 0 4px color-mix(in srgb,var(--gold) 10%,transparent)}
    .wiz-step.active .lbl b{color:var(--paper)}
    .wiz-body{background:var(--card);border:1px solid var(--line);border-radius:var(--r-lg);padding:30px}
    .wiz-title{font-family:var(--serif);font-size:24px;font-weight:600;color:var(--paper);margin-bottom:6px}
    .wiz-desc{color:var(--text-3);font-size:14px;margin-bottom:26px;max-width:560px}
    .wiz-foot{display:flex;justify-content:space-between;align-items:center;margin-top:28px;
      padding-top:22px;border-top:1px solid var(--line-soft)}
    .recap{display:flex;flex-direction:column;gap:12px;margin-top:22px}
    .recap-row{display:flex;justify-content:space-between;align-items:baseline;gap:14px;font-size:13px;
      padding-bottom:12px;border-bottom:1px solid var(--line-soft)}
    .recap-row span{color:var(--text-3)} .recap-row b{color:var(--text);font-family:var(--mono);font-weight:500}
    """
    body = f"""
{THEME_JS}
<div class="app">
  {header()}
  <div class="wizard">
    <div class="wiz-steps">
      <div class="wiz-step done">
        <span class="bullet">{ic('check',14,2.6)}</span>
        <span class="lbl"><b>Dossier</b><span>428 images</span></span>
        <span class="rail"></span>
      </div>
      <div class="wiz-step active">
        <span class="bullet">02</span>
        <span class="lbl"><b>Réglages</b><span>à ajuster</span></span>
        <span class="rail"></span>
      </div>
      <div class="wiz-step">
        <span class="bullet">03</span>
        <span class="lbl"><b>Traitement</b><span>puis résultat</span></span>
      </div>
    </div>

    <div class="wiz-body">
      <div class="wiz-title">Réglages de l'empilement</div>
      <p class="wiz-desc">Tout fonctionne déjà sans rien toucher. Ajustez seulement si vous le souhaitez,
         ou ouvrez le volet pour les réglages avancés.</p>

      <div class="group" style="margin-top:0;border-top:none;padding-top:0">
        <div class="group-head"><span class="group-title">Orientation</span>
          <span class="hint">l'EXIF ignore le retournement du télescope</span></div>
        <div class="chips">
          <button class="chip">{ic('rotate',13)} 90°</button>
          <button class="chip">{ic('rotate',13)} 180°</button>
          <button class="chip">Miroir H</button>
          <button class="chip">Miroir V</button>
          <button class="chip">{ic('reset',13)} Réinitialiser</button>
        </div>
      </div>

      <div class="group">
        <div class="group-head"><span class="group-title">Post-traitement</span></div>
        <label class="toggle">
          <span class="switch on"><span class="knob"></span></span>
          <span class="toggle-text">
            <span class="toggle-label">Améliorer l'image finale</span>
            <span class="toggle-desc">Netteté par ondelettes et contraste. Désactivé = stack brut.</span>
          </span>
        </label>
      </div>

      <div class="recap">
        <div class="recap-row"><span>Frames conservées</span><b>25 %</b></div>
        <div class="recap-row"><span>Empilement par blocs</span><b>10 frames</b></div>
        <div class="recap-row"><span>Alignement</span><b>activé</b></div>
        <div class="recap-row"><span>Crop</span><b>automatique</b></div>
      </div>

      <div class="wiz-foot">
        <button class="tbtn">{ic('arrow-left',15)} Retour</button>
        <div class="actions">
          <button class="tbtn">{ic('sliders',15)} Tous les réglages</button>
          <button class="btn primary">{ic('play',15)} Lancer le traitement</button>
        </div>
      </div>
    </div>
  </div>
</div>
</body></html>"""
    (DESIGN / "proto-1-assistant.html").write_text(head("Hyperion Studio — Assistant", css) + body)
    print("proto-1-assistant.html")


# ---------------------------------------------------------------------------
# PAGE 2 — VOLET DE RÉGLAGES (drawer)
# ---------------------------------------------------------------------------
def page_drawer():
    css = """
    .stage{position:relative;min-height:640px;border:1px solid var(--line);border-radius:var(--r-lg);
      overflow:hidden;background:linear-gradient(180deg,var(--card),var(--card-2))}
    .stage-inner{padding:40px;filter:blur(1px);opacity:.45}
    .scrim{position:absolute;inset:0;background:color-mix(in srgb,var(--ink) 55%,transparent);
      backdrop-filter:blur(2px)}
    .drawer{position:absolute;top:0;right:0;bottom:0;width:430px;max-width:92%;
      background:var(--card-2);border-left:1px solid var(--line);display:flex;flex-direction:column;
      box-shadow:-20px 0 60px rgba(0,0,0,.3)}
    .drawer-head{display:flex;align-items:center;justify-content:space-between;gap:12px;
      padding:22px 24px;border-bottom:1px solid var(--line)}
    .drawer-head h2{font-family:var(--serif);font-size:19px;font-weight:600;color:var(--paper)}
    .drawer-body{flex:1;overflow:auto;padding:24px;display:flex;flex-direction:column;gap:22px}
    .drawer-foot{padding:20px 24px;border-top:1px solid var(--line);display:flex;justify-content:space-between;align-items:center;gap:12px}
    .block{border:1px solid var(--line-soft);border-radius:var(--r);padding:18px;background:var(--card)}
    .block .group-title{display:block;margin-bottom:4px}
    .block .blk-desc{font-size:12px;color:var(--text-4);margin-bottom:14px;line-height:1.5}
    """
    body = f"""
{THEME_JS}
<div class="app">
  {header(badges='<span class="badge">428 images</span><span class="badge info">cache prêt</span>')}
  <div class="stage">
    <div class="stage-inner">
      <div class="panel" style="margin-bottom:18px">
        <h2>Dossier sélectionné</h2>
        <p class="path">/Users/simon/Documents/Photo/lune 25 oct</p>
      </div>
      <div class="panel">
        <div class="steps">
          <div class="step done"><span class="step-n">01</span>Import RAW</div>
          <div class="step done"><span class="step-n">02</span>Analyse</div>
          <div class="step active"><span class="step-n">03</span>Empilement</div>
          <div class="step"><span class="step-n">04</span>Post-traitement</div>
          <div class="step"><span class="step-n">05</span>Export TIFF</div>
        </div>
      </div>
    </div>
    <div class="scrim"></div>

    <aside class="drawer">
      <div class="drawer-head">
        <h2>Réglages</h2>
        <button class="icon-btn">{ic('x',16)}</button>
      </div>
      <div class="drawer-body">
        <div class="block">
          <span class="group-title">Orientation</span>
          <p class="blk-desc">Le capteur ne sait pas comment le télescope était retourné.</p>
          <div class="chips">
            <button class="chip">{ic('rotate',13)} 90°</button>
            <button class="chip">{ic('rotate',13)} 180°</button>
            <button class="chip on">Miroir H</button>
            <button class="chip">Miroir V</button>
          </div>
        </div>

        <div class="block">
          <span class="group-title">Frames & empilement</span>
          <div class="sliders" style="margin-top:8px">
            <label class="slider"><div class="slider-head"><span>Frames conservées</span><output>25 %</output></div>
              <input type="range" min="1" max="100" value="25" style="--pct:25%"></label>
            <label class="slider"><div class="slider-head"><span>Empilement par blocs</span><output>10 frames</output></div>
              <input type="range" min="0" max="40" value="10" style="--pct:25%"></label>
            <p class="note">{ic('alert',12)} Sur une longue session, la Lune tourne : empiler par petits
              groupes consécutifs évite le flou final.</p>
            <label class="slider"><div class="slider-head"><span>Taille du crop</span><output>auto</output></div>
              <input type="range" min="0" max="4000" value="0" style="--pct:0%"></label>
          </div>
        </div>

        <div class="block">
          <span class="group-title">Amélioration</span>
          <label class="toggle" style="margin:10px 0 16px">
            <span class="switch on"><span class="knob"></span></span>
            <span class="toggle-text"><span class="toggle-label">Activer le post-traitement</span>
            <span class="toggle-desc">Sans effet sur le TIFF brut si désactivé.</span></span>
          </label>
          <div class="sliders">
            <label class="slider"><div class="slider-head"><span>Netteté (ondelettes)</span><output>1.40</output></div>
              <input type="range" min="0" max="3" step="0.05" value="1.4" style="--pct:47%"></label>
            <label class="slider"><div class="slider-head"><span>Échelles d'ondelettes</span><output>4</output></div>
              <input type="range" min="1" max="6" value="4" style="--pct:60%"></label>
            <label class="slider"><div class="slider-head"><span>Contraste local (CLAHE)</span><output>0.15</output></div>
              <input type="range" min="0" max="1" step="0.05" value="0.15" style="--pct:15%"></label>
            <label class="slider"><div class="slider-head"><span>Gamma</span><output>1.00</output></div>
              <input type="range" min="0.5" max="1.8" step="0.05" value="1" style="--pct:38%"></label>
          </div>
        </div>

        <div class="block">
          <span class="group-title">Avancé</span>
          <div class="toggles" style="margin-top:10px">
            <label class="toggle"><span class="switch on"><span class="knob"></span></span>
              <span class="toggle-text"><span class="toggle-label">Aligner les frames</span></span></label>
            <label class="toggle"><span class="switch on"><span class="knob"></span></span>
              <span class="toggle-text"><span class="toggle-label">Normaliser la luminosité</span></span></label>
            <label class="toggle"><span class="switch"><span class="knob"></span></span>
              <span class="toggle-text"><span class="toggle-label">Points d'alignement locaux</span>
              <span class="toggle-desc">Expérimental — dégrade l'image.</span></span></label>
          </div>
        </div>
      </div>
      <div class="drawer-foot">
        <button class="tbtn small">{ic('reset',13)} Valeurs par défaut</button>
        <button class="btn primary">{ic('check',15)} Appliquer</button>
      </div>
    </aside>
  </div>
</div>
</body></html>"""
    (DESIGN / "proto-2-reglages.html").write_text(head("Hyperion Studio — Réglages", css) + body)
    print("proto-2-reglages.html")


# ---------------------------------------------------------------------------
# PAGE 3 — ÉCRAN DE RÉSULTAT
# ---------------------------------------------------------------------------
def page_result():
    badge_done = '<span class="badge ok">' + ic("check", 12, 2.6) + " terminé</span>"
    moon = """<svg xmlns='http://www.w3.org/2000/svg' width='900' height='620'>
      <defs>
        <radialGradient id='bg' cx='50%25' cy='45%25'><stop offset='0%25' stop-color='%23111826'/><stop offset='100%25' stop-color='%2305080f'/></radialGradient>
        <radialGradient id='m' cx='44%25' cy='38%25'><stop offset='0%25' stop-color='%23f2eee6'/><stop offset='62%25' stop-color='%23a8a294'/><stop offset='100%25' stop-color='%234a4436'/></radialGradient>
      </defs>
      <rect width='900' height='620' fill='url(%23bg)'/>
      <circle cx='450' cy='300' r='215' fill='url(%23m)'/>
      <circle cx='392' cy='246' r='34' fill='%23cdc7b6' opacity='.5'/>
      <circle cx='504' cy='330' r='22' fill='%23c3bdad' opacity='.45'/>
      <circle cx='450' cy='150' r='15' fill='%23ddd8ca' opacity='.42'/>
      <circle cx='360' cy='392' r='12' fill='%23b8b2a3' opacity='.4'/>
      <g fill='%23ffffff'><circle cx='110' cy='80' r='1.4' opacity='.8'/><circle cx='780' cy='120' r='1.2' opacity='.6'/>
      <circle cx='180' cy='500' r='1.1' opacity='.5'/><circle cx='740' cy='510' r='1.5' opacity='.7'/></g>
    </svg>"""
    css = """
    .result-hero{border:1px solid var(--line);border-radius:var(--r-lg);overflow:hidden;background:#000;position:relative}    .result-hero img{display:block;width:100%}
    .result-hero .hint{position:absolute;left:14px;bottom:14px;padding:6px 12px;border-radius:var(--r);
      font-family:var(--mono);font-size:11px;background:color-mix(in srgb,var(--ink) 82%,transparent);
      border:1px solid var(--gold-line);color:var(--gold-2);backdrop-filter:blur(8px)}
    .result-grid{display:grid;grid-template-columns:minmax(0,1fr) 320px;gap:22px;align-items:start}
    @media(max-width:1000px){.result-grid{grid-template-columns:1fr}}
    .stat-cards{display:grid;grid-template-columns:1fr 1fr;gap:12px}
    .stat-card{background:var(--card);border:1px solid var(--line);border-radius:var(--r);padding:18px}
    .stat-card .stat-value{font-size:28px}
    .reveal{display:flex;flex-direction:column;gap:8px}
    .reveal img{width:100%;border-radius:var(--r);border:1px solid var(--line);image-rendering:pixelated}
    .tabs{display:flex;gap:6px;margin-bottom:16px}
    .tab{padding:7px 14px;border-radius:var(--r);font-size:12.5px;font-weight:600;color:var(--text-3);
      border:1px solid transparent;cursor:pointer;background:none}
    .tab.on{color:var(--gold-2);background:var(--gold-dim);border-color:var(--gold-line)}
    """
    body = f"""
{THEME_JS}
<div class="app">
  {header(badges=badge_done)}
  <div class="result-grid">
    <div>
      <div class="panel" style="padding:20px;margin-bottom:18px">
        <div style="display:flex;justify-content:space-between;align-items:flex-start;gap:16px;flex-wrap:wrap">
          <div>
            <h2 style="color:var(--ok);display:flex;align-items:center;gap:8px">
              {ic('check',18,2.4)} Stack terminé</h2>
            <p class="path">…/lune 25 oct/hyperion_out/lune 25 oct_stack.tif</p>
          </div>
          <div class="actions">
            <button class="tbtn">{ic('refresh',13)} Aperçu</button>
            <button class="tbtn">{ic('download',13)} Enregistrer sous</button>
            <button class="btn small">{ic('folder-open',13)} Ouvrir le dossier</button>
          </div>
        </div>
      </div>

      <div class="tabs">
        <button class="tab on">Image finale</button>
        <button class="tab">Stack brut</button>
        <button class="tab">Comparer</button>
      </div>

      <div class="result-hero">
        <img src="data:image/svg+xml;utf8,{moon}" alt="Résultat" />
        <span class="hint">Cliquez pour inspecter à 100 %</span>
      </div>

      <div class="reveal" style="margin-top:18px">
        <span class="group-title">Vue 1:1 — netteté réelle</span>
        <img src="data:image/svg+xml;utf8,{moon}" alt="Détail 1:1" style="max-width:520px" />
      </div>
    </div>

    <aside style="display:flex;flex-direction:column;gap:18px">
      <div class="stat-cards">
        <div class="stat-card"><span class="stat-value">200<em class="stat-sub">/200</em></span>
          <span class="stat-label">frames empilées</span></div>
        <div class="stat-card"><span class="stat-value">26</span>
          <span class="stat-label">qualité moyenne</span></div>
        <div class="stat-card"><span class="stat-value">3620<em class="stat-sub">²</em></span>
          <span class="stat-label">taille du crop</span></div>
        <div class="stat-card"><span class="stat-value">16<em class="stat-sub">bit</em></span>
          <span class="stat-label">profondeur</span></div>
      </div>

      <div class="panel side">
        <h3>Détails du traitement</h3>
        <ul class="summary">
          <li><span>Empilement</span><b>par blocs (10)</b></li>
          <li><span>Alignement</span><b>activé</b></li>
          <li><span>Post-traitement</span><b>activé</b></li>
          <li><span>Orientation</span><b>0°</b></li>
          <li><span>Durée</span><b>3 min 12 s</b></li>
        </ul>
      </div>

      <div class="panel side">
        <h3>Et ensuite ?</h3>
        <p class="note">Le TIFF 16 bits est prêt pour Luminar, Photoshop ou GIMP.
           Pas de compression, pas de perte.</p>
        <button class="btn small" style="margin-top:14px;width:100%">{ic('folder-open',13)} Révéler dans le Finder</button>
      </div>
    </aside>
  </div>
</div>
</body></html>"""
    (DESIGN / "proto-3-resultat.html").write_text(head("Hyperion Studio — Résultat", css) + body)
    print("proto-3-resultat.html")


if __name__ == "__main__":
    page_assistant()
    page_drawer()
    page_result()
    print("prototypes generes dans design/")
