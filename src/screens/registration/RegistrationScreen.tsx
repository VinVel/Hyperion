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
import { ArrowLeft } from "lucide-react";
import {
  type SyntheticEvent,
  useDeferredValue,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import {
  Button,
  FeedbackMessage,
  Panel,
  Pill,
  ScreenMain,
  ScreenShell,
  TextField,
  Typography,
} from "../../components/ui";
import { defaultDesktopUserAgent } from "../../config/defaultDesktopUserAgent";
import { HomeserverDetailsScreen } from "./HomeserverDetailsScreen";
import { HomeserverDirectoryScreen } from "./HomeserverDirectoryScreen";
import {
  type FeedbackMessage as RegistrationFeedbackMessage,
  type HomeserverDirectory,
  type HomeserverDirectoryEntry,
  captchaWarning,
  formatWebviewUrl,
  getErrorMessage,
  handoffWarning,
  homeserverHost,
  homeserverTitle,
  normalizeHomeservers,
  registrationFlowOrder,
  safeLink,
  shouldSkipDetails,
} from "./registrationShared";
import "./RegistrationScreen.css";

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
const defaultFormValues: RegistrationFormValues = {
  username: "",
  displayName: "",
  password: "",
  email: "",
};

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
  const [feedback, setFeedback] = useState<RegistrationFeedbackMessage | null>(null);
  const [validationRequested, setValidationRequested] = useState(false);
  const [isLoadingHomeservers, setIsLoadingHomeservers] = useState(true);
  const [isRefreshingHomeservers, setIsRefreshingHomeservers] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [embeddedWebview, setEmbeddedWebview] = useState<EmbeddedWebviewState | null>(null);
  const webviewHostRef = useRef<HTMLDivElement | null>(null);
  const latestHomeserverRequestIdRef = useRef(0);
  const deferredSearchQuery = useDeferredValue(searchQuery.trim().toLowerCase());

  async function loadHomeservers(reason: "initial" | "refresh" = "initial") {
    const requestId = latestHomeserverRequestIdRef.current + 1;
    latestHomeserverRequestIdRef.current = requestId;

    if (reason === "refresh") {
      setIsRefreshingHomeservers(true);
    } else {
      setIsLoadingHomeservers(true);
    }

    try {
      const directory = await invoke<HomeserverDirectory>("list_registration_homeservers");
      const nextHomeservers = normalizeHomeservers(directory.public_servers)
        .filter((homeserver) => homeserver.server_id.trim().length > 0)
        .sort(
          (left, right) =>
            Number(right.is_official === true) - Number(left.is_official === true) ||
            registrationFlowOrder[left.registration_flow] - registrationFlowOrder[right.registration_flow] ||
            homeserverTitle(left).localeCompare(homeserverTitle(right)),
        );

      if (requestId !== latestHomeserverRequestIdRef.current) {
        return;
      }

      setHomeservers(nextHomeservers);
      setSelectedServerId((current) =>
        current && nextHomeservers.some((homeserver) => homeserver.server_id === current)
          ? current
          : null,
      );

      if (reason === "initial" || reason === "refresh") {
        setFeedback((currentFeedback) =>
          currentFeedback?.tone === "error" ? null : currentFeedback,
        );
      }
    } catch (error) {
      if (requestId !== latestHomeserverRequestIdRef.current) {
        return;
      }

      setFeedback({ tone: "error", text: getErrorMessage(error) });
    } finally {
      if (requestId === latestHomeserverRequestIdRef.current) {
        setIsLoadingHomeservers(false);
        setIsRefreshingHomeservers(false);
      }
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
        userAgent: defaultDesktopUserAgent,
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
        void fallbackToMobileOverlayOrBrowser(embeddedWebview).catch((fallbackError) => {
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

  function handleOpenedMobileOverlay(nextWebview: EmbeddedWebviewState) {
    setEmbeddedWebview(null);

    if (nextWebview.kind === "registration" && selectedHomeserver) {
      finishInLogin({
        homeserver: selectedHomeserver.homeserver_url ?? undefined,
        text: `Opened the registration page in the in-app browser overlay. Close it when finished, then sign in here.`,
        tone: "info",
      });
      return;
    }

    setStage(nextWebview.returnStage);
    setFeedback({
      tone: "info",
      text: "Opened the page in the in-app browser overlay.",
    });
  }

  async function fallbackToMobileOverlayOrBrowser(nextWebview: EmbeddedWebviewState) {
    try {
      await invoke("open_mobile_overlay_webview", {
        url: nextWebview.url,
        title: nextWebview.title,
        userAgent: defaultDesktopUserAgent,
      });
      handleOpenedMobileOverlay(nextWebview);
      return;
    } catch {
      // Fall through to the existing browser-based fallbacks.
    }

    let openedIn = "an in-app browser";

    try {
      await openUrl(nextWebview.url, "inAppBrowser");
    } catch {
      await openUrl(nextWebview.url);
      openedIn = "your browser";
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

  async function handleSubmit(event: SyntheticEvent<HTMLFormElement, SubmitEvent>) {
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
      <ScreenMain className="registration-main">
        <div className="registration-back-row">
          <Button className="registration-back-button" onClick={handleBack}>
            <ArrowLeft aria-hidden="true" />
            <span>{backLabel}</span>
          </Button>
        </div>

        {stage === "directory" ? (
          <HomeserverDirectoryScreen
            feedback={feedback}
            isLoadingHomeservers={isLoadingHomeservers}
            isRefreshingHomeservers={isRefreshingHomeservers}
            searchQuery={searchQuery}
            visibleHomeservers={visibleHomeservers}
            onOpenHomeserver={openDetails}
            onRefreshHomeservers={() => void loadHomeservers("refresh")}
            onSearchQueryChange={setSearchQuery}
          />
        ) : null}

        {stage === "details" && selectedHomeserver ? (
          <HomeserverDetailsScreen
            feedback={feedback}
            homeserver={selectedHomeserver}
            isSubmitting={isSubmitting}
            captchaWarningText={captchaWarningText}
            homepageUrl={selectedHomepage}
            rulesUrl={selectedRules}
            privacyUrl={selectedPrivacy}
            onOpenPublishedLink={openPublishedLink}
            onOpenRegistrationForm={openForm}
            onContinueHomeserverFlow={() => void handleNonVanillaAction()}
          />
        ) : null}

        {stage === "form" && selectedHomeserver ? (
          <section
            className="registration-screen--narrow registration-screen--form"
            aria-labelledby="registration-form-title"
          >
            <Typography variant="h1" id="registration-form-title">
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
          </section>
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
