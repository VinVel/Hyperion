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
import { openUrl } from "@tauri-apps/plugin-opener";
import { LogicalPosition, LogicalSize, getCurrentWindow } from "@tauri-apps/api/window";
import { ArrowLeft, Search } from "lucide-react";
import {
  type FormEvent,
  useDeferredValue,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import {
  Button,
  Card,
  FeedbackMessage,
  Panel,
  Pill,
  ScreenHeader,
  ScreenMain,
  ScreenShell,
  TextField,
  Typography,
} from "../../components/ui";
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

function isMobileWebviewUnavailableError(error: unknown): boolean {
  return getErrorMessage(error).toLowerCase().includes("webview api not available on mobile");
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

      if (isMobileWebviewUnavailableError(error)) {
        void fallbackToExternalBrowser(embeddedWebview).catch((fallbackError) => {
          if (disposed) return;

          setEmbeddedWebview(null);
          setStage(embeddedWebview.returnStage);
          setFeedback({
            tone: "error",
            text: `Failed to open the browser fallback: ${getErrorMessage(fallbackError)}`,
          });
        });
        return;
      }

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

  async function fallbackToExternalBrowser(nextWebview: EmbeddedWebviewState) {
    let openedIn = "Android Custom Tabs";

    try {
      await invoke("open_android_custom_tab", { url: nextWebview.url });
    } catch {
      try {
        await openUrl(nextWebview.url, "inAppBrowser");
        openedIn = "an in-app browser";
      } catch {
        await openUrl(nextWebview.url);
        openedIn = "your browser";
      }
    }

    setEmbeddedWebview(null);

    if (nextWebview.kind === "registration" && selectedHomeserver) {
      finishInLogin({
        homeserver: selectedHomeserver.homeserver_url ?? undefined,
        text: `Opened the registration page in ${openedIn} because the embedded webview is not available on mobile. Close it when finished, then sign in here.`,
        tone: "info",
      });
      return;
    }

    setStage(nextWebview.returnStage);
    setFeedback({
      tone: "info",
      text: `Opened the page in ${openedIn} because the embedded webview is not available on mobile.`,
    });
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
    <ScreenShell>
      <ScreenHeader className="registration-header">
        <Button className="registration-back-button" onClick={handleBack}>
          <ArrowLeft aria-hidden="true" />
          <span>{backLabel}</span>
        </Button>
      </ScreenHeader>

      <ScreenMain className="registration-main">
        {stage === "directory" ? (
          <Panel className="registration-screen--directory">
            <Typography variant="h1">Homeservers with open registration</Typography>
            <Typography variant="body" muted className="registration-screen-copy">
              Choose a homeserver first. Open the details screen only when you want to
              inspect one more closely.
            </Typography>

            <div className="registration-toolbar">
              <label className="registration-search">
                <span className="registration-search-icon">
                  <Search aria-hidden="true" />
                </span>
                <input
                  className="ui-field__control registration-search-control"
                  name="homeserver-search"
                  onChange={(event) => setSearchQuery(event.currentTarget.value)}
                  placeholder="Search homeservers, domains, or software"
                  spellCheck={false}
                  type="search"
                  value={searchQuery}
                />
              </label>

              <Button
                variant="secondary"
                disabled={isLoadingHomeservers || isRefreshingHomeservers}
                onClick={() => void loadHomeservers("refresh")}
              >
                {isRefreshingHomeservers ? "Refreshing..." : "Refresh"}
              </Button>
            </div>

            <Typography variant="meta" muted className="registration-toolbar-meta">
              {isLoadingHomeservers
                ? "Loading published registration metadata..."
                : `${visibleHomeservers.length} ${visibleHomeservers.length === 1 ? "homeserver" : "homeservers"} available`}
            </Typography>

            {feedback ? (
              <FeedbackMessage tone={feedback.tone}>
                {feedback.text}
              </FeedbackMessage>
            ) : null}

            {isLoadingHomeservers ? (
              <Typography variant="body" muted className="registration-empty-state">
                Pulling the latest public homeserver directory from the native layer.
              </Typography>
            ) : visibleHomeservers.length > 0 ? (
              <div className="registration-directory-grid" role="list">
                {visibleHomeservers.map((homeserver) => (
                  <Button
                    key={homeserver.server_id}
                    variant="secondary"
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

                      <Pill tone="secondary">
                        {flowLabel(homeserver.registration_flow)}
                      </Pill>
                    </div>

                    <p className="registration-server-description">
                      {homeserverCopy(homeserver)}
                    </p>

                    <div className="registration-server-meta">
                      {homeserver.is_official ? (
                        <Pill tone="primary">Official</Pill>
                      ) : null}
                      {homeserver.software ? (
                        <Pill>
                          {homeserver.version
                            ? `${homeserver.software} ${homeserver.version}`
                            : homeserver.software}
                        </Pill>
                      ) : null}
                      {homeserver.longstanding ? (
                        <Pill>Established</Pill>
                      ) : null}
                    </div>
                  </Button>
                ))}
              </div>
            ) : (
              <Typography variant="body" muted className="registration-empty-state">
                No homeservers matched your search. Try a broader query or refresh the
                directory.
              </Typography>
            )}
          </Panel>
        ) : null}

        {stage === "details" && selectedHomeserver ? (
          <Panel className="registration-screen--narrow" narrow>
            <Typography variant="h1">
              {homeserverTitle(selectedHomeserver)}
            </Typography>
            <Typography variant="body" muted className="registration-screen-copy">
              {homeserverCopy(selectedHomeserver)}
            </Typography>

            <div className="registration-detail-tags">
              {selectedHomeserver.is_official ? (
                <Pill tone="primary">Official</Pill>
              ) : null}
              <Pill tone="secondary">
                {flowLabel(selectedHomeserver.registration_flow)}
              </Pill>
              <Pill>
                {selectedHomeserver.homeserver_url ?? homeserverHost(selectedHomeserver)}
              </Pill>
              {selectedHomeserver.reg_method ? (
                <Pill>{selectedHomeserver.reg_method}</Pill>
              ) : null}
            </div>

            {feedback ? (
              <FeedbackMessage tone={feedback.tone}>
                {feedback.text}
              </FeedbackMessage>
            ) : null}

            {selectedHomeserver.registration_flow !== "matrix_sdk" ? (
              <FeedbackMessage tone="warning" className="registration-warning">
                {handoffWarning(selectedHomeserver, "external_flow")}
              </FeedbackMessage>
            ) : null}

            {captchaWarningText ? (
              <FeedbackMessage tone="error" className="registration-warning">
                {captchaWarningText}
              </FeedbackMessage>
            ) : null}

            <div className="registration-detail-grid">
              <Card>
                <Typography as="h2" variant="h3" className="registration-detail-title">
                  Links
                </Typography>
                <div className="registration-link-list">
                  {selectedHomepage ? (
                    <Button
                      variant="ghost"
                      className="registration-link-button"
                      onClick={() =>
                        openPublishedLink(
                          selectedHomepage,
                          `${homeserverTitle(selectedHomeserver)} homepage`,
                        )
                      }
                    >
                      Homepage
                    </Button>
                  ) : null}
                  {selectedRules ? (
                    <Button
                      variant="ghost"
                      className="registration-link-button"
                      onClick={() =>
                        openPublishedLink(
                          selectedRules,
                          `${homeserverTitle(selectedHomeserver)} rules`,
                        )
                      }
                    >
                      Rules
                    </Button>
                  ) : null}
                  {selectedPrivacy ? (
                    <Button
                      variant="ghost"
                      className="registration-link-button"
                      onClick={() =>
                        openPublishedLink(
                          selectedPrivacy,
                          `${homeserverTitle(selectedHomeserver)} privacy policy`,
                        )
                      }
                    >
                      Privacy policy
                    </Button>
                  ) : null}
                  {!selectedHomepage && !selectedRules && !selectedPrivacy ? (
                    <Typography variant="bodySmall" muted className="registration-detail-copy">
                      No additional links were published for this homeserver.
                    </Typography>
                  ) : null}
                </div>
              </Card>

              <Card>
                <Typography as="h2" variant="h3" className="registration-detail-title">
                  Registration
                </Typography>
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
              </Card>

              <Card>
                <Typography as="h2" variant="h3" className="registration-detail-title">
                  Technical details
                </Typography>
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
              </Card>

              <Card>
                <Typography as="h2" variant="h3" className="registration-detail-title">
                  Jurisdiction
                </Typography>
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
              </Card>
            </div>

            {selectedHomeserver.reg_note ? (
              <FeedbackMessage tone="info" className="registration-note">
                {selectedHomeserver.reg_note}
              </FeedbackMessage>
            ) : null}

            <div className="registration-action-row">
              {selectedHomeserver.registration_flow === "matrix_sdk" ? (
                <Button variant="primary" onClick={openForm}>
                  Continue to registration form
                </Button>
              ) : (
                <Button
                  variant="primary"
                  disabled={isSubmitting}
                  onClick={() => void handleNonVanillaAction()}
                >
                  {isSubmitting
                    ? "Working..."
                    : selectedHomeserver.registration_flow === "external_link"
                      ? "Open registration in Hyperion"
                      : "Continue with guidance"}
                </Button>
              )}
            </div>
          </Panel>
        ) : null}

        {stage === "form" && selectedHomeserver ? (
          <Panel className="registration-screen--form" narrow>
            <Typography variant="h1">
              Register on {homeserverTitle(selectedHomeserver)}
            </Typography>
            <Typography variant="body" muted className="registration-screen-copy">
              Finish the form below to create the account.
            </Typography>

            <div className="registration-detail-tags">
              {selectedHomeserver.is_official ? (
                <Pill tone="primary">Official</Pill>
              ) : null}
              <Pill>
                {selectedHomeserver.homeserver_url ?? homeserverHost(selectedHomeserver)}
              </Pill>
            </div>

            {feedback ? (
              <FeedbackMessage tone={feedback.tone}>
                {feedback.text}
              </FeedbackMessage>
            ) : null}

            {captchaWarningText ? (
              <FeedbackMessage tone="error" className="registration-warning">
                {captchaWarningText}
              </FeedbackMessage>
            ) : null}

            {selectedHomeserver.reg_note ? (
              <FeedbackMessage tone="info" className="registration-note">
                {selectedHomeserver.reg_note}
              </FeedbackMessage>
            ) : null}

            <form className="registration-form" noValidate onSubmit={handleSubmit}>
              <TextField
                autoCapitalize="none"
                autoComplete="username"
                isInvalid={usernameMissing}
                isRequiredVisible
                label="Username"
                name="username"
                onChange={(event) => updateField("username", event.currentTarget.value)}
                spellCheck={false}
                type="text"
                value={formValues.username}
              />

              {selectedHomeserver.supports_display_name ? (
                <TextField
                  autoComplete="nickname"
                  label="Display name"
                  name="display-name"
                  onChange={(event) => updateField("displayName", event.currentTarget.value)}
                  type="text"
                  value={formValues.displayName}
                />
              ) : null}

              <TextField
                autoComplete="new-password"
                isInvalid={passwordMissing}
                isRequiredVisible
                label="Password"
                name="password"
                onChange={(event) => updateField("password", event.currentTarget.value)}
                type="password"
                value={formValues.password}
              />

              <TextField
                aria-required={emailRequired}
                autoComplete="email"
                inputMode="email"
                isInvalid={emailMissing}
                isRequiredVisible={emailRequired}
                label="Email"
                name="email"
                onChange={(event) => updateField("email", event.currentTarget.value)}
                required={emailRequired}
                type="email"
                value={formValues.email}
              />

              <div className="registration-form-foot">
                <span className="registration-required-copy">
                  <span className="ui-required-marker" aria-hidden="true">
                    *
                  </span>{" "}
                  Required fields
                </span>

                <Button
                  type="submit"
                  variant="primary"
                  disabled={isSubmitting}
                >
                  {isSubmitting ? "Creating account..." : "Create account"}
                </Button>
              </div>
            </form>
          </Panel>
        ) : null}

        {stage === "webview" && embeddedWebview ? (
          <Panel className="registration-screen--webview">
            <div className="registration-webview-bar">
              <div className="registration-webview-copy">
                <Typography as="span" variant="label" className="registration-webview-eyebrow">
                  {embeddedWebview.kind === "registration"
                    ? "Registration page"
                    : "Published homeserver link"}
                </Typography>
                <Typography variant="h2" className="registration-webview-title">
                  {embeddedWebview.title}
                </Typography>
                <Typography variant="bodySmall" muted className="registration-webview-url">
                  {formatWebviewUrl(embeddedWebview.url)}
                </Typography>
              </div>

              {embeddedWebview.kind === "registration" && selectedHomeserver ? (
                <Button
                  variant="secondary"
                  onClick={() =>
                    finishInLogin({
                      homeserver: selectedHomeserver.homeserver_url ?? undefined,
                      text: `If you completed registration on ${homeserverTitle(selectedHomeserver)}, sign in here.`,
                      tone: "info",
                    })
                  }
                >
                  Go to log in
                </Button>
              ) : null}
            </div>

            {embeddedWebview.warning ? (
              <FeedbackMessage tone="error" className="registration-warning">
                {embeddedWebview.warning}
              </FeedbackMessage>
            ) : null}

            <div
              ref={webviewHostRef}
              className="registration-webview-host"
              aria-label={`${embeddedWebview.title} webview`}
            />
          </Panel>
        ) : null}
      </ScreenMain>
    </ScreenShell>
  );
}
