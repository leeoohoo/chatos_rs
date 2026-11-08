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
