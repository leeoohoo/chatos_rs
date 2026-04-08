import { useEffect, useRef } from 'react';

export const useDismissiblePopover = <T extends HTMLElement>(
  open: boolean,
  onClose: () => void,
) => {
  const ref = useRef<T>(null);

  useEffect(() => {
    if (!open) {
      return;
    }
    const onDocClick = (event: MouseEvent) => {
      if (!ref.current) {
        return;
      }
      if (!ref.current.contains(event.target as Node)) {
        onClose();
      }
    };
    document.addEventListener('mousedown', onDocClick);
    return () => document.removeEventListener('mousedown', onDocClick);
  }, [onClose, open]);

  return ref;
};
