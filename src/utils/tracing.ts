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

import {
  LogLevel,
  emit,
  enabledLevels,
  type EnabledLevels,
  type StructuredEvent,
  type StructuredFields,
} from "@fltsci/tauri-plugin-tracing";

export const tracingComponents = {
  application: "application",
  pagination: "pagination",
  theme: "theme",
  timeline: "timeline",
  ui: "ui",
} as const;

export type TracingComponent =
  (typeof tracingComponents)[keyof typeof tracingComponents];

export type TracingContext = StructuredFields & {
  component: TracingComponent | (string & {});
  operation?: string;
  message?: string;
  diagnosticDetails?: unknown;
};

type LazyTracingContext = TracingContext | (() => TracingContext);
type LevelName = keyof EnabledLevels;

const disabledLevels: EnabledLevels = {
  trace: false,
  debug: false,
  info: false,
  warn: false,
  error: false,
};

let effectiveLevels = disabledLevels;
let initialization: Promise<void> | null = null;
const previousDiagnosticEvents = new Map<string, string>();

/** Initializes and caches the fixed frontend target's effective levels. */
export function initializeTracing(): Promise<void> {
  if (initialization) {
    return initialization;
  }

  initialization = enabledLevels()
    .then((levels) => {
      effectiveLevels = levels;
    })
    .catch(() => {
      effectiveLevels = disabledLevels;
    });
  return initialization;
}

export function trace(eventName: string, context: LazyTracingContext): void {
  log("trace", LogLevel.Trace, eventName, context);
}

export function debug(eventName: string, context: LazyTracingContext): void {
  log("debug", LogLevel.Debug, eventName, context);
}

export function info(eventName: string, context: TracingContext): void {
  log("info", LogLevel.Info, eventName, context);
}

export function warn(eventName: string, context: TracingContext): void {
  log("warn", LogLevel.Warn, eventName, context);
}

export function error(eventName: string, context: TracingContext): void {
  log("error", LogLevel.Error, eventName, context);
}

/** Reads the cached state without issuing IPC. */
export function isTracingLevelEnabled(level: LevelName): boolean {
  return effectiveLevels[level];
}

function log(
  levelName: LevelName,
  level: LogLevel,
  eventName: string,
  contextOrFactory: LazyTracingContext,
): void {
  if (!effectiveLevels[levelName]) {
    return;
  }

  const context =
    typeof contextOrFactory === "function"
      ? contextOrFactory()
      : contextOrFactory;
  const event = structuredEvent(level, eventName, context);
  if (
    (level === LogLevel.Trace || level === LogLevel.Debug) &&
    !diagnosticEventChanged(event)
  ) {
    return;
  }

  void emit(event).catch(() => undefined);
}

function diagnosticEventChanged(event: StructuredEvent): boolean {
  const diagnosticKey = [
    event.level,
    event.component,
    event.eventName,
    event.operation ?? "",
    event.fields?.accountId ?? "",
    event.fields?.roomId ?? "",
  ].join(":");
  const serializedEvent = JSON.stringify(event);
  if (previousDiagnosticEvents.get(diagnosticKey) === serializedEvent) {
    return false;
  }

  previousDiagnosticEvents.set(diagnosticKey, serializedEvent);
  return true;
}

function structuredEvent(
  level: LogLevel,
  eventName: string,
  context: TracingContext,
): StructuredEvent {
  const fields: StructuredFields = {
    errorCode: context.errorCode,
    errorCategory: context.errorCategory,
    outcome: context.outcome,
    durationMs: context.durationMs,
    itemCount: context.itemCount,
  };

  if (import.meta.env.DEV) {
    fields.accountId = context.accountId;
    fields.roomId = context.roomId;
    fields.matrixEventId = context.matrixEventId;
  }

  const event: StructuredEvent = {
    level,
    eventName,
    component: context.component,
    operation: context.operation,
    fields,
  };

  if (import.meta.env.DEV) {
    event.message = context.message;
    event.diagnosticDetails = normalizeDiagnosticDetails(
      context.diagnosticDetails,
    );
  }

  return event;
}

function normalizeDiagnosticDetails(value: unknown): unknown {
  if (value instanceof Error) {
    return {
      name: value.name,
      message: value.message,
      stack: value.stack,
    };
  }

  if (value === undefined || value === null) {
    return value;
  }

  try {
    return JSON.parse(JSON.stringify(value)) as unknown;
  } catch {
    return String(value);
  }
}

export const tracingTesting = {
  reset() {
    effectiveLevels = disabledLevels;
    initialization = null;
    previousDiagnosticEvents.clear();
  },
};
