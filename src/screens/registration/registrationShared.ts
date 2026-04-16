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

export type FeedbackMessage = { tone: "error" | "success" | "info"; text: string };

export type RegistrationFlow = "matrix_sdk" | "external_link" | "info_only";

export type HomeserverDirectory = { public_servers: HomeserverDirectoryEntry[] };

export type HomeserverDirectoryEntry = {
  server_id: string;
  homeserver_url?: string | null;
  registration_flow: RegistrationFlow;
  supports_display_name: boolean;
  is_official?: boolean;
  name: string;
  client_domain?: string | null;
  homepage?: string | null;
  isp?: string | null;
  staff_jur?: string | null;
  rules?: string | null;
  privacy?: string | null;
  description?: string | null;
  reg_method?: string | null;
  reg_link?: string | null;
  reg_note?: string | null;
  software?: string | null;
  version?: string | null;
  captcha?: boolean | null;
  captcha_note?: string | null;
  email?: boolean | null;
  longstanding?: boolean | null;
  languages: string[];
  features: string[];
  server_domain?: string | null;
  sliding_sync?: boolean | null;
  ipv6?: boolean | null;
  cloudflare?: boolean | null;
};

const officialMatrixHomeserver: HomeserverDirectoryEntry = {
  server_id: "matrix.org",
  homeserver_url: "https://matrix.org",
  registration_flow: "matrix_sdk",
  supports_display_name: true,
  is_official: true,
  name: "matrix.org",
  client_domain: "matrix.org",
  homepage: "https://matrix.org",
  description: "The official Matrix homeserver and the default registration target.",
  software: "Synapse",
  longstanding: true,
  languages: [],
  features: [],
};

export const registrationFlowOrder: Record<RegistrationFlow, number> = {
  matrix_sdk: 0,
  external_link: 1,
  info_only: 2,
};

export function getErrorMessage(error: unknown): string {
  if (typeof error === "string" && error.trim()) return error;
  if (error instanceof Error && error.message.trim()) return error.message;
  return "Something went wrong while contacting the registration service.";
}

export function homeserverTitle(homeserver: HomeserverDirectoryEntry): string {
  return homeserver.name.trim() || homeserver.server_id;
}

export function homeserverHost(homeserver: HomeserverDirectoryEntry): string {
  return (
    homeserver.client_domain ??
    homeserver.server_domain ??
    homeserver.homeserver_url ??
    homeserver.server_id
  );
}

export function homeserverCopy(homeserver: HomeserverDirectoryEntry): string {
  return (
    homeserver.description?.trim() ??
    `Create a Matrix account on ${homeserverHost(homeserver)}.`
  );
}

export function flowLabel(flow: RegistrationFlow): string {
  if (flow === "matrix_sdk") return "Vanilla registration";
  if (flow === "external_link") return "External registration";
  return "Manual guidance";
}

export function boolLabel(value?: boolean | null, trueLabel = "Available"): string {
  if (value === true) return trueLabel;
  if (value === false) return trueLabel === "Required" ? "Not required" : "Unavailable";
  return "Unknown";
}

export function safeLink(value?: string | null): string | null {
  if (!value?.trim()) return null;
  return value.includes("://") ? value : `https://${value}`;
}

export function formatWebviewUrl(url: string): string {
  try {
    const parsedUrl = new URL(url);
    return `${parsedUrl.host}${parsedUrl.pathname === "/" ? "" : parsedUrl.pathname}`;
  } catch {
    return url;
  }
}

export function normalizeHomeservers(
  homeservers: HomeserverDirectoryEntry[],
): HomeserverDirectoryEntry[] {
  const matrixOrgIndex = homeservers.findIndex(
    (homeserver) =>
      homeserver.server_id === "matrix.org" ||
      homeserver.client_domain === "matrix.org" ||
      homeserver.server_domain === "matrix.org" ||
      homeserver.homeserver_url === "https://matrix.org",
  );

  if (matrixOrgIndex === -1) {
    return [officialMatrixHomeserver, ...homeservers];
  }

  return homeservers.map((homeserver, index) =>
    index === matrixOrgIndex
      ? {
          ...officialMatrixHomeserver,
          ...homeserver,
          server_id: "matrix.org",
          homeserver_url: homeserver.homeserver_url ?? officialMatrixHomeserver.homeserver_url,
          client_domain: homeserver.client_domain ?? "matrix.org",
          is_official: true,
        }
      : homeserver,
  );
}

export function shouldSkipDetails(homeserver: HomeserverDirectoryEntry): boolean {
  return homeserver.is_official === true && homeserver.registration_flow === "matrix_sdk";
}

export function captchaWarning(homeserver: HomeserverDirectoryEntry): string {
  if (homeserver.captcha !== true) return "";

  if (homeserver.captcha_note?.trim()) {
    return `This homeserver reports a required captcha during registration. ${homeserver.captcha_note.trim()}`;
  }

  return "This homeserver reports a required captcha during registration. Hyperion may need to forward you to the homeserver's own page if the built-in form cannot complete the flow.";
}

export function handoffWarning(
  homeserver: HomeserverDirectoryEntry,
  reason: "external_flow" | "interactive_fallback",
): string {
  if (reason === "interactive_fallback") {
    return `Direct registration on ${homeserverTitle(homeserver)} could not be completed inside Hyperion. The homeserver requires additional interactive steps, so Hyperion is showing the server's own registration page instead.`;
  }

  return `${homeserverTitle(homeserver)} uses its own registration flow. Hyperion is opening the homeserver's published registration page inside the app.`;
}
