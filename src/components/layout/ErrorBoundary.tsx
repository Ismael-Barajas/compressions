import { Component, type ReactNode } from "react";

interface Props {
  children: ReactNode;
}

interface State {
  hasError: boolean;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { hasError: false };

  static getDerivedStateFromError(): State {
    return { hasError: true };
  }

  componentDidCatch(error: Error) {
    console.error("Unhandled error:", error);
  }

  render() {
    if (this.state.hasError) {
      return (
        <div className="flex flex-1 flex-col items-center justify-center gap-4 p-8">
          <p className="text-lg font-medium" style={{ color: "var(--text-primary)" }}>
            Something went wrong
          </p>
          <button
            onClick={() => window.location.reload()}
            className="rounded-lg px-4 py-2 text-sm font-medium text-white"
            style={{ backgroundColor: "var(--accent)" }}
          >
            Reload
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
