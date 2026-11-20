export interface AgentTeamMentionCandidate {
  id: string;
  name: string;
  subtitle?: string | null;
}

export interface AgentTeamMentionQuery {
  start: number;
  value: string;
}

export function trailingAgentTeamMentionQuery(text: string): AgentTeamMentionQuery | null {
  const start = text.lastIndexOf("@");
  if (start < 0) return null;
  if (start > 0 && !isMentionBoundary(text[start - 1] ?? "")) return null;
  const value = text.slice(start + 1);
  if (value.length > 80 || /\s/u.test(value)) return null;
  return { start, value };
}

export function visibleAgentTeamMentionCandidates(
  text: string,
  candidates: AgentTeamMentionCandidate[],
) {
  const query = trailingAgentTeamMentionQuery(text);
  if (!query) return [];
  const value = normalize(query.value);
  return candidates
    .filter((candidate) => !value || normalize(candidate.name).includes(value))
    .sort((left, right) => {
      const leftPrefix = normalize(left.name).startsWith(value);
      const rightPrefix = normalize(right.name).startsWith(value);
      if (leftPrefix !== rightPrefix) return leftPrefix ? -1 : 1;
      return left.name.localeCompare(right.name, "zh-CN");
    })
    .slice(0, 6);
}

export function removeTrailingAgentTeamMentionQuery(text: string) {
  const query = trailingAgentTeamMentionQuery(text);
  return query ? text.slice(0, query.start) : text;
}

export function resolveAgentTeamMentionIds(
  text: string,
  selectedIds: string[],
  candidates: AgentTeamMentionCandidate[],
): { ids: string[]; error: string | null } {
  const validIds = new Set(candidates.map((candidate) => candidate.id));
  const resolved = new Set(selectedIds.filter((id) => validIds.has(id)));
  const names = new Map<string, AgentTeamMentionCandidate[]>();
  for (const candidate of candidates) {
    const key = normalize(candidate.name.trim());
    if (!key) continue;
    names.set(key, [...(names.get(key) ?? []), candidate]);
  }
  for (const group of names.values()) {
    const name = group[0]?.name.trim() ?? "";
    if (!containsAgentTeamMention(text, name)) continue;
    if (group.length === 1) resolved.add(group[0]!.id);
    else if (!group.some((candidate) => resolved.has(candidate.id))) {
      return { ids: [], error: `团队中有多个 Agent 名为“${name}”，请从 @ 候选列表选择具体成员。` };
    }
  }
  return { ids: [...resolved].sort(), error: null };
}

function containsAgentTeamMention(text: string, name: string) {
  if (!name) return false;
  const escaped = name.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
  return new RegExp(`(?:^|[\\s\\p{P}\\p{S}])@${escaped}(?=$|[\\s\\p{P}\\p{S}])`, "iu").test(text);
}

function isMentionBoundary(value: string) {
  return /[\s\p{P}\p{S}]/u.test(value);
}

function normalize(value: string) {
  return value.normalize("NFD").replace(/\p{M}/gu, "").toLocaleLowerCase();
}
