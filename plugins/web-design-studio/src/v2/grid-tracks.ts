export type SceneGridTrack =
  | { kind: 'fixed'; value: number }
  | { kind: 'percent'; value: number }
  | { kind: 'fraction'; value: number }
  | { kind: 'auto' }
  | { kind: 'minmax'; min: SceneGridTrackBase; max: SceneGridTrackBase }
  | { kind: 'repeat'; count: number | 'auto-fit' | 'auto-fill'; track: Exclude<SceneGridTrack, { kind: 'repeat' }> };

export type SceneGridTrackBase = Exclude<SceneGridTrack, { kind: 'repeat' | 'minmax' }>;

export interface ExpandedSceneGridTrack {
  source: string;
  track: Exclude<SceneGridTrack, { kind: 'repeat' }>;
}

const numberSource = '(?:0|[1-9]\\d*)(?:\\.\\d+)?';
const fixedPattern = new RegExp(`^(${numberSource})px$`, 'i');
const percentPattern = new RegExp(`^(${numberSource})%$`, 'i');
const fractionPattern = new RegExp(`^(${numberSource})fr$`, 'i');

function splitArguments(source: string): [string, string] {
  let depth = 0;
  for (let index = 0; index < source.length; index += 1) {
    if (source[index] === '(') depth += 1;
    if (source[index] === ')') depth -= 1;
    if (source[index] === ',' && depth === 0) return [source.slice(0, index).trim(), source.slice(index + 1).trim()];
  }
  throw new Error(`Grid track ${source} needs two arguments.`);
}

function parseBaseTrack(source: string): SceneGridTrackBase {
  const normalized = source.trim().toLowerCase();
  if (normalized === 'auto') return { kind: 'auto' };
  const fixed = normalized.match(fixedPattern);
  if (fixed) return { kind: 'fixed', value: Number(fixed[1]) };
  const percent = normalized.match(percentPattern);
  if (percent) return { kind: 'percent', value: Number(percent[1]) / 100 };
  const fraction = normalized.match(fractionPattern);
  if (fraction && Number(fraction[1]) > 0) return { kind: 'fraction', value: Number(fraction[1]) };
  throw new Error(`Grid track ${source} is unsupported.`);
}

export function parseSceneGridTrack(source: string): SceneGridTrack {
  if (typeof source !== 'string' || !source.trim()) throw new Error('Grid track must be a non-empty string.');
  const normalized = source.trim().toLowerCase();
  if (normalized.startsWith('minmax(') && normalized.endsWith(')')) {
    const [minimumSource, maximumSource] = splitArguments(normalized.slice(7, -1));
    const min = parseBaseTrack(minimumSource);
    const max = parseBaseTrack(maximumSource);
    if (min.kind === 'fraction') throw new Error(`Grid track ${source} cannot use fr as a minmax minimum.`);
    if (max.kind === 'fixed' && min.kind === 'fixed' && max.value < min.value) throw new Error(`Grid track ${source} has reversed minmax bounds.`);
    if (max.kind === 'percent' && min.kind === 'percent' && max.value < min.value) throw new Error(`Grid track ${source} has reversed minmax bounds.`);
    return { kind: 'minmax', min, max };
  }
  if (normalized.startsWith('repeat(') && normalized.endsWith(')')) {
    const [countSource, trackSource] = splitArguments(normalized.slice(7, -1));
    const count = countSource === 'auto-fit' || countSource === 'auto-fill'
      ? countSource
      : Number(countSource);
    if (typeof count === 'number' && (!Number.isSafeInteger(count) || count < 1 || count > 256)) throw new Error(`Grid track ${source} has an invalid repeat count.`);
    const track = parseSceneGridTrack(trackSource);
    if (track.kind === 'repeat') throw new Error(`Grid track ${source} cannot nest repeat().`);
    if ((count === 'auto-fit' || count === 'auto-fill') && minimumTrackSize(track, 0) <= 0) {
      throw new Error(`Grid track ${source} needs a measurable minimum for automatic repetition.`);
    }
    return { kind: 'repeat', count, track };
  }
  return parseBaseTrack(normalized);
}

export function assertSceneGridTracks(tracks: string[], label: string, allowEmpty = false): void {
  if (!Array.isArray(tracks) || (!allowEmpty && tracks.length === 0)) throw new Error(`${label} needs at least one track.`);
  if (tracks.length > 256) throw new Error(`${label} has too many tracks.`);
  for (const track of tracks) parseSceneGridTrack(track);
}

export function minimumTrackSize(track: Exclude<SceneGridTrack, { kind: 'repeat' }>, available: number): number {
  if (track.kind === 'fixed') return track.value;
  if (track.kind === 'percent') return track.value * available;
  if (track.kind === 'minmax') return minimumTrackSize(track.min, available);
  return 0;
}

export function expandSceneGridTracks(
  sources: string[],
  available: number,
  gap: number,
  itemCount: number
): ExpandedSceneGridTrack[] {
  const expanded: ExpandedSceneGridTrack[] = [];
  for (const source of sources) {
    const parsed = parseSceneGridTrack(source);
    if (parsed.kind !== 'repeat') {
      expanded.push({ source, track: parsed });
      continue;
    }
    let count: number;
    if (typeof parsed.count === 'number') count = parsed.count;
    else {
      const minimum = minimumTrackSize(parsed.track, available);
      count = Math.max(1, Math.floor((available + gap) / (minimum + gap)));
      if (parsed.count === 'auto-fit' && itemCount > 0) count = Math.min(count, itemCount);
    }
    for (let index = 0; index < count; index += 1) expanded.push({ source, track: parsed.track });
  }
  if (expanded.length > 256) throw new Error('Expanded grid has too many tracks.');
  return expanded;
}

export function resolveSceneGridTracks(
  tracks: ExpandedSceneGridTrack[],
  available: number,
  gap: number,
  autoContentSizes: number[] = []
): number[] {
  if (tracks.length === 0) return [];
  const availableForTracks = Math.max(0, available - Math.max(0, tracks.length - 1) * gap);
  const bases: number[] = [];
  const fractionWeights: number[] = [];
  for (const [index, entry] of tracks.entries()) {
    const content = autoContentSizes[index] ?? 0;
    const track = entry.track;
    if (track.kind === 'fixed') {
      bases.push(track.value);
      fractionWeights.push(0);
    } else if (track.kind === 'percent') {
      bases.push(track.value * availableForTracks);
      fractionWeights.push(0);
    } else if (track.kind === 'auto') {
      bases.push(content);
      fractionWeights.push(0);
    } else if (track.kind === 'fraction') {
      bases.push(0);
      fractionWeights.push(track.value);
    } else {
      const minimum = Math.max(minimumTrackSize(track.min, availableForTracks), track.min.kind === 'auto' ? content : 0);
      if (track.max.kind === 'fraction') {
        bases.push(minimum);
        fractionWeights.push(track.max.value);
      } else {
        const maximum = track.max.kind === 'auto'
          ? Math.max(minimum, content)
          : minimumTrackSize(track.max, availableForTracks);
        bases.push(Math.max(minimum, Math.min(maximum, Math.max(minimum, content))));
        fractionWeights.push(0);
      }
    }
  }
  const remaining = Math.max(0, availableForTracks - bases.reduce((total, size) => total + size, 0));
  const totalWeight = fractionWeights.reduce((total, weight) => total + weight, 0);
  return bases.map((base, index) => base + (totalWeight > 0 ? remaining * fractionWeights[index] / totalWeight : 0));
}
