import { Component, type ErrorInfo, type ReactNode } from "react";
import { Button } from "@/shared/ui/Button";
import { CopyIcon, RefreshIcon, WarningCircleIcon } from "@/shared/icons/set";
import { Icon } from "@/shared/icons/Icon";

interface ErrorBoundaryProps {
  children: ReactNode;
}

interface ErrorBoundaryState {
  error: Error | null;
  errorInfo: ErrorInfo | null;
}

export class ErrorBoundary extends Component<
  ErrorBoundaryProps,
  ErrorBoundaryState
> {
  state: ErrorBoundaryState = {
    error: null,
    errorInfo: null,
  };

  static getDerivedStateFromError(error: Error): Partial<ErrorBoundaryState> {
    return { error };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    this.setState({ error, errorInfo });
    // eslint-disable-next-line no-console
    console.error("[render-error]", error, errorInfo);
  }

  private copyDetails = async () => {
    const { error, errorInfo } = this.state;
    const details = [
      error?.name,
      error?.message,
      error?.stack,
      errorInfo?.componentStack,
    ]
      .filter(Boolean)
      .join("\n\n");
    await navigator.clipboard?.writeText(details);
  };

  render() {
    const { error, errorInfo } = this.state;
    if (!error) return this.props.children;

    return (
      <div className="min-h-screen bg-surface text-text-primary flex items-center justify-center p-8">
        <div className="w-full max-w-2xl rounded-lg border border-border-subtle bg-elevated p-6 shadow-sm">
          <div className="flex items-start gap-3">
            <div className="mt-0.5 rounded-md bg-danger-soft p-2 text-danger">
              <Icon icon={WarningCircleIcon} size={18} />
            </div>
            <div className="min-w-0 flex-1">
              <h1 className="text-lg font-semibold">页面渲染出错</h1>
              <p className="mt-1 text-sm text-text-secondary">
                捕获到一个前端渲染异常，错误详情如下。
              </p>
            </div>
          </div>

          <div className="mt-4 rounded-md border border-border-subtle bg-surface p-3">
            <div className="text-sm font-medium text-danger">
              {error.name}: {error.message}
            </div>
            <pre className="mt-3 max-h-72 overflow-auto whitespace-pre-wrap break-words text-xs leading-relaxed text-text-tertiary">
              {error.stack}
              {errorInfo?.componentStack}
            </pre>
          </div>

          <div className="mt-5 flex justify-end gap-2">
            <Button variant="secondary" icon={CopyIcon} onClick={this.copyDetails}>
              复制错误
            </Button>
            <Button
              variant="primary"
              icon={RefreshIcon}
              onClick={() => window.location.reload()}
            >
              重新加载
            </Button>
          </div>
        </div>
      </div>
    );
  }
}
