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

import { invoke } from "@tauri-apps/api/core";
import { Webview } from "@tauri-apps/api/webview";
import { LogicalPosition, LogicalSize, getCurrentWindow } from "@tauri-apps/api/window";
import {
  type FormEvent,
  useDeferredValue,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import "./RegistrationScreen.css";

type FeedbackMessage = { tone: "error" | "success" | "info"; text: string };
type LoginLaunchState = {
  homeserver?: string;
  text: string;
  tone: "error" | "success" | "info";
};
type RegistrationScreenProps = {
  onBackToLogin: (nextLaunchState?: LoginLaunchState | null) => void;
};
type RegistrationStage = "directory" | "details" | "form" | "webview";
type NonWebviewStage = Exclude<RegistrationStage, "webview">;
type RegistrationFlow = "matrix_sdk" | "external_link" | "info_only";
type HomeserverDirectory = { public_servers: HomeserverDirectoryEntry[] };
type HomeserverDirectoryEntry = {
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
type RegistrationOutcome =
  | {
      kind: "registered";
      account: { user_id: string };
      homeserver: HomeserverDirectoryEntry;
      note?: string | null;
    }
  | {
      kind: "external_registration_opened";
      homeserver: HomeserverDirectoryEntry;
      reg_link: string;
    }
  | {
      kind: "information_only";
      homeserver: HomeserverDirectoryEntry;
      message: string;
    };
type RegistrationFormValues = {
  username: string;
  displayName: string;
  password: string;
  email: string;
};
type EmbeddedWebviewState = {
  kind: "registration" | "link";
  returnStage: NonWebviewStage;
  title: string;
  url: string;
  warning?: string | null;
};

const DEVICE_DISPLAY_NAME = "Hyperion";
const EMBEDDED_WEBVIEW_LABEL = "registration-handoff-webview";
const DESKTOP_WEBVIEW_USER_AGENT =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/134.0.0.0 Safari/537.36 Hyperion/0.1";
const OFFICIAL_MATRIX_HOMESERVER: HomeserverDirectoryEntry = {
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
const flowOrder: Record<RegistrationFlow, number> = {
  matrix_sdk: 0,
  external_link: 1,
  info_only: 2,
};
const defaultFormValues: RegistrationFormValues = {
  username: "",
  displayName: "",
  password: "",
  email: "",
};

function getErrorMessage(error: unknown): string {
  if (typeof error === "string" && error.trim()) return error;
  if (error instanceof Error && error.message.trim()) return error.message;
  return "Something went wrong while contacting the registration service.";
}

function homeserverTitle(homeserver: HomeserverDirectoryEntry): string {
  return homeserver.name.trim() || homeserver.server_id;
}

function homeserverHost(homeserver: HomeserverDirectoryEntry): string {
  return (
    homeserver.client_domain ??
    homeserver.server_domain ??
    homeserver.homeserver_url ??
    homeserver.server_id
  );
}

function homeserverCopy(homeserver: HomeserverDirectoryEntry): string {
  return (
    homeserver.description?.trim() ??
    `Create a Matrix account on ${homeserverHost(homeserver)}.`
  );
}

function flowLabel(flow: RegistrationFlow): string {
  if (flow === "matrix_sdk") return "Vanilla registration";
  if (flow === "external_link") return "External registration";
  return "Manual guidance";
}

function boolLabel(value?: boolean | null, trueLabel = "Available"): string {
  if (value === true) return trueLabel;
  if (value === false) return trueLabel === "Required" ? "Not required" : "Unavailable";
  return "Unknown";
}

function safeLink(value?: string | null): string | null {
  if (!value?.trim()) return null;
  return value.includes("://") ? value : `https://${value}`;
}

function formatWebviewUrl(url: string): string {
  try {
    const parsedUrl = new URL(url);
    return `${parsedUrl.host}${parsedUrl.pathname === "/" ? "" : parsedUrl.pathname}`;
  } catch {
    return url;
  }
}

function normalizeHomeservers(
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
    return [OFFICIAL_MATRIX_HOMESERVER, ...homeservers];
  }

  return homeservers.map((homeserver, index) =>
    index === matrixOrgIndex
      ? {
          ...OFFICIAL_MATRIX_HOMESERVER,
          ...homeserver,
          server_id: "matrix.org",
          homeserver_url: homeserver.homeserver_url ?? OFFICIAL_MATRIX_HOMESERVER.homeserver_url,
          client_domain: homeserver.client_domain ?? "matrix.org",
          is_official: true,
        }
      : homeserver,
  );
}

function shouldSkipDetails(homeserver: HomeserverDirectoryEntry): boolean {
  return homeserver.is_official === true && homeserver.registration_flow === "matrix_sdk";
}

function captchaWarning(homeserver: HomeserverDirectoryEntry): string {
  if (homeserver.captcha !== true) return "";

  if (homeserver.captcha_note?.trim()) {
    return `This homeserver reports a required captcha during registration. ${homeserver.captcha_note.trim()}`;
  }

  return "This homeserver reports a required captcha during registration. Hyperion may need to forward you to the homeserver's own page if the built-in form cannot complete the flow.";
}

function handoffWarning(
  homeserver: HomeserverDirectoryEntry,
  reason: "external_flow" | "interactive_fallback",
): string {
  if (reason === "interactive_fallback") {
    return `Direct registration on ${homeserverTitle(homeserver)} could not be completed inside Hyperion. The homeserver requires additional interactive steps, so Hyperion is showing the server's own registration page instead.`;
  }

  return `${homeserverTitle(homeserver)} uses its own registration flow. Hyperion is opening the homeserver's published registration page inside the app.`;
}

function BackArrowGlyph() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      <path
        d="M19 12H6m5-5-5 5 5 5"
        fill="none"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="2"
      />
    </svg>
  );
}

function SearchGlyph() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      <circle cx="11" cy="11" r="6.5" fill="none" stroke="currentColor" strokeWidth="2" />
      <path
        d="m16 16 4.5 4.5"
        fill="none"
        stroke="currentColor"
        strokeLinecap="round"
        strokeWidth="2"
      />
    </svg>
  );
}

export default function RegistrationScreen({
  onBackToLogin,
}: RegistrationScreenProps) {
  const [homeservers, setHomeservers] = useState<HomeserverDirectoryEntry[]>([]);
  const [selectedServerId, setSelectedServerId] = useState<string | null>(null);
  const [stage, setStage] = useState<RegistrationStage>("directory");
  const [searchQuery, setSearchQuery] = useState("");
  const [formValues, setFormValues] = useState<RegistrationFormValues>(defaultFormValues);
  const [feedback, setFeedback] = useState<FeedbackMessage | null>(null);
  const [validationRequested, setValidationRequested] = useState(false);
  const [isLoadingHomeservers, setIsLoadingHomeservers] = useState(true);
  const [isRefreshingHomeservers, setIsRefreshingHomeservers] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [embeddedWebview, setEmbeddedWebview] = useState<EmbeddedWebviewState | null>(null);
  const webviewHostRef = useRef<HTMLDivElement | null>(null);
  const deferredSearchQuery = useDeferredValue(searchQuery.trim().toLowerCase());

  async function loadHomeservers(reason: "initial" | "refresh" = "initial") {
    reason === "refresh" ? setIsRefreshingHomeservers(true) : setIsLoadingHomeservers(true);
    try {
      const directory = await invoke<HomeserverDirectory>("list_registration_homeservers");
      const nextHomeservers = normalizeHomeservers(directory.public_servers)
        .filter((homeserver) => homeserver.server_id.trim().length > 0)
        .sort(
          (left, right) =>
            Number(right.is_official === true) - Number(left.is_official === true) ||
            flowOrder[left.registration_flow] - flowOrder[right.registration_flow] ||
            homeserverTitle(left).localeCompare(homeserverTitle(right)),
        );
      setHomeservers(nextHomeservers);
      setSelectedServerId((current) =>
        current && nextHomeservers.some((homeserver) => homeserver.server_id === current)
          ? current
          : null,
      );
    } catch (error) {
      setFeedback({ tone: "error", text: getErrorMessage(error) });
    } finally {
      setIsLoadingHomeservers(false);
      setIsRefreshingHomeservers(false);
    }
  }

  useEffect(() => {
    void loadHomeservers();
  }, []);

  useLayoutEffect(() => {
    window.scrollTo({ top: 0, left: 0, behavior: "auto" });
  }, [stage, selectedServerId]);

  useEffect(() => {
    if (stage !== "webview" || !embeddedWebview || !webviewHostRef.current) {
      return;
    }

    const appWindow = getCurrentWindow();
    let disposed = false;
    let resizeObserver: ResizeObserver | null = null;
    let removeWindowResizeListener: (() => void) | null = null;
    let removeScrollListener: (() => void) | null = null;
    let currentWebview: Webview | null = null;

    const syncBounds = async () => {
      if (disposed || !currentWebview || !webviewHostRef.current) return;

      const nextRect = webviewHostRef.current.getBoundingClientRect();
      const nextWidth = Math.max(1, Math.round(nextRect.width));
      const nextHeight = Math.max(1, Math.round(nextRect.height));

      if (nextWidth < 1 || nextHeight < 1) return;

      await Promise.allSettled([
        currentWebview.setPosition(
          new LogicalPosition(Math.round(nextRect.left), Math.round(nextRect.top)),
        ),
        currentWebview.setSize(new LogicalSize(nextWidth, nextHeight)),
      ]);
    };

    const openWebview = async () => {
      const existingWebview = await Webview.getByLabel(EMBEDDED_WEBVIEW_LABEL);
      if (existingWebview) {
        await existingWebview.close().catch(() => undefined);
      }

      if (disposed || !webviewHostRef.current) return;

      const initialRect = webviewHostRef.current.getBoundingClientRect();
      const initialWidth = Math.max(1, Math.round(initialRect.width));
      const initialHeight = Math.max(1, Math.round(initialRect.height));

      if (initialWidth < 1 || initialHeight < 1) {
        requestAnimationFrame(() => {
          if (!disposed) {
            void openWebview();
          }
        });
        return;
      }

      const nextWebview = new Webview(appWindow, EMBEDDED_WEBVIEW_LABEL, {
        url: embeddedWebview.url,
        x: Math.round(initialRect.left),
        y: Math.round(initialRect.top),
        width: initialWidth,
        height: initialHeight,
        focus: true,
        userAgent: DESKTOP_WEBVIEW_USER_AGENT,
      });

      const creationResult = new Promise<void>((resolve, reject) => {
        void nextWebview.once("tauri://created", () => resolve());
        void nextWebview.once("tauri://error", (event) => {
          reject(new Error(getErrorMessage(event.payload)));
        });
      });

      currentWebview = nextWebview;
      await creationResult;

      if (disposed || !webviewHostRef.current) return;

      const handleLayoutChange = () => {
        void syncBounds();
      };

      if (typeof ResizeObserver !== "undefined") {
        resizeObserver = new ResizeObserver(handleLayoutChange);
        resizeObserver.observe(webviewHostRef.current);
      }

      removeWindowResizeListener = await appWindow.onResized(handleLayoutChange);
      window.addEventListener("scroll", handleLayoutChange, true);
      removeScrollListener = () => window.removeEventListener("scroll", handleLayoutChange, true);

      await syncBounds();
    };

    void openWebview().catch((error) => {
      if (disposed) return;

      setEmbeddedWebview(null);
      setStage(embeddedWebview.returnStage);
      setFeedback({
        tone: "error",
        text: `Failed to open the in-app webview: ${getErrorMessage(error)}`,
      });
    });

    return () => {
      disposed = true;
      resizeObserver?.disconnect();
      removeWindowResizeListener?.();
      removeScrollListener?.();

      void (async () => {
        const existingWebview =
          currentWebview ?? (await Webview.getByLabel(EMBEDDED_WEBVIEW_LABEL));
        await existingWebview?.close().catch(() => undefined);
      })();
    };
  }, [embeddedWebview, stage]);

  const visibleHomeservers = homeservers.filter((homeserver) =>
    deferredSearchQuery.length === 0
      ? true
      : [
          homeserver.server_id,
          homeserver.name,
          homeserver.client_domain,
          homeserver.server_domain,
          homeserver.software,
          homeserver.version,
          homeserver.description,
        ]
          .filter(Boolean)
          .join(" ")
          .toLowerCase()
          .includes(deferredSearchQuery),
  );

  const selectedHomeserver =
    homeservers.find((homeserver) => homeserver.server_id === selectedServerId) ?? null;
  const usernameMissing = validationRequested && formValues.username.trim().length === 0;
  const passwordMissing = validationRequested && formValues.password.length === 0;
  const emailRequired = selectedHomeserver?.email === true;
  const emailMissing = validationRequested && emailRequired && formValues.email.trim().length === 0;

  function resetTransientState() {
    setFeedback(null);
    setValidationRequested(false);
  }

  function updateField(field: keyof RegistrationFormValues, value: string) {
    setFormValues((current) => ({ ...current, [field]: value }));
  }

  function finishInLogin(nextLaunchState?: LoginLaunchState) {
    onBackToLogin(nextLaunchState ?? null);
  }

  function openEmbeddedWebview(nextWebview: EmbeddedWebviewState) {
    resetTransientState();
    setEmbeddedWebview(nextWebview);
    setStage("webview");
  }

  function openPublishedLink(url: string, title: string) {
    openEmbeddedWebview({
      kind: "link",
      returnStage: stage === "form" ? "form" : "details",
      title,
      url,
    });
  }

  function handleBack() {
    if (stage === "directory") {
      finishInLogin();
      return;
    }

    if (stage === "webview") {
      setStage(embeddedWebview?.returnStage ?? "directory");
      setEmbeddedWebview(null);
      resetTransientState();
      return;
    }

    if (stage === "details") {
      setStage("directory");
      resetTransientState();
      return;
    }

    setStage(
      selectedHomeserver && shouldSkipDetails(selectedHomeserver) ? "directory" : "details",
    );
    resetTransientState();
  }

  function openDetails(homeserver: HomeserverDirectoryEntry) {
    setSelectedServerId(homeserver.server_id);
    setEmbeddedWebview(null);
    resetTransientState();
    setStage(shouldSkipDetails(homeserver) ? "form" : "details");
  }

  function openForm() {
    if (!selectedHomeserver || selectedHomeserver.registration_flow !== "matrix_sdk") return;
    setEmbeddedWebview(null);
    setStage("form");
    resetTransientState();
  }

  function handleOutcome(outcome: RegistrationOutcome) {
    if (outcome.kind === "registered") {
      finishInLogin({
        homeserver: outcome.homeserver.homeserver_url ?? undefined,
        text: outcome.note
          ? `Registered and signed in as ${outcome.account.user_id}. ${outcome.note}`
          : `Registered and signed in as ${outcome.account.user_id}.`,
        tone: "success",
      });
      return;
    }

    if (outcome.kind === "external_registration_opened") {
      openEmbeddedWebview({
        kind: "registration",
        returnStage: stage === "form" ? "form" : "details",
        title: `Registration for ${homeserverTitle(outcome.homeserver)}`,
        url: outcome.reg_link,
        warning: handoffWarning(
          outcome.homeserver,
          stage === "form" ? "interactive_fallback" : "external_flow",
        ),
      });
      return;
    }

    finishInLogin({
      homeserver: outcome.homeserver.homeserver_url ?? undefined,
      text: outcome.message,
      tone: "info",
    });
  }

  async function handleNonVanillaAction() {
    if (!selectedHomeserver) return;
    setIsSubmitting(true);
    setFeedback(null);
    try {
      const outcome = await invoke<RegistrationOutcome>("register_account", {
        request: {
          server_id: selectedHomeserver.server_id,
          username: "",
          password: "",
          email: null,
          display_name: null,
          device_display_name: DEVICE_DISPLAY_NAME,
        },
      });
      handleOutcome(outcome);
    } catch (error) {
      setFeedback({ tone: "error", text: getErrorMessage(error) });
    } finally {
      setIsSubmitting(false);
    }
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!selectedHomeserver) return;
    setValidationRequested(true);

    if (
      formValues.username.trim().length === 0 ||
      formValues.password.length === 0 ||
      (emailRequired && formValues.email.trim().length === 0)
    ) {
      setFeedback({
        tone: "error",
        text: emailRequired
          ? "Username, password, and email are required before you can create an account."
          : "Username and password are required before you can create an account.",
      });
      return;
    }

    setIsSubmitting(true);
    setFeedback(null);
    try {
      const outcome = await invoke<RegistrationOutcome>("register_account", {
        request: {
          server_id: selectedHomeserver.server_id,
          username: formValues.username.trim(),
          password: formValues.password,
          email: formValues.email.trim() ? formValues.email.trim() : null,
          display_name: formValues.displayName.trim() ? formValues.displayName.trim() : null,
          device_display_name: DEVICE_DISPLAY_NAME,
        },
      });
      handleOutcome(outcome);
    } catch (error) {
      setFeedback({ tone: "error", text: getErrorMessage(error) });
    } finally {
      setIsSubmitting(false);
    }
  }

  const backLabel =
    stage === "directory"
      ? "Back to log in"
      : stage === "webview"
        ? "Return to app"
        : stage === "details"
          ? "Back to homeservers"
          : selectedHomeserver && shouldSkipDetails(selectedHomeserver)
            ? "Back to homeservers"
            : "Back to details";

  const captchaWarningText = selectedHomeserver ? captchaWarning(selectedHomeserver) : "";
  const selectedHomepage = selectedHomeserver ? safeLink(selectedHomeserver.homepage) : null;
  const selectedRules = selectedHomeserver ? safeLink(selectedHomeserver.rules) : null;
  const selectedPrivacy = selectedHomeserver ? safeLink(selectedHomeserver.privacy) : null;

  return (
    <div className="registration-shell">
      <header className="registration-header">
        <button type="button" className="registration-back-button" onClick={handleBack}>
          <BackArrowGlyph />
          <span>{backLabel}</span>
        </button>
      </header>

      <main className="registration-main">
        {stage === "directory" ? (
          <section className="registration-screen registration-screen--directory">
            <h1 className="registration-screen-title">Homeservers with open registration</h1>
            <p className="registration-screen-copy">
              Choose a homeserver first. Open the details screen only when you want to
              inspect one more closely.
            </p>

            <div className="registration-toolbar">
              <label className="registration-search">
                <span className="registration-search-icon">
                  <SearchGlyph />
                </span>
                <input
                  className="registration-search-control"
                  name="homeserver-search"
                  onChange={(event) => setSearchQuery(event.currentTarget.value)}
                  placeholder="Search homeservers, domains, or software"
                  spellCheck={false}
                  type="search"
                  value={searchQuery}
                />
              </label>

              <button
                type="button"
                className="registration-secondary-button"
                disabled={isLoadingHomeservers || isRefreshingHomeservers}
                onClick={() => void loadHomeservers("refresh")}
              >
                {isRefreshingHomeservers ? "Refreshing..." : "Refresh"}
              </button>
            </div>

            <p className="registration-toolbar-meta">
              {isLoadingHomeservers
                ? "Loading published registration metadata..."
                : `${visibleHomeservers.length} ${visibleHomeservers.length === 1 ? "homeserver" : "homeservers"} available`}
            </p>

            {feedback ? (
              <p className={`registration-feedback registration-feedback--${feedback.tone}`}>
                {feedback.text}
              </p>
            ) : null}

            {isLoadingHomeservers ? (
              <p className="registration-empty-state">
                Pulling the latest public homeserver directory from the native layer.
              </p>
            ) : visibleHomeservers.length > 0 ? (
              <div className="registration-directory-grid" role="list">
                {visibleHomeservers.map((homeserver) => (
                  <button
                    key={homeserver.server_id}
                    type="button"
                    className="registration-server-card"
                    onClick={() => openDetails(homeserver)}
                    role="listitem"
                  >
                    <div className="registration-server-card-head">
                      <div className="registration-server-card-copy">
                        <span className="registration-server-title">
                          {homeserverTitle(homeserver)}
                        </span>
                        <span className="registration-server-subtitle">
                          {homeserverHost(homeserver)}
                        </span>
                      </div>

                      <span className="registration-flow-pill">
                        {flowLabel(homeserver.registration_flow)}
                      </span>
                    </div>

                    <p className="registration-server-description">
                      {homeserverCopy(homeserver)}
                    </p>

                    <div className="registration-server-meta">
                      {homeserver.is_official ? (
                        <span className="registration-official-pill">Official</span>
                      ) : null}
                      {homeserver.software ? (
                        <span className="registration-info-pill">
                          {homeserver.version
                            ? `${homeserver.software} ${homeserver.version}`
                            : homeserver.software}
                        </span>
                      ) : null}
                      {homeserver.longstanding ? (
                        <span className="registration-info-pill">Established</span>
                      ) : null}
                    </div>
                  </button>
                ))}
              </div>
            ) : (
              <p className="registration-empty-state">
                No homeservers matched your search. Try a broader query or refresh the
                directory.
              </p>
            )}
          </section>
        ) : null}

        {stage === "details" && selectedHomeserver ? (
          <section className="registration-screen registration-screen--narrow">
            <h1 className="registration-screen-title">
              {homeserverTitle(selectedHomeserver)}
            </h1>
            <p className="registration-screen-copy">{homeserverCopy(selectedHomeserver)}</p>

            <div className="registration-detail-tags">
              {selectedHomeserver.is_official ? (
                <span className="registration-official-pill">Official</span>
              ) : null}
              <span className="registration-flow-pill">
                {flowLabel(selectedHomeserver.registration_flow)}
              </span>
              <span className="registration-info-pill">
                {selectedHomeserver.homeserver_url ?? homeserverHost(selectedHomeserver)}
              </span>
              {selectedHomeserver.reg_method ? (
                <span className="registration-info-pill">{selectedHomeserver.reg_method}</span>
              ) : null}
            </div>

            {feedback ? (
              <p className={`registration-feedback registration-feedback--${feedback.tone}`}>
                {feedback.text}
              </p>
            ) : null}

            {selectedHomeserver.registration_flow !== "matrix_sdk" ? (
              <p className="registration-warning">
                {handoffWarning(selectedHomeserver, "external_flow")}
              </p>
            ) : null}

            {captchaWarningText ? (
              <p className="registration-warning registration-warning--accent">
                {captchaWarningText}
              </p>
            ) : null}

            <div className="registration-detail-grid">
              <article className="registration-detail-card">
                <h2 className="registration-detail-title">Links</h2>
                <div className="registration-link-list">
                  {selectedHomepage ? (
                    <button
                      type="button"
                      className="registration-link-button"
                      onClick={() =>
                        openPublishedLink(
                          selectedHomepage,
                          `${homeserverTitle(selectedHomeserver)} homepage`,
                        )
                      }
                    >
                      Homepage
                    </button>
                  ) : null}
                  {selectedRules ? (
                    <button
                      type="button"
                      className="registration-link-button"
                      onClick={() =>
                        openPublishedLink(
                          selectedRules,
                          `${homeserverTitle(selectedHomeserver)} rules`,
                        )
                      }
                    >
                      Rules
                    </button>
                  ) : null}
                  {selectedPrivacy ? (
                    <button
                      type="button"
                      className="registration-link-button"
                      onClick={() =>
                        openPublishedLink(
                          selectedPrivacy,
                          `${homeserverTitle(selectedHomeserver)} privacy policy`,
                        )
                      }
                    >
                      Privacy policy
                    </button>
                  ) : null}
                  {!selectedHomepage && !selectedRules && !selectedPrivacy ? (
                    <p className="registration-detail-copy">
                      No additional links were published for this homeserver.
                    </p>
                  ) : null}
                </div>
              </article>

              <article className="registration-detail-card">
                <h2 className="registration-detail-title">Registration</h2>
                <dl className="registration-detail-list">
                  <div>
                    <dt>Flow</dt>
                    <dd>{flowLabel(selectedHomeserver.registration_flow)}</dd>
                  </div>
                  <div>
                    <dt>Email</dt>
                    <dd>{boolLabel(selectedHomeserver.email, "Required")}</dd>
                  </div>
                  <div>
                    <dt>Captcha</dt>
                    <dd>{boolLabel(selectedHomeserver.captcha, "Required")}</dd>
                  </div>
                  <div>
                    <dt>Display name</dt>
                    <dd>
                      {selectedHomeserver.supports_display_name ? "Supported" : "Not supported"}
                    </dd>
                  </div>
                </dl>
              </article>

              <article className="registration-detail-card">
                <h2 className="registration-detail-title">Technical details</h2>
                <dl className="registration-detail-list">
                  <div>
                    <dt>Software</dt>
                    <dd>
                      {selectedHomeserver.software
                        ? selectedHomeserver.version
                          ? `${selectedHomeserver.software} ${selectedHomeserver.version}`
                          : selectedHomeserver.software
                        : "Unknown"}
                    </dd>
                  </div>
                  <div>
                    <dt>Sliding sync</dt>
                    <dd>{boolLabel(selectedHomeserver.sliding_sync)}</dd>
                  </div>
                  <div>
                    <dt>IPv6</dt>
                    <dd>{boolLabel(selectedHomeserver.ipv6)}</dd>
                  </div>
                  <div>
                    <dt>Cloudflare</dt>
                    <dd>{boolLabel(selectedHomeserver.cloudflare)}</dd>
                  </div>
                </dl>
              </article>

              <article className="registration-detail-card">
                <h2 className="registration-detail-title">Jurisdiction</h2>
                <dl className="registration-detail-list">
                  <div>
                    <dt>ISP</dt>
                    <dd>{selectedHomeserver.isp ?? "Unknown"}</dd>
                  </div>
                  <div>
                    <dt>Staff jurisdiction</dt>
                    <dd>{selectedHomeserver.staff_jur ?? "Unknown"}</dd>
                  </div>
                  <div>
                    <dt>Languages</dt>
                    <dd>
                      {selectedHomeserver.languages.length > 0
                        ? selectedHomeserver.languages.join(", ")
                        : "Not listed"}
                    </dd>
                  </div>
                  <div>
                    <dt>Features</dt>
                    <dd>
                      {selectedHomeserver.features.length > 0
                        ? selectedHomeserver.features.join(", ")
                        : "No extra features listed"}
                    </dd>
                  </div>
                </dl>
              </article>
            </div>

            {selectedHomeserver.reg_note ? (
              <p className="registration-note">{selectedHomeserver.reg_note}</p>
            ) : null}

            <div className="registration-action-row">
              {selectedHomeserver.registration_flow === "matrix_sdk" ? (
                <button type="button" className="registration-primary-button" onClick={openForm}>
                  Continue to registration form
                </button>
              ) : (
                <button
                  type="button"
                  className="registration-primary-button"
                  disabled={isSubmitting}
                  onClick={() => void handleNonVanillaAction()}
                >
                  {isSubmitting
                    ? "Working..."
                    : selectedHomeserver.registration_flow === "external_link"
                      ? "Open registration in Hyperion"
                      : "Continue with guidance"}
                </button>
              )}
            </div>
          </section>
        ) : null}

        {stage === "form" && selectedHomeserver ? (
          <section className="registration-screen registration-screen--form">
            <h1 className="registration-screen-title">
              Register on {homeserverTitle(selectedHomeserver)}
            </h1>
            <p className="registration-screen-copy">
              Finish the form below to create the account.
            </p>

            <div className="registration-detail-tags">
              {selectedHomeserver.is_official ? (
                <span className="registration-official-pill">Official</span>
              ) : null}
              <span className="registration-info-pill">
                {selectedHomeserver.homeserver_url ?? homeserverHost(selectedHomeserver)}
              </span>
            </div>

            {feedback ? (
              <p className={`registration-feedback registration-feedback--${feedback.tone}`}>
                {feedback.text}
              </p>
            ) : null}

            {captchaWarningText ? (
              <p className="registration-warning registration-warning--accent">
                {captchaWarningText}
              </p>
            ) : null}

            {selectedHomeserver.reg_note ? (
              <p className="registration-note">{selectedHomeserver.reg_note}</p>
            ) : null}

            <form className="registration-form" noValidate onSubmit={handleSubmit}>
              <label className="registration-field">
                <span className="registration-field-label">
                  Username
                  <span className="registration-field-required" aria-hidden="true">
                    *
                  </span>
                </span>
                <input
                  autoCapitalize="none"
                  autoComplete="username"
                  className="registration-field-control"
                  data-invalid={usernameMissing || undefined}
                  name="username"
                  onChange={(event) => updateField("username", event.currentTarget.value)}
                  spellCheck={false}
                  type="text"
                  value={formValues.username}
                />
              </label>

              {selectedHomeserver.supports_display_name ? (
                <label className="registration-field">
                  <span className="registration-field-label">Display name</span>
                  <input
                    autoComplete="nickname"
                    className="registration-field-control"
                    name="display-name"
                    onChange={(event) =>
                      updateField("displayName", event.currentTarget.value)
                    }
                    type="text"
                    value={formValues.displayName}
                  />
                </label>
              ) : null}

              <label className="registration-field">
                <span className="registration-field-label">
                  Password
                  <span className="registration-field-required" aria-hidden="true">
                    *
                  </span>
                </span>
                <input
                  autoComplete="new-password"
                  className="registration-field-control"
                  data-invalid={passwordMissing || undefined}
                  name="password"
                  onChange={(event) => updateField("password", event.currentTarget.value)}
                  type="password"
                  value={formValues.password}
                />
              </label>

              <label className="registration-field">
                <span className="registration-field-label">
                  Email
                  {emailRequired ? (
                    <span className="registration-field-required" aria-hidden="true">
                      *
                    </span>
                  ) : null}
                </span>
                <input
                  aria-required={emailRequired}
                  autoComplete="email"
                  className="registration-field-control"
                  data-invalid={emailMissing || undefined}
                  inputMode="email"
                  name="email"
                  onChange={(event) => updateField("email", event.currentTarget.value)}
                  required={emailRequired}
                  type="email"
                  value={formValues.email}
                />
              </label>

              <div className="registration-form-foot">
                <span className="registration-required-copy">
                  <span className="registration-field-required" aria-hidden="true">
                    *
                  </span>{" "}
                  Required fields
                </span>

                <button
                  type="submit"
                  className="registration-primary-button"
                  disabled={isSubmitting}
                >
                  {isSubmitting ? "Creating account..." : "Create account"}
                </button>
              </div>
            </form>
          </section>
        ) : null}

        {stage === "webview" && embeddedWebview ? (
          <section className="registration-screen registration-screen--webview">
            <div className="registration-webview-bar">
              <div className="registration-webview-copy">
                <span className="registration-webview-eyebrow">
                  {embeddedWebview.kind === "registration"
                    ? "Registration page"
                    : "Published homeserver link"}
                </span>
                <h1 className="registration-webview-title">{embeddedWebview.title}</h1>
                <p className="registration-webview-url">
                  {formatWebviewUrl(embeddedWebview.url)}
                </p>
              </div>

              {embeddedWebview.kind === "registration" && selectedHomeserver ? (
                <button
                  type="button"
                  className="registration-secondary-button"
                  onClick={() =>
                    finishInLogin({
                      homeserver: selectedHomeserver.homeserver_url ?? undefined,
                      text: `If you completed registration on ${homeserverTitle(selectedHomeserver)}, sign in here.`,
                      tone: "info",
                    })
                  }
                >
                  Go to log in
                </button>
              ) : null}
            </div>

            {embeddedWebview.warning ? (
              <p className="registration-warning registration-warning--accent">
                {embeddedWebview.warning}
              </p>
            ) : null}

            <div
              ref={webviewHostRef}
              className="registration-webview-host"
              aria-label={`${embeddedWebview.title} webview`}
            />
          </section>
        ) : null}
      </main>
    </div>
  );
}
