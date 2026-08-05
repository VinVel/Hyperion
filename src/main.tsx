/*
 * Copyright (c) 2026 VinVel
 *
 * SPDX-License-Identifier: AGPL-3.0-only
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, version 3 only.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 *
 * Project home: hyperion.velcore.net
 */

import React from "react";
import ReactDOM from "react-dom/client";
import type { ErrorInfo } from "react";
import App from "./App";
import {
  AppErrorBoundary,
  AppWindowFrame,
  ToastProvider,
} from "./components/ui";
import { ThemeProvider } from "./components/context";
import {
  error as traceError,
  initializeTracing,
  tracingComponents,
} from "./utils/tracing";

function reportUiError(error: unknown, _errorInfo: ErrorInfo) {
  traceError("ui.render_failed", {
    component: tracingComponents.ui,
    operation: "render",
    errorCode: "ui.render_failed",
    errorCategory: "ui",
    diagnosticDetails: error,
  });
}

function reportThemeStorageError(error: unknown) {
  traceError("theme.storage_load_failed", {
    component: tracingComponents.theme,
    operation: "load_preferences",
    errorCode: "settings.theme_load_failed",
    errorCategory: "settings",
    diagnosticDetails: error,
  });
}

void initializeTracing().finally(() => {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <ThemeProvider onStorageError={reportThemeStorageError}>
        <ToastProvider>
          <AppWindowFrame
            iconSrc="/Hyperion-icon.svg"
            titlebarLabel="Hyperion window controls"
          >
            <AppErrorBoundary onError={reportUiError}>
              <App />
            </AppErrorBoundary>
          </AppWindowFrame>
        </ToastProvider>
      </ThemeProvider>
    </React.StrictMode>,
  );
});
