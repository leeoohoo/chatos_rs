export interface LibraryRuntimeBoundaryInput {
  component: unknown;
  preview: boolean;
  showcase: boolean;
  tokens?: unknown;
  pickItems: boolean;
  slotContent: Record<string, unknown>;
}

export function sameLibraryRuntimeBoundary(left: LibraryRuntimeBoundaryInput, right: LibraryRuntimeBoundaryInput): boolean {
  if (left.component !== right.component || left.preview !== right.preview || left.showcase !== right.showcase
    || left.tokens !== right.tokens || left.pickItems !== right.pickItems) return false;
  const leftSlots = Object.keys(left.slotContent);
  const rightSlots = Object.keys(right.slotContent);
  if (leftSlots.length !== rightSlots.length || leftSlots.some((slot) => !rightSlots.includes(slot))) return false;
  if (leftSlots.length === 0) return true;
  return leftSlots.every((slot) => left.slotContent[slot] === right.slotContent[slot]);
}
