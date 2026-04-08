import React, { useCallback, useEffect } from 'react';

interface UseInlineActionMenusResult {
  closeActionMenus: (exceptMenu?: HTMLElement | null) => void;
  toggleActionMenu: (event: React.MouseEvent<HTMLButtonElement>) => void;
}

export const useInlineActionMenus = (): UseInlineActionMenusResult => {
  const closeActionMenus = useCallback((exceptMenu?: HTMLElement | null) => {
    if (typeof document === 'undefined') {
      return;
    }
    const menus = document.querySelectorAll<HTMLElement>('.js-inline-action-menu');
    menus.forEach((menu) => {
      if (exceptMenu && menu === exceptMenu) {
        return;
      }
      menu.classList.add('hidden');
    });
  }, []);

  const toggleActionMenu = useCallback((event: React.MouseEvent<HTMLButtonElement>) => {
    event.stopPropagation();
    const menu = event.currentTarget.nextElementSibling as HTMLElement | null;
    if (!menu) {
      return;
    }
    const shouldOpen = menu.classList.contains('hidden');
    closeActionMenus(menu);
    if (shouldOpen) {
      menu.classList.remove('hidden');
    } else {
      menu.classList.add('hidden');
    }
  }, [closeActionMenus]);

  useEffect(() => {
    if (typeof document === 'undefined') {
      return;
    }

    const handlePointerDown = (event: MouseEvent | TouchEvent) => {
      const target = event.target as HTMLElement | null;
      if (!target) {
        return;
      }
      if (target.closest('[data-action-menu-root="true"]')) {
        return;
      }
      closeActionMenus();
    };

    const handleEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        closeActionMenus();
      }
    };

    document.addEventListener('mousedown', handlePointerDown);
    document.addEventListener('touchstart', handlePointerDown);
    document.addEventListener('keydown', handleEscape);
    return () => {
      document.removeEventListener('mousedown', handlePointerDown);
      document.removeEventListener('touchstart', handlePointerDown);
      document.removeEventListener('keydown', handleEscape);
    };
  }, [closeActionMenus]);

  return {
    closeActionMenus,
    toggleActionMenu,
  };
};
