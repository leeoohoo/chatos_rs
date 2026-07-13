// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

type BrandMarkProps = {
  className?: string;
};

export function BrandMark({ className = 'brand-mark' }: BrandMarkProps) {
  return (
    <span className={className} aria-hidden="true">
      <img src="/brand/okra-logo-mark.png" alt="" />
    </span>
  );
}
