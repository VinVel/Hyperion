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

import { debug, tracingComponents } from "../../utils/tracing";

export type PaginationDiagnosticPayload = Record<
  string,
  boolean | number | string | null | undefined
>;

export function logPaginationDiagnostic(
  label: string,
  payload: PaginationDiagnosticPayload,
): void {
  debug(label, () => ({
    component: tracingComponents.pagination,
    operation: label,
    accountId:
      typeof payload.accountKey === "string" ? payload.accountKey : undefined,
    roomId: typeof payload.roomId === "string" ? payload.roomId : undefined,
    outcome:
      typeof payload.success === "boolean"
        ? payload.success
          ? "success"
          : "failure"
        : undefined,
    diagnosticDetails: payload,
  }));
}
