import { Component, type ReactNode } from "react";

interface Props {
  children: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { hasError: false, error: null };

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error) {
    console.error("Unhandled error:", error);
  }

  render() {
    if (this.state.hasError) {
      return (
        <div className="flex flex-1 flex-col items-center justify-center gap-6 p-8">
          <div
            className="flex h-16 w-16 items-center justify-center"
            style={{ border: "1.5px solid var(--error)" }}
          >
            <span className="text-2xl font-bold" style={{ color: "var(--error)" }}>!</span>
          </div>
          <p className="text-base font-semibold" style={{ color: "var(--text-primary)" }}>
            Something went wrong
          </p>
          {this.state.error?.message && (
            <p
              className="max-w-md break-words text-center font-mono text-xs"
              style={{ color: "var(--text-muted)" }}
            >
              {this.state.error.message}
            </p>
          )}
          <button
            onClick={() => window.location.reload()}
            className="btn-primary"
          >
            Reload
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
