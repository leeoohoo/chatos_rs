import { Component, ReactNode, type ErrorInfo } from 'react';

import { UI_MESSAGES } from '../i18n/messages';

interface Props {
  children: ReactNode;
  fallback?: ReactNode;
}

interface State {
  hasError: boolean;
  error?: Error;
  locale: 'zh-CN' | 'en-US';
}

export class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = {
      hasError: false,
      locale: typeof document !== 'undefined' && document.documentElement.lang === 'en-US'
        ? 'en-US'
        : 'zh-CN',
    };
  }

  static getDerivedStateFromError(error: Error): State {
    return {
      hasError: true,
      error,
      locale: typeof document !== 'undefined' && document.documentElement.lang === 'en-US'
        ? 'en-US'
        : 'zh-CN',
    };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error('ErrorBoundary caught an error:', error, errorInfo);
  }

  render() {
    if (this.state.hasError) {
      if (this.props.fallback) {
        return this.props.fallback;
      }

      const t = (key: string) => UI_MESSAGES[this.state.locale][key] || UI_MESSAGES['zh-CN'][key] || key;

      return (
        <div className="flex items-center justify-center h-full p-8">
          <div className="text-center space-y-4">
            <div className="w-16 h-16 mx-auto bg-destructive/10 rounded-full flex items-center justify-center">
              <svg className="w-8 h-8 text-destructive" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L3.732 16.5c-.77.833.192 2.5 1.732 2.5z" />
              </svg>
            </div>
            <div>
              <h3 className="text-lg font-semibold text-foreground">{t('errorBoundary.title')}</h3>
              <p className="text-sm text-muted-foreground mt-1">
                {this.state.error?.message || t('errorBoundary.fallback')}
              </p>
              <button
                onClick={() => this.setState({ hasError: false, error: undefined })}
                className="mt-4 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors"
              >
                {t('common.retry')}
              </button>
            </div>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}
