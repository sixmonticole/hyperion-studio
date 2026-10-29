// Curseur avec libellé et valeur affichée.
// La piste se remplit à gauche du curseur (repère visuel immédiat).

export function Slider({
  label,
  value,
  min,
  max,
  step,
  onChange,
  format,
  unit,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  onChange: (v: number) => void;
  format?: (v: number) => string;
  unit?: string;
}) {
  const pct = max > min ? ((value - min) / (max - min)) * 100 : 0;
  const display = format ? format(value) : unit ? `${value} ${unit}` : String(value);

  return (
    <label className="slider">
      <div className="slider-head">
        <span>{label}</span>
        <output>{display}</output>
      </div>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        style={{ "--pct": `${pct}%` } as React.CSSProperties}
      />
    </label>
  );
}
