// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export const LocalMemoryPolicyNumberInput = ({
  label,
  value,
  disabled,
  min,
  max,
  onChange,
}: {
  label: string;
  value: number;
  disabled: boolean;
  min: number;
  max: number;
  onChange: (value: number) => void;
}) => (
  <label className="text-[11px] text-muted-foreground">
    <span>{label}</span>
    <input
      type="number"
      className="mt-1 w-full rounded border border-border bg-background px-2 py-1 text-xs text-foreground"
      value={value}
      min={min}
      max={max}
      disabled={disabled}
      onChange={(event) => onChange(clampNumber(event.target.value, min, max, value))}
    />
  </label>
);

const clampNumber = (value: string, min: number, max: number, fallback: number): number => {
  const numeric = Number(value);
  return Number.isFinite(numeric) ? Math.min(max, Math.max(min, Math.trunc(numeric))) : fallback;
};
