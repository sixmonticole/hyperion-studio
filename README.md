# Hyperion Studio

Pipeline de traitement d'astrophotographie lunaire/planétaire, en un seul
logiciel : il remplace la chaîne **PIPP + AutoStakkert**.

Il part d'un dossier de RAW (Sony ARW et autres, ou de TIFF 16-bit déjà
convertis) et produit un **stack TIFF 16-bit**, en faisant tout le travail
automatiquement :

```
RAW -> dématriçage (PPG) -> cache TIFF -> détection de l'objet -> crop
    -> score de netteté -> tri -> empilement par blocs
    -> post-traitement -> TIFF final
```

## État : V1 validée sur une session réelle de 200 poses

Session test : Sony A6000 (ILCE-6000), 6048×4024, 200 poses RAW de la Lune
(DSC04743 à DSC04942), trépied fixe.

Le résultat est **net** : cratères piqués (Tycho et ses rayons, Copernic,
Platon, Kepler, Aristarque), mers avec relief, limbe propre, bruit fortement
réduit.

### La découverte déterminante : pourquoi un gros empilement était flou

Pendant longtemps, empiler 200 poses donnait une image **plus floue** qu'en
empiler 20. Les tests successifs ont isolé la cause :

| Test | Résultat |
|---|---|
| 1 frame brute | molle (bruit d'une pose) |
| **20 frames du début de session** | **net** |
| **10 frames du milieu de session** | **net** |
| 200 frames | **flou** |

Sur cette session, la Lune **dérive de plusieurs centaines de pixels**
(jusqu'à ~630 px mesurés) et, sur ~30 minutes, elle **tourne et change
d'éclairage**. Empiler des frames très éloignées dans le temps revient à
moyenner des images qui ne se superposent plus : le détail se noie.

Ce n'était donc **ni la turbulence, ni le dématriçage, ni le post-traitement** :

- empiler 20 copies *identiques* d'une frame → image parfaitement nette
  (l'accumulation n'introduit aucun flou) ;
- empiler **sans aucun alignement ni rééchantillonnage** → flou également
  (le rééchantillonnage sub-pixel n'est pas coupable) ;
- empiler un **bloc de frames consécutives** → net.

### La solution retenue : empilement par blocs

```
session -> blocs de N frames consécutives -> empilage de chaque bloc
        -> alignement mutuel des blocs -> moyenne des blocs
```

À l'intérieur d'un bloc, la rotation est négligeable, donc chaque bloc est
net. Les blocs étant peu nombreux, leur alignement mutuel reste robuste malgré
la dérive globale.

C'est le **comportement par défaut** (`blockSize = 10`, réglable dans l'UI ou
désactivable avec 0).

## Chaîne validée

| Étape | Statut |
|---|---|
| Import RAW + cache TIFF 16-bit (reprise possible) | ✅ |
| Dématriçage RAW → RGB 16-bit (PPG de rawler) | ✅ |
| Balance des blancs neutre (gray-world) | ✅ |
| Détection automatique de l'objet (seuils haut/bas) | ✅ |
| Crop contenant tout l'objet, dérive comprise | ✅ |
| Score de netteté (Laplacien + Tenengrad) + tri | ✅ |
| Alignement par corrélation de phase (sub-pixel, bicubique) | ✅ |
| **Empilement par blocs** | ✅ |
| Empilement masqué (poids par pixel, bords préservés) | ✅ |
| Post-traitement : niveaux, ondelettes, CLAHE, clarté | ✅ |
| Rotation / miroir (orientation manuelle) | ✅ |
| Export TIFF 16-bit + rapport JSON | ✅ |
| Interface Tauri (dossier, progression, réglages) | ✅ |

### Post-traitement : ordre et protection des hautes lumières

Deux erreurs corrigées, qui rendaient l'image **plus molle que le stack brut** :

1. **Ordre du pipeline.** Les ondelettes étaient appliquées **avant** les
   niveaux. Le stack brut sortant très sombre (~0,1 en [0,1]), les
   coefficients de détail étaient minuscules et l'accentuation n'avait presque
   aucun effet. Ordre correct : **niveaux d'abord, ondelettes ensuite**.
2. **Hautes lumières écrasées.** Un étirement linéaire suivi d'un `clamp`
   aplatissait en blanc les zones brillantes (les rayons de Tycho). Une courbe
   de compression douce (*soft shoulder*) au-delà de 0,8 préserve le détail
   près du blanc.

## Architecture

```
src/                              UI React (Tauri)
  App.tsx                         dossier, progression, réglages
  App.css                         thème sombre
src-tauri/src/
  lib.rs                          commandes Tauri (scan_folder, run_stack)
  model.rs                        Settings, EnhanceSettings, Orientation, StackReport, Progress
  pipeline/
    mod.rs                        orchestrateur : stack_blockwise / stack_global
    decode.rs                     RAW -> RGB 16-bit (rawler, PPG + gray-world)
    cache.rs                      cache TIFF + manifeste
    detect.rs                     détection objet + fenêtre de crop
    quality.rs                    score de netteté (Laplacien + Tenengrad)
    align.rs                      alignement (corrélation de phase) + orientation
    similarity.rs                 alignement par similarité (dérotation) — instable
    ap.rs                         alignement à points locaux — instable
    stack.rs                      accumulateur (poids par pixel)
    enhance.rs                    niveaux, ondelettes, CLAHE, clarté
    output.rs                     écriture TIFF + rapport JSON
    tests.rs                      tests unitaires
src-tauri/src/bin/                outils de développement (--features devtools)
```

## Compilation et lancement

```bash
bun install
bun run tauri dev      # développement
bun run tauri build    # application macOS
cd src-tauri && cargo test
```

## Réglages

Tout est automatique par défaut. Le panneau « Réglages avancés » expose :

- **Frames conservées** : pourcentage des meilleures (défaut 25 %).
- **Taille du crop** : auto (détection) ou valeur en pixels.
- **Aligner les frames** : corrélation de phase (défaut oui).
- **Empilement par blocs** : taille des blocs, 0 = désactivé (défaut 10).
- **Normaliser la luminosité** / **Pondérer par la qualité** (défaut oui).
- **Points d'alignement locaux** : expérimental, dégrade (défaut non).
- **Orientation** : rotation 90/180, miroir H/V (défaut aucun).
- **Post-traitement** : ondelettes, CLAHE, clarté, gamma, points noir/blanc.

## Sortie

Dans `hyperion_out/` du dossier source :

- `*_stack.tif` : le stack final, RGB 16-bit non compressé.
- `*_stack.json` : le rapport (frames gardées, qualité moyenne, crop, réglages).

## Performances et volume disque

- 200 ARW : import + détection + empilement par blocs ≈ **7-15 min**.
- Cache chaud : ~2 min pour 200 frames (analyse + empilement + post-traitement).
- Un RAW A6000 dématriçé fait **~145 Mo** ; le cache de 200 frames pèse ~29 Go,
  celui de 428 frames ~62 Go. Supprimable sans risque (régénéré).

## Outils de développement

Compilés avec `--features devtools` :

```bash
cd src-tauri
cargo run --features devtools --release --bin runtest         -- <dossier> [cache] [fraction] [orientation] [enhance] [wavelet] [clahe] [clarté] [gamma] [noap] [noalign]
cargo run --features devtools --release --bin runtest_blocks  -- <dossier> <cache> [bloc] [keep_par_bloc]
cargo run --features devtools --release --bin pipeline_steps  -- <dossier> <out_dir> [n] [fraction]
cargo run --features devtools --release --bin preview         -- <in.tif> <out.png>
cargo run --features devtools --release --bin crop_preview    -- <in.tif> <out.png> <x> <y> <size> [scale]
cargo run --features devtools --release --bin compare_sharp   -- <A.tif> <B.tif>
cargo run --features devtools --release --bin profile_session -- <dossier_cache> [n]
cargo run --features devtools --release --bin diag_chain      -- <dossier_cache> [n]
```

`pipeline_steps` écrit une image par étape du pipeline (frame brute, moyenne
simple, frame alignée, stack brut, niveaux, ondelettes, final) : c'est l'outil
à utiliser pour **valider visuellement** où le détail se perd ou se gagne.

## Pistes pour la suite

1. **Alignement local (AP) correct** : la vraie réponse aux déformations
   locales de la turbulence. Points à fort contraste, masque du disque, champ
   de déplacement régularisé, un seul rééchantillonnage.
2. **Dérotation fiable** : permettrait d'empiler toute la session d'un coup au
   lieu de par blocs.
3. **Drizzle** (sur-échantillonnage) pour reconstruire du détail entre pixels.
4. **Lecture de flux vidéo** (SER/MOV/AVI) : la pratique standard du
   planétaire, où des milliers de poses très courtes figent la turbulence.

## Note sur les licences

Le décodage RAW s'appuie sur [`rawler`](https://crates.io/crates/rawler),
sous **LGPL-2.1**. Sans conséquence pour un usage personnel ; à garder en tête
pour une distribution binaire.
