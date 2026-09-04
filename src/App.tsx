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

import "./App.css";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import {
  notifyFeedback,
  Button,
  ScreenMain,
  ScreenShell,
  Typography,
  useFeedbackToast,
} from "./components/ui";
import { AppShell, type AccountSummary } from "./screens/app-shell";
import { LogInScreen } from "./screens/Log-in";
import { RegistrationScreen } from "./screens/registration";

type EntryScreen = "login" | "registration";
type AppStage =
  "loading" | "unauthenticated" | "authenticated" | "storage_recovery";
type LoginLaunchState = {
  homeserver?: string;
  username?: string;
  text: string;
  tone: "error" | "success" | "info";
};

const ACTIVE_ACCOUNT_CACHE_KEY = "hyperion.activeAccountSummary";
const APP_BOOTSTRAP_FALLBACK_DELAY_MS = 1200;
const SHELL_SESSION_DEAUTHORIZED_EVENT = "hyperion://session-deauthorized";
const SHELL_SESSION_REAUTHENTICATION_REQUIRED_EVENT =
  "hyperion://session-reauthentication-required";
const SESSION_VERIFICATION_REQUEST_RECEIVED_EVENT =
  "hyperion://session-verification-request-received";

type SessionDeauthorizedPayload = {
  account_key: string;
};
type SessionReauthenticationRequiredPayload = {
  account_key: string;
  state: "reauthentication_required";
  reason: string;
};

type IncomingSessionVerification = {
  account_key: string;
  flow_id: string;
  device_id: string;
  event_kind: "request" | "start";
  supported_methods: string[];
};

type Feedback = {
  tone: "error" | "success" | "info" | "warning";
  text: string;
};

function messageFromError(error: unknown): string {
  if (typeof error === "string" && error.trim().length > 0) {
    return error;
  }

  if (error instanceof Error && error.message.trim().length > 0) {
    return error.message;
  }

  return "Verification request could not be updated.";
}

function loadCachedActiveAccount(): AccountSummary | null {
  if (typeof window === "undefined") {
    return null;
  }

  const cachedValue = window.localStorage.getItem(ACTIVE_ACCOUNT_CACHE_KEY);
  if (!cachedValue) {
    return null;
  }

  try {
    return JSON.parse(cachedValue) as AccountSummary;
  } catch {
    window.localStorage.removeItem(ACTIVE_ACCOUNT_CACHE_KEY);
    return null;
  }
}

function persistCachedActiveAccount(nextAccount: AccountSummary | null) {
  if (typeof window === "undefined") {
    return;
  }

  if (nextAccount) {
    window.localStorage.setItem(
      ACTIVE_ACCOUNT_CACHE_KEY,
      JSON.stringify(nextAccount),
    );
    return;
  }

  window.localStorage.removeItem(ACTIVE_ACCOUNT_CACHE_KEY);
}

function App() {
  const [activeAccount, setActiveAccount] = useState<AccountSummary | null>(
    () => loadCachedActiveAccount(),
  );
  const [appStage, setAppStage] = useState<AppStage>(() =>
    loadCachedActiveAccount() ? "authenticated" : "loading",
  );
  const [activeScreen, setActiveScreen] = useState<EntryScreen>("login");
  const [loginLaunchState, setLoginLaunchState] =
    useState<LoginLaunchState | null>(null);
  const [verificationFeedback, setVerificationFeedback] =
    useState<Feedback | null>(null);
  useFeedbackToast(verificationFeedback);
  const [readOnlyAccountKey, setReadOnlyAccountKey] = useState<string | null>(
    null,
  );

  useEffect(() => {
    persistCachedActiveAccount(activeAccount);
  }, [activeAccount]);

  useEffect(() => {
    let cancelled = false;
    const unlistenPromise = listen<SessionDeauthorizedPayload>(
      SHELL_SESSION_DEAUTHORIZED_EVENT,
      (event) => {
        if (cancelled) {
          return;
        }

        setActiveAccount((currentAccount) => {
          if (currentAccount?.account_key !== event.payload.account_key) {
            return currentAccount;
          }

          setAppStage("unauthenticated");
          setActiveScreen("login");
          setLoginLaunchState({
            tone: "error",
            text: "This Matrix session was deauthorized by the server.",
          });
          return null;
        });
      },
    );

    return () => {
      cancelled = true;
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    const unlistenPromise = listen<SessionReauthenticationRequiredPayload>(
      SHELL_SESSION_REAUTHENTICATION_REQUIRED_EVENT,
      (event) => {
        if (!cancelled) {
          setReadOnlyAccountKey(event.payload.account_key);
        }
      },
    );
    return () => {
      cancelled = true;
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, []);

  useEffect(() => {
    if (appStage !== "authenticated" || !activeAccount) {
      return;
    }

    const activeAccountKey = activeAccount.account_key;
    let cancelled = false;

    async function respondToVerificationRequest(
      flowId: string,
      command:
        | "accept_session_verification_request"
        | "deny_session_verification_request",
    ) {
      await invoke(command, {
        request: { flow_id: flowId },
      });
    }

    const unlistenPromise = listen<IncomingSessionVerification>(
      SESSION_VERIFICATION_REQUEST_RECEIVED_EVENT,
      (event) => {
        if (cancelled) {
          return;
        }

        if (event.payload.account_key !== activeAccountKey) {
          return;
        }

        if (event.payload.event_kind !== "request") {
          return;
        }

        notifyFeedback({
          tone: "info",
          text: `Incoming verification request from ${event.payload.device_id}.`,
          actions: [
            {
              label: "Accept",
              variant: "primary",
              onSelect: async () => {
                if (cancelled) {
                  return;
                }

                try {
                  await respondToVerificationRequest(
                    event.payload.flow_id,
                    "accept_session_verification_request",
                  );
                } catch (error) {
                  setVerificationFeedback({
                    tone: "error",
                    text: messageFromError(error),
                  });
                }
              },
            },
            {
              label: "Deny",
              variant: "destructive",
              onSelect: async () => {
                if (cancelled) {
                  return;
                }

                try {
                  await respondToVerificationRequest(
                    event.payload.flow_id,
                    "deny_session_verification_request",
                  );
                } catch (error) {
                  setVerificationFeedback({
                    tone: "error",
                    text: messageFromError(error),
                  });
                }
              },
            },
          ],
        });
      },
    );

    return () => {
      cancelled = true;
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, [activeAccount, appStage]);

  useEffect(() => {
    let cancelled = false;
    const fallbackTimer = window.setTimeout(() => {
      if (!cancelled) {
        setAppStage((currentStage) =>
          currentStage === "loading" ? "unauthenticated" : currentStage,
        );
      }
    }, APP_BOOTSTRAP_FALLBACK_DELAY_MS);

    async function bootstrapAuthenticatedState() {
      try {
        const currentAccount = await invoke<AccountSummary | null>(
          "active_account",
        );
        if (cancelled) {
          return;
        }

        if (currentAccount) {
          setActiveAccount(currentAccount);
          setAppStage("authenticated");
          return;
        }

        if (activeAccount) {
          setAppStage("authenticated");
          return;
        }

        setActiveAccount(null);
      } catch (error) {
        if (
          typeof error === "string" &&
          error.startsWith("secure_storage_unavailable:")
        ) {
          setActiveAccount(null);
          setAppStage("storage_recovery");
          return;
        }
        // Transport failures remain ordinary offline startup: a locally restored
        // account may still expose its cached history.
        if (!cancelled && activeAccount) {
          setAppStage("authenticated");
          return;
        }
      }

      if (!cancelled) {
        setAppStage("unauthenticated");
      }
    }

    void bootstrapAuthenticatedState();

    return () => {
      cancelled = true;
      window.clearTimeout(fallbackTimer);
    };
  }, []);

  function openAccountEntryFlow() {
    setAppStage("unauthenticated");
    setActiveScreen("login");
    setLoginLaunchState(null);
  }

  function handleSessionStateChange(nextAccount: AccountSummary | null) {
    setActiveAccount(nextAccount);

    if (nextAccount) {
      setReadOnlyAccountKey(null);
      setAppStage("authenticated");
      return;
    }

    setAppStage("unauthenticated");
    setActiveScreen("login");
    setLoginLaunchState(null);
  }

  async function retrySecureStorageRestore() {
    setAppStage("loading");
    try {
      const account = await invoke<AccountSummary | null>("active_account");
      if (account) {
        setActiveAccount(account);
        setAppStage("authenticated");
        return;
      }
      setAppStage("unauthenticated");
    } catch {
      setAppStage("storage_recovery");
    }
  }

  if (appStage === "loading") {
    return (
      <ScreenShell>
        <ScreenMain largeBlockPadding>
          <Typography variant="body">
            Loading the application shell...
          </Typography>
        </ScreenMain>
      </ScreenShell>
    );
  }

  if (appStage === "storage_recovery") {
    return (
      <ScreenShell>
        <ScreenMain centered largeBlockPadding>
          <Typography as="h1" variant="h1">
            Secure storage is unavailable
          </Typography>
          <Typography variant="body">
            Hyperion cannot unlock this device’s encrypted account data. Start
            or unlock your desktop Secret Service, then try again.
          </Typography>
          <Button onClick={() => void retrySecureStorageRestore()}>
            Try again
          </Button>
          <Button variant="ghost" onClick={openAccountEntryFlow}>
            Return to sign in
          </Button>
        </ScreenMain>
      </ScreenShell>
    );
  }

  if (appStage === "authenticated" && activeAccount) {
    return (
      <AppShell
        activeAccount={activeAccount}
        onAddAccount={openAccountEntryFlow}
        onActiveAccountChange={setActiveAccount}
        onSignedOut={handleSessionStateChange}
        isReadOnly={readOnlyAccountKey === activeAccount.account_key}
        onReauthenticate={() => {
          setAppStage("unauthenticated");
          setActiveScreen("login");
          setLoginLaunchState({
            tone: "info",
            homeserver: activeAccount.homeserver_url,
            username: activeAccount.user_id,
            text: "Please sign in again to resume syncing.",
          });
        }}
      />
    );
  }

  if (activeScreen === "registration") {
    return (
      <RegistrationScreen
        onBackToLogin={(nextLaunchState) => {
          setAppStage("unauthenticated");
          setLoginLaunchState(nextLaunchState ?? null);
          setActiveScreen("login");
        }}
      />
    );
  }

  return (
    <LogInScreen
      initialFeedback={loginLaunchState}
      initialHomeserver={loginLaunchState?.homeserver}
      initialUsername={loginLaunchState?.username}
      onAuthenticated={(nextAccount) => {
        setActiveAccount(nextAccount);
        setReadOnlyAccountKey(null);
        setAppStage("authenticated");
      }}
      onBackToApp={
        activeAccount
          ? () => {
              setAppStage("authenticated");
            }
          : undefined
      }
      onOpenRegistration={() => {
        setLoginLaunchState(null);
        setActiveScreen("registration");
      }}
    />
  );
}

export default App;
