const ENTITIES: Record<string, string> = {
  amp: '&',
  apos: "'",
  gt: '>',
  lt: '<',
  quot: '"'
};

export function decodeXmlText(value: string): string {
  return value.replace(/&(#x[0-9a-f]+|#[0-9]+|[a-z]+);/gi, (match, entity: string) => {
    if (entity.startsWith('#x')) return String.fromCodePoint(Number.parseInt(entity.slice(2), 16));
    if (entity.startsWith('#')) return String.fromCodePoint(Number.parseInt(entity.slice(1), 10));
    return ENTITIES[entity] ?? match;
  });
}

export function firstTagText(xml: string | undefined, tagName: string): string | undefined {
  if (!xml) return undefined;
  const escaped = tagName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const match = xml.match(new RegExp(`<${escaped}(?:\\s[^>]*)?>([\\s\\S]*?)<\\/${escaped}>`, 'i'));
  if (!match?.[1]) return undefined;
  return decodeXmlText(match[1].replace(/<[^>]+>/g, '')).trim() || undefined;
}

export function countTags(xml: string | undefined, qualifiedName: string): number {
  if (!xml) return 0;
  const escaped = qualifiedName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  return xml.match(new RegExp(`<${escaped}(?:\\s|>|\\/)`, 'g'))?.length ?? 0;
}

export function attributeValues(xml: string | undefined, tagName: string, attribute: string): string[] {
  if (!xml) return [];
  const escapedTag = tagName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const tag = tagName.includes(':')
    ? escapedTag
    : `(?:[A-Za-z_][A-Za-z0-9_.-]*:)?${escapedTag}`;
  const attr = attribute.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const expression = new RegExp(`<${tag}(?:\\s[^>]*)?\\s${attr}=(?:"([^"]*)"|'([^']*)')`, 'gi');
  return Array.from(xml.matchAll(expression), (match) => decodeXmlText(match[1] ?? match[2] ?? ''));
}

export function tagTexts(xml: string | undefined, tagName: string): string[] {
  if (!xml) return [];
  const tag = tagName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const expression = new RegExp(`<${tag}(?:\\s[^>]*)?>([\\s\\S]*?)<\\/${tag}>`, 'gi');
  return Array.from(xml.matchAll(expression), (match) =>
    decodeXmlText((match[1] ?? '').replace(/<[^>]+>/g, '')).trim()
  ).filter(Boolean);
}
