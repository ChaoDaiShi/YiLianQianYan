import { Component, type ErrorInfo, type ReactNode } from "react";
import { AlertCircle, RotateCcw } from "lucide-react";
import Button from "../ui/Button";

interface Props {
  children: ReactNode;
  fallback?: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export default class RouteErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error("RouteErrorBoundary caught an error:", error, errorInfo);
  }

  handleRetry = () => {
    this.setState({ hasError: false, error: null });
  };

  render() {
    if (this.state.hasError) {
      if (this.props.fallback) {
        return this.props.fallback;
      }

      const isChunkError =
        this.state.error?.message?.includes("dynamically imported module") ||
        this.state.error?.message?.includes("Failed to fetch");

      return (
        <div
          role="alert"
          aria-live="assertive"
          className="flex h-full w-full flex-col items-center justify-center p-6 text-center"
        >
          <div className="flex max-w-md flex-col items-center rounded-2xl border border-[var(--danger-border)] bg-[var(--surface-solid)] p-6 shadow-[var(--shadow-card)]">
            <div className="mb-3 flex h-12 w-12 items-center justify-center rounded-full bg-[var(--danger-soft)] text-[var(--danger)]">
              <AlertCircle className="h-6 w-6" />
            </div>
            <h2 className="mb-1 text-base font-semibold text-[var(--text-primary)]">
              {isChunkError ? "页面资源更新或加载失败" : "页面渲染遇到异常"}
            </h2>
            <p className="mb-4 text-xs text-[var(--text-secondary)] leading-relaxed">
              {isChunkError
                ? "检测到前端资源已被更新，或者网络加载中断，请尝试刷新重新载入最新版本。"
                : this.state.error?.message || "发生了未知渲染错误，你可以尝试重试或返回首页。"}
            </p>
            <div className="flex items-center gap-2">
              <Button
                variant="primary"
                size="sm"
                onClick={this.handleRetry}
                className="flex items-center gap-1.5"
              >
                <RotateCcw className="h-3.5 w-3.5" />
                <span>重试</span>
              </Button>
              <Button
                variant="secondary"
                size="sm"
                onClick={() => window.location.reload()}
              >
                刷新页面
              </Button>
            </div>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}
