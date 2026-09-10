import type { WebDesignLibraryName } from '../../src/schema';
import { uiLibraryByName } from '../../src/ui-libraries';
import { editableSlotsForLibraryContract } from '../../src/library-slots';
import { variantsForUiComponent } from '../../src/ui-library';
import { createSceneNodeBase, type SceneLibraryInstanceNode, type SceneShapeNode } from '../../src/v2/scene-schema';
import type { LibraryPreviewSelection } from '../library-runtime/element-selection';

function absoluteNode<T extends SceneLibraryInstanceNode | SceneShapeNode>(node: T): T {
  node.layout = { ...node.layout, position: 'absolute' };
  return node;
}

export function createSceneLibraryInstance(input: {
  nodeId: string;
  libraryName: WebDesignLibraryName;
  definitionId: string;
  variantId?: string;
  x: number;
  y: number;
  registryElement?: LibraryPreviewSelection;
}): SceneLibraryInstanceNode {
  const catalog = uiLibraryByName(input.libraryName);
  if (!catalog) throw new Error(`UI library not found: ${input.libraryName}`);
  const definition = catalog.components.find((candidate) => candidate.id === input.definitionId);
  if (!definition) throw new Error(`${catalog.displayName} component not found: ${input.definitionId}`);
  const variants = variantsForUiComponent(catalog, definition.id);
  const variant = variants.find((candidate) => candidate.id === input.variantId) ?? variants[0];
  const selection = input.registryElement;
  const width = selection?.width ?? variant.width ?? definition.width;
  const height = selection?.height ?? variant.height ?? definition.height;
  const componentSlug = String(definition.props?.componentSlug
    ?? definition.id.replace(/([a-z0-9])([A-Z])/g, '$1-$2').replace(/([A-Z])([A-Z][a-z])/g, '$1-$2').toLowerCase());
  const base = createSceneNodeBase(
    'library-instance',
    selection ? `${catalog.displayName} · ${selection.label}` : `${catalog.displayName} · ${definition.label}`,
    { x: input.x, y: input.y, width: Math.max(6, Math.round(width)), height: Math.max(6, Math.round(height)) },
    'human'
  );
  const node: SceneLibraryInstanceNode = {
    ...base,
    id: input.nodeId,
    type: 'library-instance',
    library: catalog.id,
    component: definition.id,
    variant: variant.id,
    properties: {
      ...(definition.props ?? {}),
      ...variant.props,
      componentSlug,
      ...(selection ? { registryElement: { ...selection } } : {})
    },
    content: selection?.label ?? variant.content ?? definition.content,
    slots: {}
  };
  node.slots = Object.fromEntries(editableSlotsForLibraryContract({
    width: node.frame.width,
    height: node.frame.height,
    library: { name: node.library, component: node.component, variant: node.variant, props: node.properties }
  }).map((slot) => [slot.id, []]));
  return absoluteNode(node);
}

export function createSceneBasicShape(input: {
  nodeId: string;
  shape: 'rectangle' | 'ellipse' | 'line';
  x: number;
  y: number;
}): SceneShapeNode {
  const frame = input.shape === 'line'
    ? { x: input.x, y: input.y, width: 320, height: 1 }
    : { x: input.x, y: input.y, width: input.shape === 'ellipse' ? 160 : 260, height: 160 };
  const base = createSceneNodeBase('shape', input.shape === 'ellipse' ? '圆形' : input.shape === 'line' ? '直线' : '矩形', frame, 'human');
  const node: SceneShapeNode = {
    ...base,
    id: input.nodeId,
    type: 'shape',
    shape: input.shape,
    appearance: {
      ...base.appearance,
      fills: input.shape === 'line' ? [] : [{ type: 'solid', visible: true, opacity: 1, color: '#EAF3FF' }],
      strokes: [{
        paint: { type: 'solid', visible: true, opacity: 1, color: input.shape === 'line' ? '#8E8E93' : '#A8CCFF' },
        width: { top: 1, right: 1, bottom: 1, left: 1 },
        style: 'solid'
      }],
      radius: input.shape === 'ellipse'
        ? { topLeft: 999, topRight: 999, bottomRight: 999, bottomLeft: 999 }
        : input.shape === 'rectangle'
          ? { topLeft: 16, topRight: 16, bottomRight: 16, bottomLeft: 16 }
          : base.appearance.radius
    }
  };
  return absoluteNode(node);
}
