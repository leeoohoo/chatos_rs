import type { ToolFamily } from '../../../lib/tools/catalog';
import type { UiLocale } from '../../../i18n/messages';
import {
  getToolFamilyDescription as getLocalizedToolFamilyDescription,
  getToolFamilyLabel as getLocalizedToolFamilyLabel,
} from '../../../i18n/toolText';

export const triStateLabel = (value: boolean | null): string => (
  value === null ? 'unknown' : (value ? 'yes' : 'no')
);

export const isMeaningfulBrowserPageUrl = (url: string): boolean => {
  const normalized = url.trim().toLowerCase();
  if (!normalized) {
    return false;
  }

  return ![
    'about:blank',
    'about:srcdoc',
    'about:newtab',
    'data:,',
    'chrome://newtab/',
    'chrome://new-tab-page/',
    'edge://newtab/',
  ].includes(normalized);
};

export const getToolFamilyLabel = (family: ToolFamily, locale: UiLocale): string => (
  getLocalizedToolFamilyLabel(family, locale)
);

export const getToolFamilyDescription = (
  family: ToolFamily,
  _displayName: string,
  locale: UiLocale,
): string => getLocalizedToolFamilyDescription(family, locale);
