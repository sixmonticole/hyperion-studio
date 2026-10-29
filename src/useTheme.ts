// Gestion du thème clair / sombre.
//
// Le thème est porté par l'attribut `data-theme` sur <html>. Il est mémorisé
// dans le stockage local pour être retrouvé au lancement suivant, et suit le
// réglage système la première fois.

import { useCallback, useEffect, useState } from "react";

export type Theme = "dark" | "light";

const STORAGE_KEY = "hyperion:theme";

/// Thème initial : le choix mémorisé, sinon la préférence système, sinon sombre.
function initialTheme(): Theme {
  if (typeof window === "undefined") return "dark";
  const saved = window.localStorage.getItem(STORAGE_KEY);
  if (saved === "dark" || saved === "light") return saved;
  return window.matchMedia?.("(prefers-color-scheme: light)").matches ? "light" : "dark";
}

export function useTheme() {
  const [theme, setTheme] = useState<Theme>(initialTheme);

  // Applique le thème au document et le mémorise.
  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
    try {
      window.localStorage.setItem(STORAGE_KEY, theme);
    } catch {
      // Stockage indisponible (mode privé) : on ignore, le thème reste pour la session.
    }
  }, [theme]);

  const toggle = useCallback(() => {
    setTheme((t) => (t === "dark" ? "light" : "dark"));
  }, []);

  return { theme, toggle };
}
