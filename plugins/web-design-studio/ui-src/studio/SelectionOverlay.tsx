import type { PointerEvent as ReactPointerEvent } from 'react';
import type { SceneResizeHandle } from '../../src/v2/scene-editor-transaction';
import { selectionBounds, type EditorSelectionRect } from './selection-model';

export interface SelectionOverlayItem {
  id: string;
  name: string;
  locked: boolean;
  primary: boolean;
  rect: EditorSelectionRect;
}

export function SelectionOverlay({ items, marqueeRect, resizeHandles = ['south-east'], onResizePointerDown }: {
  items: readonly SelectionOverlayItem[];
  marqueeRect?: EditorSelectionRect;
  resizeHandles?: readonly SceneResizeHandle[];
  onResizePointerDown: (componentId: string, handle: SceneResizeHandle, event: ReactPointerEvent<HTMLSpanElement>) => void;
}) {
  if (items.length === 0 && !marqueeRect) return null;
  const aggregate = items.length > 1 ? selectionBounds(items.map((item) => item.rect)) : undefined;
  return <div className="selection-overlay-layer">
    {items.map((item) => <div key={item.id} className={`selection-overlay-box ${item.primary ? 'primary' : ''} ${item.locked ? 'locked' : ''}`} style={{
      left: item.rect.x,
      top: item.rect.y,
      width: item.rect.width,
      height: item.rect.height
    }}>
      {!aggregate && item.primary && <span className="selection-overlay-label">{item.locked ? '🔒 ' : ''}{item.name}</span>}
      {!aggregate && item.primary && !item.locked && resizeHandles.map((handle) => <span
        key={handle}
        className={`selection-overlay-resize handle-${handle}`}
        onPointerDown={(event) => onResizePointerDown(item.id, handle, event)}
      />)}
    </div>)}
    {aggregate && <div className="selection-overlay-aggregate" style={{ left: aggregate.x, top: aggregate.y, width: aggregate.width, height: aggregate.height }}>
      <span className="selection-overlay-label">{items.length} 个图层</span>
    </div>}
    {marqueeRect && <div className="selection-marquee" style={{ left: marqueeRect.x, top: marqueeRect.y, width: marqueeRect.width, height: marqueeRect.height }} />}
  </div>;
}
