import React from "react";
import ReactDOM from "react-dom/client";
import "@fontsource-variable/inter";
import { App } from "./app/App";
import { ErrorBoundary } from "./app/ErrorBoundary";
import "./styles/index.css";

// Dev-only: expose stores on window for e2e probing (no effect in prod build).
if (import.meta.env.DEV) {
  void (async () => {
    const [{ useChatStore }, { useRouterStore }] = await Promise.all([
      import("./stores/chatStore"),
      import("./stores/routerStore"),
    ]);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (window as any).__stores = { chat: useChatStore, router: useRouterStore };
  })();
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </React.StrictMode>,
);
