#!/usr/bin/env python3
"""Génère la page de rendu de l'icône Hyperion Studio.

L'icône est composée dans un canvas navigateur : carré arrondi sombre
(« observatoire ») + logo en blanc (le H et l'orbite) + astre doré.
Le SVG du logo est embarqué en clair pour que `currentColor` soit résolu
en blanc, ce qui rend le monogramme lisible sur fond sombre.

Usage :
    python3 design/build_icon.py     # écrit public/make-icon.html
"""

import json
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent
LOGO = ROOT / "public" / "logo.svg"
OUT = ROOT / "public" / "make-icon.html"

svg = LOGO.read_text(encoding="utf-8")
viewbox = re.search(r'viewBox="([^"]+)"', svg).group(1)
paths = re.findall(r"<path[^>]*>", svg)

# Le logo utilise currentColor pour le monogramme : dans un SVG chargé seul
# (data URL), il n'y a pas de contexte CSS parent. On force donc le blanc
# directement dans l'attribut fill. L'astre doré garde sa couleur propre.
logo_inner = "\n".join(paths).replace('fill="currentColor"', 'fill="#ffffff"')

# Data URL prête à l'emploi : bien plus fiable qu'un blob SVG côté navigateur,
# car elle se charge de façon synchrone et garde ses dimensions intrinsèques.
logo_svg = (
    f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{viewbox}" '
    f'width="1095" height="1095" style="color:#ffffff">{logo_inner}</svg>'
)
from urllib.parse import quote

logo_data_url = "data:image/svg+xml;charset=utf-8," + quote(logo_svg, safe="")

PAGE = """<!doctype html>
<html>
<head>
<meta charset="utf-8">
<style>
  html,body{{margin:0;background:#1a1a1a;display:grid;place-items:center;min-height:100vh;gap:16px}}
  canvas{{box-shadow:0 20px 60px rgba(0,0,0,.5)}}
  #out{{color:#888;font:12px ui-monospace,monospace}}
</style>
</head>
<body>
<canvas id="c" width="1024" height="1024"></canvas>
<div id="out">…</div>
<script>
const LOGO_SRC = "{logo_data_url}";
// --- Icône Hyperion Studio --------------------------------------------------
// Carré arrondi sombre (fond « coupole ») + monogramme H blanc + astre doré.
const SIZE = 1024;
const c = document.getElementById("c");
const ctx = c.getContext("2d");

function roundRect(ctx, x, y, w, h, r) {{
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}}

function draw() {{
  ctx.clearRect(0, 0, SIZE, SIZE);

  // --- Fond : carré arrondi sombre, dégradé radial profond ---
  const r = SIZE * 0.225;
  const g = ctx.createRadialGradient(
    SIZE * 0.36, SIZE * 0.28, SIZE * 0.04,
    SIZE * 0.5, SIZE * 0.56, SIZE * 0.92
  );
  g.addColorStop(0, "#26303f");
  g.addColorStop(0.55, "#161d2b");
  g.addColorStop(1, "#0a0e16");
  ctx.fillStyle = g;
  roundRect(ctx, 0, 0, SIZE, SIZE, r);
  ctx.fill();

  // Liseré intérieur doré discret.
  ctx.save();
  roundRect(ctx, 2, 2, SIZE - 4, SIZE - 4, r - 2);
  ctx.strokeStyle = "rgba(240,185,92,0.22)";
  ctx.lineWidth = 3;
  ctx.stroke();
  ctx.restore();

  // Halo en haut à gauche : évite un fond plat et mort.
  const halo = ctx.createRadialGradient(
    SIZE * 0.30, SIZE * 0.24, 0,
    SIZE * 0.30, SIZE * 0.24, SIZE * 0.68
  );
  halo.addColorStop(0, "rgba(120,150,210,0.16)");
  halo.addColorStop(1, "rgba(120,150,210,0)");
  ctx.save();
  roundRect(ctx, 0, 0, SIZE, SIZE, r);
  ctx.clip();
  ctx.fillStyle = halo;
  ctx.fillRect(0, 0, SIZE, SIZE);
  ctx.restore();

  // --- Logo : monogramme blanc + astre doré, centré ---
  const src = new Image();
  src.onload = () => {{
    const inner = SIZE * 0.62;
    const ratio = src.width / src.height || 1;
    const w = ratio >= 1 ? inner : inner * ratio;
    const h = ratio >= 1 ? inner / ratio : inner;
    const x = (SIZE - w) / 2;
    const y = (SIZE - h) / 2;

    // Ombre portée douce sous le monogramme.
    ctx.save();
    ctx.shadowColor = "rgba(0,0,0,0.55)";
    ctx.shadowBlur = 30;
    ctx.shadowOffsetY = 10;
    ctx.drawImage(src, x, y, w, h);
    ctx.restore();

    // Halo doré autour de l'astre.
    ctx.save();
    roundRect(ctx, 0, 0, SIZE, SIZE, r);
    ctx.clip();
    const gold = ctx.createRadialGradient(
      SIZE * 0.64, SIZE * 0.60, 0,
      SIZE * 0.64, SIZE * 0.60, SIZE * 0.32
    );
    gold.addColorStop(0, "rgba(240,185,92,0.18)");
    gold.addColorStop(1, "rgba(240,185,92,0)");
    ctx.fillStyle = gold;
    ctx.fillRect(0, 0, SIZE, SIZE);
    ctx.restore();

    const data = c.toDataURL("image/png");
    document.getElementById("out").textContent = data.length + " bytes";
    window.__ICON_READY__ = data;
  }};
  src.onerror = () => {{ document.getElementById("out").textContent = "erreur de rendu du SVG"; }};
  src.src = LOGO_SRC;
}}

draw();
</script>
</body>
</html>
"""

OUT.write_text(PAGE.format(logo_data_url=logo_data_url), encoding="utf-8")
print(f"écrit {OUT} ({len(PAGE)} octets, {len(paths)} paths)")
