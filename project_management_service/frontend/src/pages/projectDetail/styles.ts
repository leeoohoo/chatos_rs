import type { CSSProperties } from 'react';

export const profileFormStyle: CSSProperties = {
  maxWidth: 1280,
};

export const profileToolbarStyle: CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  gap: 16,
  marginBottom: 16,
};

export const profileMarkdownSectionStyle: CSSProperties = {
  marginBottom: 18,
  background: '#fff',
  border: '1px solid #eceff3',
  borderRadius: 8,
  overflow: 'hidden',
};

export const profileMarkdownSectionHeaderStyle: CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  gap: 12,
  padding: '14px 18px',
  borderBottom: '1px solid #eef0f3',
};

export const profileEditorLayoutStyle: CSSProperties = {
  padding: 18,
};

export const profileTextAreaStyle: CSSProperties = {
  minHeight: 520,
  fontFamily:
    'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace',
  lineHeight: 1.7,
  resize: 'vertical',
};

export const profilePreviewOnlyStyle: CSSProperties = {
  minHeight: 220,
  maxHeight: 680,
  overflow: 'auto',
  background: '#fff',
};

export const profileEmptyPreviewStyle: CSSProperties = {
  minHeight: 120,
  background: '#fff',
};

export const detailPreviewShellStyle: CSSProperties = {
  minHeight: '100%',
};

export const detailPreviewHeaderStyle: CSSProperties = {
  padding: '24px 32px 18px',
  background: '#fff',
  borderBottom: '1px solid #eef0f3',
};

export const detailPreviewTitleStyle: CSSProperties = {
  margin: 0,
  lineHeight: 1.35,
  letterSpacing: 0,
};

export const detailPreviewMetaStyle: CSSProperties = {
  padding: '16px 32px 0',
  background: '#f6f7f9',
};

export const markdownSectionsStyle: CSSProperties = {
  display: 'grid',
  gap: 16,
  padding: '16px 32px 32px',
};

export const technicalOverviewModalBodyStyle: CSSProperties = {
  maxHeight: 'calc(100vh - 180px)',
  overflowY: 'auto',
  paddingTop: 20,
};

export const technicalDocumentsLayoutStyle: CSSProperties = {
  display: 'grid',
  gridTemplateColumns: 'minmax(240px, 320px) minmax(0, 1fr)',
  gap: 16,
  minHeight: 620,
};

export const technicalDocumentsListPaneStyle: CSSProperties = {
  border: '1px solid #e5e7eb',
  borderRadius: 8,
  background: '#fff',
  overflow: 'hidden',
};

export const technicalDocumentsListHeaderStyle: CSSProperties = {
  padding: '11px 16px',
  borderBottom: '1px solid #eef0f3',
  background: '#fafafa',
};

export const technicalDocumentsListBodyStyle: CSSProperties = {
  maxHeight: 590,
  overflowY: 'auto',
};

export const technicalOverviewPreviewPaneStyle: CSSProperties = {
  minHeight: 620,
  border: '1px solid #e5e7eb',
  borderRadius: 8,
  background: '#fff',
  overflow: 'hidden',
};

export const technicalOverviewPreviewHeaderStyle: CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  gap: 12,
  padding: '11px 16px',
  borderBottom: '1px solid #eef0f3',
  background: '#fafafa',
};

export const technicalOverviewPreviewBodyStyle: CSSProperties = {
  maxHeight: 560,
  overflowY: 'auto',
};
