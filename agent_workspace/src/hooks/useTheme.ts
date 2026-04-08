import { useState, useEffect } from 'react';

export type Theme = 'light' | 'dark' | 'system';

// 获取系统主题
const getSystemTheme = (): 'light' | 'dark' => {
  if (typeof window === 'undefined') return 'light';
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
};

// 计算实际主题
const getActualTheme = (theme: Theme): 'light' | 'dark' => {
  return theme === 'system' ? getSystemTheme() : theme;
};

// 应用主题到DOM
const applyTheme = (actualTheme: 'light' | 'dark') => {
  if (typeof window === 'undefined') return;
  
  const root = window.document.documentElement;
  root.classList.remove('light', 'dark');
  root.classList.add(actualTheme);
  
  // 更新meta标签
  const metaThemeColor = document.querySelector('meta[name="theme-color"]');
  if (metaThemeColor) {
    metaThemeColor.setAttribute('content', actualTheme === 'dark' ? '#0f172a' : '#ffffff');
  }
};

// 从localStorage获取保存的主题
const getSavedTheme = (): Theme => {
  if (typeof window === 'undefined') return 'system';
  const saved = localStorage.getItem('theme');
  return (saved as Theme) || 'system';
};

// 保存主题到localStorage
const saveTheme = (theme: Theme) => {
  if (typeof window === 'undefined') return;
  localStorage.setItem('theme', theme);
};

export const useTheme = () => {
  const [theme, setThemeState] = useState<Theme>(() => getSavedTheme());
  const [actualTheme, setActualTheme] = useState<'light' | 'dark'>(() => getActualTheme(getSavedTheme()));

  // 设置主题
  const setTheme = (newTheme: Theme) => {
    setThemeState(newTheme);
    saveTheme(newTheme);
    const newActualTheme = getActualTheme(newTheme);
    setActualTheme(newActualTheme);
    applyTheme(newActualTheme);
  };

  // 切换主题
  const toggleTheme = () => {
    let newTheme: Theme;
    
    switch (theme) {
      case 'light':
        newTheme = 'dark';
        break;
      case 'dark':
        newTheme = 'system';
        break;
      case 'system':
        newTheme = 'light';
        break;
      default:
        newTheme = 'system';
    }
    
    setTheme(newTheme);
  };

  // 监听系统主题变化
  useEffect(() => {
    if (theme !== 'system') return;
    
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    const handleChange = () => {
      const newActualTheme = getSystemTheme();
      setActualTheme(newActualTheme);
      applyTheme(newActualTheme);
    };
    
    mediaQuery.addEventListener('change', handleChange);
    return () => mediaQuery.removeEventListener('change', handleChange);
  }, [theme]);

  // 初始化时应用主题
  useEffect(() => {
    applyTheme(actualTheme);
  }, [actualTheme]);

  return {
    theme,
    actualTheme,
    setTheme,
    toggleTheme,
  };
};