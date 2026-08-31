import { Button, Result } from 'antd';
import { Component, type ErrorInfo, type ReactNode } from 'react';

type ModuleErrorBoundaryProps = {
  children: ReactNode;
  resetKey: string;
  title: string;
  description: string;
  retryLabel: string;
};

type ModuleErrorBoundaryState = {
  failed: boolean;
};

export class ModuleErrorBoundary extends Component<ModuleErrorBoundaryProps, ModuleErrorBoundaryState> {
  state: ModuleErrorBoundaryState = { failed: false };

  static getDerivedStateFromError(): ModuleErrorBoundaryState {
    return { failed: true };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('Administration module render failed', error, info.componentStack);
  }

  componentDidUpdate(previousProps: ModuleErrorBoundaryProps) {
    if (this.state.failed && previousProps.resetKey !== this.props.resetKey) {
      this.setState({ failed: false });
    }
  }

  render() {
    if (!this.state.failed) return this.props.children;
    return (
      <Result
        status="error"
        title={this.props.title}
        subTitle={this.props.description}
        extra={<Button type="primary" onClick={() => this.setState({ failed: false })}>{this.props.retryLabel}</Button>}
      />
    );
  }
}
