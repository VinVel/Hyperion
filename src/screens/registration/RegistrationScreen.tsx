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
import {
  type RefObject,
  type SyntheticEvent,
  useDeferredValue,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import {
  BackButton,
  Button,
  notifyFeedback,
  Panel,
  Pill,
  ScreenMain,
  ScreenShell,
  TextField,
  toastVisibilityChangedEvent,
  Typography,
  useFeedbackToast,
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
type WebviewBounds = {
  height: number;
  width: number;
  x: number;
  y: number;
};
type RegistrationFormStageProps = {
  captchaWarningText: string;
  emailMissing: boolean;
  emailRequired: boolean;
  formValues: RegistrationFormValues;
  isSubmitting: boolean;
  passwordMissing: boolean;
  selectedHomeserver: HomeserverDirectoryEntry;
  usernameMissing: boolean;
  onBack: () => void;
  onFieldChange: (field: keyof RegistrationFormValues, value: string) => void;
  onSubmit: (event: SyntheticEvent<HTMLFormElement, SubmitEvent>) => void;
};
type EmbeddedWebviewStageProps = {
  embeddedWebview: EmbeddedWebviewState;
  selectedHomeserver: HomeserverDirectoryEntry | null;
  webviewHostRef: RefObject<HTMLDivElement | null>;
  onBack: () => void;
  onGoToLogin: () => void;
};

const DEVICE_DISPLAY_NAME = "Hyperion";
const EMBEDDED_WEBVIEW_LABEL = "registration-handoff-webview";
// Native child webviews paint above DOM content, so toasts need a non-overlapping viewport slot.
const nativeWebviewActiveClassName = "hyperion-native-webview-active";
const toastVisibleClassName = "hyperion-toast-visible";
const toastRegionSelector = ".ui-toast-region";
// Keep the native child webview visually separated from the floating toast.
const webviewToastGapPixels = 12;
const defaultFormValues: RegistrationFormValues = {
  username: "",
  displayName: "",
  password: "",
  email: "",
};

function isMobileWebviewUnavailableError(error: unknown): boolean {
  return getErrorMessage(error).toLowerCase().includes("webview api not available on mobile");
}

function compareHomeservers(
  left: HomeserverDirectoryEntry,
  right: HomeserverDirectoryEntry,
): number {
  const officialOrder = Number(right.is_official === true) - Number(left.is_official === true);
  if (officialOrder !== 0) {
    return officialOrder;
  }

  const flowOrder =
    registrationFlowOrder[left.registration_flow] -
    registrationFlowOrder[right.registration_flow];
  if (flowOrder !== 0) {
    return flowOrder;
  }

  return homeserverTitle(left).localeCompare(homeserverTitle(right));
}

function findRetainedHomeserverId(
  currentServerId: string | null,
  nextHomeservers: HomeserverDirectoryEntry[],
): string | null {
  if (!currentServerId) {
    return null;
  }

  const serverStillExists = nextHomeservers.some(
    (homeserver) => homeserver.server_id === currentServerId,
  );
  if (!serverStillExists) {
    return null;
  }

  return currentServerId;
}

function homeserverMatchesSearch(
  homeserver: HomeserverDirectoryEntry,
  normalizedQuery: string,
): boolean {
  if (normalizedQuery.length === 0) {
    return true;
  }

  const searchableText = [
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
    .toLowerCase();

  return searchableText.includes(normalizedQuery);
}

function getWebviewBounds(element: HTMLElement): WebviewBounds {
  const rect = element.getBoundingClientRect();
  const toastRect = getVisibleToastRect();

  if (!toastRect || !rectsOverlap(rect, toastRect)) {
    return roundedWebviewBounds(rect.left, rect.top, rect.right, rect.bottom);
  }

  let clippedTop = rect.top;
  let clippedBottom = rect.bottom;
  const toastIsInTopHalf = toastRect.top < window.innerHeight / 2;

  if (toastIsInTopHalf) {
    clippedTop = Math.min(rect.bottom - 1, toastRect.bottom + webviewToastGapPixels);
  } else {
    clippedBottom = Math.max(rect.top + 1, toastRect.top - webviewToastGapPixels);
  }

  return roundedWebviewBounds(rect.left, clippedTop, rect.right, clippedBottom);
}

function getVisibleToastRect(): DOMRect | null {
  if (!document.body.classList.contains(toastVisibleClassName)) {
    return null;
  }

  const toastRegion = document.querySelector<HTMLElement>(toastRegionSelector);
  if (!toastRegion) {
    return null;
  }

  return toastRegion.getBoundingClientRect();
}

function rectsOverlap(first: DOMRect, second: DOMRect): boolean {
  return (
    first.left < second.right &&
    first.right > second.left &&
    first.top < second.bottom &&
    first.bottom > second.top
  );
}

function roundedWebviewBounds(
  left: number,
  top: number,
  right: number,
  bottom: number,
): WebviewBounds {
  return {
    x: Math.round(left),
    y: Math.round(top),
    width: Math.max(1, Math.round(right - left)),
    height: Math.max(1, Math.round(bottom - top)),
  };
}

function formValueOrNull(value: string): string | null {
  const trimmedValue = value.trim();
  if (trimmedValue.length === 0) {
    return null;
  }

  return trimmedValue;
}

function registrationReturnStage(stage: RegistrationStage): NonWebviewStage {
  if (stage === "form") {
    return "form";
  }

  return "details";
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

  useFeedbackToast(feedback);

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
        .sort(compareHomeservers);

      if (requestId !== latestHomeserverRequestIdRef.current) {
        return;
      }

      setHomeservers(nextHomeservers);
      setSelectedServerId((current) => findRetainedHomeserverId(current, nextHomeservers));

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
    let bodyClassObserver: MutationObserver | null = null;
    let removeWindowResizeListener: (() => void) | null = null;
    let removeScrollListener: (() => void) | null = null;
    let currentWebview: Webview | null = null;

    const syncBounds = async () => {
      if (disposed || !currentWebview || !webviewHostRef.current) return;

      const nextBounds = getWebviewBounds(webviewHostRef.current);

      await Promise.allSettled([
        currentWebview.setPosition(
          new LogicalPosition(nextBounds.x, nextBounds.y),
        ),
        currentWebview.setSize(new LogicalSize(nextBounds.width, nextBounds.height)),
      ]);
    };

    const handleLayoutChange = () => {
      void syncBounds();
    };

    const openWebview = async () => {
      const existingWebview = await Webview.getByLabel(EMBEDDED_WEBVIEW_LABEL);
      if (existingWebview) {
        await existingWebview.close().catch(() => undefined);
      }

      if (disposed || !webviewHostRef.current) return;

      const initialBounds = getWebviewBounds(webviewHostRef.current);

      const nextWebview = new Webview(appWindow, EMBEDDED_WEBVIEW_LABEL, {
        url: embeddedWebview.url,
        x: initialBounds.x,
        y: initialBounds.y,
        width: initialBounds.width,
        height: initialBounds.height,
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

      if (typeof ResizeObserver !== "undefined") {
        resizeObserver = new ResizeObserver(handleLayoutChange);
        resizeObserver.observe(webviewHostRef.current);
      }

      if (typeof MutationObserver !== "undefined") {
        bodyClassObserver = new MutationObserver(handleLayoutChange);
        bodyClassObserver.observe(document.body, {
          attributeFilter: ["class"],
          attributes: true,
        });
      }

      removeWindowResizeListener = await appWindow.onResized(handleLayoutChange);
      window.addEventListener("scroll", handleLayoutChange, true);
      window.addEventListener(toastVisibilityChangedEvent, handleLayoutChange);
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
      bodyClassObserver?.disconnect();
      removeWindowResizeListener?.();
      removeScrollListener?.();
      window.removeEventListener(toastVisibilityChangedEvent, handleLayoutChange);

      void (async () => {
        const existingWebview =
          currentWebview ?? (await Webview.getByLabel(EMBEDDED_WEBVIEW_LABEL));
        await existingWebview?.close().catch(() => undefined);
      })();
    };
  }, [embeddedWebview, stage]);

  const visibleHomeservers = homeservers.filter((homeserver) =>
    homeserverMatchesSearch(homeserver, deferredSearchQuery),
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
      returnStage: registrationReturnStage(stage),
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
        returnStage: registrationReturnStage(stage),
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
          email: formValueOrNull(formValues.email),
          display_name: formValueOrNull(formValues.displayName),
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

  const captchaWarningText = selectedHomeserver ? captchaWarning(selectedHomeserver) : "";
  const selectedHomepage = selectedHomeserver ? safeLink(selectedHomeserver.homepage) : null;
  const selectedRules = selectedHomeserver ? safeLink(selectedHomeserver.rules) : null;
  const selectedPrivacy = selectedHomeserver ? safeLink(selectedHomeserver.privacy) : null;

  function handleLoginAfterEmbeddedRegistration() {
    if (!selectedHomeserver) {
      return;
    }

    finishInLogin({
      homeserver: selectedHomeserver.homeserver_url ?? undefined,
      text: `If you completed registration on ${homeserverTitle(selectedHomeserver)}, sign in here.`,
      tone: "info",
    });
  }

  return (
    <ScreenShell>
      <ScreenMain className="registration-main">
        {stage === "directory" ? (
          <HomeserverDirectoryScreen
            isLoadingHomeservers={isLoadingHomeservers}
            isRefreshingHomeservers={isRefreshingHomeservers}
            searchQuery={searchQuery}
            visibleHomeservers={visibleHomeservers}
            onBack={handleBack}
            onOpenHomeserver={openDetails}
            onRefreshHomeservers={() => void loadHomeservers("refresh")}
            onSearchQueryChange={setSearchQuery}
          />
        ) : null}

        {stage === "details" && selectedHomeserver ? (
          <HomeserverDetailsScreen
            homeserver={selectedHomeserver}
            isSubmitting={isSubmitting}
            captchaWarningText={captchaWarningText}
            homepageUrl={selectedHomepage}
            rulesUrl={selectedRules}
            privacyUrl={selectedPrivacy}
            onBack={handleBack}
            onOpenPublishedLink={openPublishedLink}
            onOpenRegistrationForm={openForm}
            onContinueHomeserverFlow={() => void handleNonVanillaAction()}
          />
        ) : null}

        {stage === "form" && selectedHomeserver ? (
          <RegistrationFormStage
            captchaWarningText={captchaWarningText}
            emailMissing={emailMissing}
            emailRequired={emailRequired}
            formValues={formValues}
            isSubmitting={isSubmitting}
            passwordMissing={passwordMissing}
            selectedHomeserver={selectedHomeserver}
            usernameMissing={usernameMissing}
            onBack={handleBack}
            onFieldChange={updateField}
            onSubmit={handleSubmit}
          />
        ) : null}

        {stage === "webview" && embeddedWebview ? (
          <EmbeddedWebviewStage
            embeddedWebview={embeddedWebview}
            selectedHomeserver={selectedHomeserver}
            webviewHostRef={webviewHostRef}
            onBack={handleBack}
            onGoToLogin={handleLoginAfterEmbeddedRegistration}
          />
        ) : null}
      </ScreenMain>
    </ScreenShell>
  );
}

function RegistrationFormStage({
  captchaWarningText,
  emailMissing,
  emailRequired,
  formValues,
  isSubmitting,
  passwordMissing,
  selectedHomeserver,
  usernameMissing,
  onBack,
  onFieldChange,
  onSubmit,
}: RegistrationFormStageProps) {
  useEffect(() => {
    if (captchaWarningText) {
      notifyFeedback({ tone: "error", text: captchaWarningText });
    }
  }, [captchaWarningText]);

  useEffect(() => {
    if (selectedHomeserver.reg_note) {
      notifyFeedback({ tone: "info", text: selectedHomeserver.reg_note });
    }
  }, [selectedHomeserver.reg_note]);

  return (
    <section
      className="registration-screen--narrow registration-screen--form"
      aria-labelledby="registration-form-title"
    >
      <div className="registration-heading-row">
        <BackButton onClick={onBack} />
        <Typography variant="h1" id="registration-form-title">
          Register on {homeserverTitle(selectedHomeserver)}
        </Typography>
      </div>
      <Typography variant="body" muted className="registration-screen-copy">
        Finish the form below to create the account.
      </Typography>

      <div className="registration-detail-tags">
        {selectedHomeserver.is_official ? <Pill tone="primary">Official</Pill> : null}
        <Pill>{selectedHomeserver.homeserver_url ?? homeserverHost(selectedHomeserver)}</Pill>
      </div>

      <form className="registration-form" noValidate onSubmit={onSubmit}>
        <TextField
          autoCapitalize="none"
          autoComplete="username"
          isInvalid={usernameMissing}
          isRequiredVisible
          label="Username"
          name="username"
          onChange={(event) => onFieldChange("username", event.currentTarget.value)}
          spellCheck={false}
          type="text"
          value={formValues.username}
        />

        {selectedHomeserver.supports_display_name ? (
          <TextField
            autoComplete="nickname"
            label="Display name"
            name="display-name"
            onChange={(event) => onFieldChange("displayName", event.currentTarget.value)}
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
          onChange={(event) => onFieldChange("password", event.currentTarget.value)}
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
          onChange={(event) => onFieldChange("email", event.currentTarget.value)}
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

          <Button type="submit" variant="primary" disabled={isSubmitting}>
            {isSubmitting ? "Creating account..." : "Create account"}
          </Button>
        </div>
      </form>
    </section>
  );
}

function EmbeddedWebviewStage({
  embeddedWebview,
  selectedHomeserver,
  webviewHostRef,
  onBack,
  onGoToLogin,
}: EmbeddedWebviewStageProps) {
  const webviewKindLabel =
    embeddedWebview.kind === "registration" ? "Registration page" : "Published homeserver link";

  useEffect(() => {
    document.body.classList.add(nativeWebviewActiveClassName);

    return () => {
      document.body.classList.remove(nativeWebviewActiveClassName);
    };
  }, []);

  useEffect(() => {
    if (embeddedWebview.warning) {
      notifyFeedback({ tone: "error", text: embeddedWebview.warning });
    }
  }, [embeddedWebview.warning]);

  return (
    <Panel className="registration-screen--webview">
      <div className="registration-webview-bar">
        <BackButton onClick={onBack} />
        <div className="registration-webview-copy">
          <Typography as="span" variant="label" className="registration-webview-eyebrow">
            {webviewKindLabel}
          </Typography>
          <Typography variant="h2" className="registration-webview-title">
            {embeddedWebview.title}
          </Typography>
          <Typography variant="bodySmall" muted className="registration-webview-url">
            {formatWebviewUrl(embeddedWebview.url)}
          </Typography>
        </div>

        {embeddedWebview.kind === "registration" && selectedHomeserver ? (
          <Button variant="secondary" onClick={onGoToLogin}>
            Go to log in
          </Button>
        ) : null}
      </div>

      <div
        ref={webviewHostRef}
        className="registration-webview-host"
        aria-label={`${embeddedWebview.title} webview`}
      />
    </Panel>
  );
}
