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

import { Component, type ErrorInfo, type ReactNode } from "react";
import { Button } from "./Button";
import { Typography } from "./Typography";

type AppErrorBoundaryProps = {
  children: ReactNode;
};

type AppErrorBoundaryState = {
  errorMessage: string | null;
};

export class AppErrorBoundary extends Component<
  AppErrorBoundaryProps,
  AppErrorBoundaryState
> {
  state: AppErrorBoundaryState = {
    errorMessage: null,
  };

  static getDerivedStateFromError(error: unknown): AppErrorBoundaryState {
    return {
      errorMessage:
        error instanceof Error ? error.message : "Unexpected UI error.",
    };
  }

  componentDidCatch(error: unknown, errorInfo: ErrorInfo) {
    console.error("Hyperion UI render failure", error, errorInfo);
  }

  render() {
    if (!this.state.errorMessage) {
      return this.props.children;
    }

    return (
      <main className="ui-app-error-boundary" role="alert">
        <div className="ui-app-error-boundary__content">
          <Typography as="h1" variant="h2">
            The interface crashed
          </Typography>
          <Typography variant="body" muted>
            {this.state.errorMessage}
          </Typography>
          <Button variant="primary" onClick={() => window.location.reload()}>
            Reload
          </Button>
        </div>
      </main>
    );
  }
}
