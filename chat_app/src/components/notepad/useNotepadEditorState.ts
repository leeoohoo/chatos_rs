// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useState } from 'react';

export const useNotepadEditorState = () => {
  const [selectedNoteId, setSelectedNoteId] = useState('');
  const [title, setTitle] = useState('');
  const [tagsText, setTagsText] = useState('');
  const [content, setContent] = useState('');
  const [dirty, setDirty] = useState(false);

  const resetEditor = useCallback(() => {
    setSelectedNoteId('');
    setTitle('');
    setTagsText('');
    setContent('');
    setDirty(false);
  }, []);

  const handleTitleChange = useCallback((value: string) => {
    setTitle(value);
    setDirty(true);
  }, []);

  const handleTagsTextChange = useCallback((value: string) => {
    setTagsText(value);
    setDirty(true);
  }, []);

  const handleContentChange = useCallback((value: string) => {
    setContent(value);
    setDirty(true);
  }, []);

  return {
    selectedNoteId,
    setSelectedNoteId,
    title,
    setTitle,
    tagsText,
    setTagsText,
    content,
    setContent,
    dirty,
    setDirty,
    resetEditor,
    handleTitleChange,
    handleTagsTextChange,
    handleContentChange,
  };
};
