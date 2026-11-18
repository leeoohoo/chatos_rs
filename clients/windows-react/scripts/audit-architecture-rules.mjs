export function auditIconOnlyButtonAccessibleNames(source, path, violations) {
  const iconOnlyButtons = source.matchAll(/<button\b([^>]*)>\s*[×x]\s*<\/button>/gms);
  for (const match of iconOnlyButtons) {
    const openingTag = match[1];
    if (/\baria-label\s*=|\baria-labelledby\s*=/.test(openingTag)) {
      continue;
    }

    const line = source.slice(0, match.index ?? 0).split("\n").length;
    violations.push(path + ":" + line + ": icon-only buttons must provide aria-label or aria-labelledby");
  }
}

export function auditSecondaryTextContrast(source, path, violations) {
  const match = source.match(/--secondary\s*:\s*rgba\(\s*(\d+(?:\.\d+)?)\s*,\s*(\d+(?:\.\d+)?)\s*,\s*(\d+(?:\.\d+)?)\s*,\s*(\d*\.?\d+)\s*\)\s*;/);
  if (!match) {
    violations.push(path + ": --secondary must use an auditable rgba() color");
    return;
  }

  const foreground = match.slice(1, 4).map(Number);
  const alpha = Number(match[4]);
  if (foreground.some((channel) => channel < 0 || channel > 255) || alpha < 0 || alpha > 1) {
    violations.push(path + ": --secondary contains an invalid rgba() color");
    return;
  }

  const surfaces = [
    [255, 255, 255],
    [248, 248, 250],
    [241, 241, 246],
  ];
  const minimumRatio = Math.min(...surfaces.map((background) => {
    const composited = foreground.map((channel, index) => channel * alpha + background[index] * (1 - alpha));
    return contrastRatio(composited, background);
  }));

  if (minimumRatio < 4.5) {
    violations.push(path + ": --secondary text contrast must be at least 4.5:1 on core light surfaces (found " + minimumRatio.toFixed(3) + ":1)");
  }
}

function contrastRatio(first, second) {
  const firstLuminance = relativeLuminance(first);
  const secondLuminance = relativeLuminance(second);
  return (Math.max(firstLuminance, secondLuminance) + 0.05)
    / (Math.min(firstLuminance, secondLuminance) + 0.05);
}

function relativeLuminance(color) {
  const [red, green, blue] = color.map((channel) => {
    const normalized = channel / 255;
    return normalized <= 0.04045
      ? normalized / 12.92
      : ((normalized + 0.055) / 1.055) ** 2.4;
  });
  return red * 0.2126 + green * 0.7152 + blue * 0.0722;
}
