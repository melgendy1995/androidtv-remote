import React, { Component, type ReactNode } from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./theme.css";

interface Props {
  children: ReactNode;
}

interface State {
  hasError: boolean;
  error?: Error;
}

class ErrorBoundary extends Component<Props, State> {
  public state: State = {
    hasError: false,
  };

  public static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  public componentDidCatch(error: Error, errorInfo: React.ErrorInfo) {
    console.error("Uncaught Error in App:", error, errorInfo);
  }

  public render() {
    if (this.state.hasError) {
      return (
        <div style={{ padding: 32, color: "#fff", background: "#0e0e10", height: "100vh", display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", textAlign: "center" }}>
          <h2 style={{ fontSize: "1.2rem", marginBottom: 8 }}>App Encountered a Temporary Error</h2>
          <p style={{ color: "#ff453a", fontFamily: "monospace", fontSize: "0.9rem", background: "rgba(255,69,58,0.1)", padding: 12, borderRadius: 8, maxWidth: 500, wordBreak: "break-word" }}>
            {this.state.error?.toString()}
          </p>
          <button
            style={{ marginTop: 16, padding: "10px 20px", background: "#0a84ff", color: "#fff", border: "none", borderRadius: 8, fontWeight: 600, cursor: "pointer" }}
            onClick={() => window.location.reload()}
          >
            Reload Application
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </React.StrictMode>,
);
