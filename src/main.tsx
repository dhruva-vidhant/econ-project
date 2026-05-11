import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { BrowserRouter } from "react-router-dom";

import App from "@/App";
import "@/styles/index.css";

// E2E mode: replace the Tauri IPC with an in-memory mock so Playwright
// can drive the UI without a real Tauri runtime. Activated via VITE_E2E=1.
if (import.meta.env.VITE_E2E === "1") {
  // Dynamic import keeps the mock out of the production bundle.
  await import("@/test-mock-tauri");
}

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 5 * 60 * 1000,
      refetchOnWindowFocus: false,
      retry: 1,
    },
  },
});

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <App />
      </BrowserRouter>
    </QueryClientProvider>
  </React.StrictMode>,
);
