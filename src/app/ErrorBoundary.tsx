import { Component, type ErrorInfo, type ReactNode } from "react";

export class ErrorBoundary extends Component<{ children: ReactNode }, { error: string | null }> {
  state = { error: null as string | null };

  static getDerivedStateFromError(error: Error) { return { error: error.message }; }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("Workspace render failure", { message: error.message, componentStack: info.componentStack });
  }

  render() {
    if (this.state.error) return <main className="fatal-error"><h1>Workspace could not render</h1><p>{this.state.error}</p><button onClick={() => location.reload()}>Reload</button></main>;
    return this.props.children;
  }
}
