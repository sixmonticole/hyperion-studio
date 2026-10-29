// Jeu d'icônes SVG minimal (trait, 24x24) : évite toute dépendance externe et
// garde un rendu net et cohérent dans l'interface.

export type IconName =
  | "moon"
  | "sun"
  | "folder"
  | "folder-open"
  | "camera"
  | "check"
  | "alert"
  | "play"
  | "download"
  | "search"
  | "layers"
  | "sparkle"
  | "file"
  | "sliders"
  | "rotate"
  | "flip-h"
  | "flip-v"
  | "reset"
  | "refresh"
  | "x"
  | "arrow-left"
  | "arrow-right";

const PATHS: Record<IconName, string> = {
  moon: "M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8Z",
  sun: "M12 17a5 5 0 1 0 0-10 5 5 0 0 0 0 10Zm0-14v2m0 14v2M3 12h2m14 0h2M5.6 5.6l1.4 1.4m10 10 1.4 1.4m0-12.8-1.4 1.4m-10 10L5.6 18.4",
  folder: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z",
  "folder-open": "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v1M3 7v10a2 2 0 0 0 2 2h12.5a2 2 0 0 0 1.9-1.4L21 11H6Z",
  camera: "M4 8h3l1.5-2h7L17 8h3a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V9a1 1 0 0 1 1-1Zm8 8.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7Z",
  check: "M20 6 9 17l-5-5",
  alert: "M12 8v5M12 17h.01M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0Z",
  play: "M6 4l14 8-14 8V4Z",
  download: "M12 3v12m0 0 4-4m-4 4-4-4M4 21h16",
  search: "M11 19a8 8 0 1 0 0-16 8 8 0 0 0 0 16Zm10 2-4.35-4.35",
  layers: "m12 2 9 5-9 5-9-5 9-5Zm9 10-9 5-9-5m18 5-9 5-9-5",
  sparkle: "M12 3v4M12 17v4M3 12h4M17 12h4M6 6l2.5 2.5M15.5 15.5 18 18M18 6l-2.5 2.5M8.5 15.5 6 18",
  file: "M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8Zm0 0v5h5",
  sliders: "M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3M1 14h6M9 8h6M17 16h6",
  rotate: "M21 12a9 9 0 1 1-3-6.7M21 3v6h-6",
  "flip-h": "M12 3v18M8 7 3 12l5 5V7Zm8 0 5 5-5 5V7Z",
  "flip-v": "M3 12h18M7 8l5-5 5 5H7Zm0 8 5 5 5-5H7Z",
  reset: "M3 3v6h6M3.5 9a9 9 0 1 1 2.6 6.4",
  refresh: "M21 12a9 9 0 1 1-2.6-6.4M21 4v5h-5",
  x: "M18 6 6 18M6 6l12 12",
  "arrow-left": "M19 12H5M11 18l-6-6 6-6",
  "arrow-right": "M5 12h14M13 6l6 6-6 6",
};

export function Icon({
  name,
  size = 16,
  className,
}: {
  name: IconName;
  size?: number;
  className?: string;
}) {
  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.9}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d={PATHS[name]} />
    </svg>
  );
}
