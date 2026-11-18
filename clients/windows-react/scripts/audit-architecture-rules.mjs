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

export function auditAccentBackgroundToken(source, path, violations) {
  const matches = source.matchAll(/background(?:-color)?\s*:\s*var\(--accent\)(?![-\w])/g);
  for (const match of matches) {
    const line = source.slice(0, match.index ?? 0).split("\n").length;
    violations.push(path + ":" + line + ": backgrounds must use --accent-action so dark foreground contrast stays independent");
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

export function auditAccentTextContrast(source, path, violations) {
  const accent = readHexToken(source, "accent");
  const action = readHexToken(source, "accent-action");
  const darkAccent = readHexToken(source, "accent-dark");
  if (!accent || !action || !darkAccent) {
    violations.push(path + ": --accent, --accent-action, and --accent-dark must use auditable six-digit hex colors");
    return;
  }
  if (!/@media\s*\(prefers-color-scheme:\s*dark\)[\s\S]*?:root\s*\{[^}]*--accent\s*:\s*var\(--accent-dark\)\s*;/i.test(source)) {
    violations.push(path + ": dark mode must map --accent to --accent-dark");
  }

  const lightSurfaces = [
    [255, 255, 255],
    [248, 248, 250],
    [241, 241, 246],
  ];
  const lightForegroundRatio = Math.min(...lightSurfaces.map((background) => contrastRatio(accent, background)));
  if (lightForegroundRatio < 4.5) {
    violations.push(path + ": --accent text contrast must be at least 4.5:1 on core light surfaces (found " + lightForegroundRatio.toFixed(3) + ":1)");
  }

  const whiteActionRatio = contrastRatio([255, 255, 255], action);
  if (whiteActionRatio < 4.5) {
    violations.push(path + ": white action text contrast on --accent-action must be at least 4.5:1 (found " + whiteActionRatio.toFixed(3) + ":1)");
  }

  const darkSurfaces = [
    [42, 42, 46],
    [35, 35, 39],
    [28, 28, 31],
  ];
  const darkForegroundRatio = Math.min(...darkSurfaces.map((background) => contrastRatio(darkAccent, background)));
  if (darkForegroundRatio < 4.5) {
    violations.push(path + ": --accent-dark text contrast must be at least 4.5:1 on core dark surfaces (found " + darkForegroundRatio.toFixed(3) + ":1)");
  }
}

function readHexToken(source, name) {
  const match = source.match(new RegExp("--" + name + "\\s*:\\s*#([0-9a-f]{6})\\s*;", "i"));
  if (!match) return null;
  return [0, 2, 4].map((offset) => Number.parseInt(match[1].slice(offset, offset + 2), 16));
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
