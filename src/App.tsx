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
import { useEffect, useState } from "react";
import { ScreenMain, ScreenShell, Typography } from "./components/ui";
import { AppShell, type AccountSummary } from "./screens/app-shell";
import { LogInScreen } from "./screens/Log-in";
import { RegistrationScreen } from "./screens/registration";

type EntryScreen = "login" | "registration";
type AppStage = "loading" | "unauthenticated" | "authenticated";
type LoginLaunchState = {
  homeserver?: string;
  text: string;
  tone: "error" | "success" | "info";
};

const ACTIVE_ACCOUNT_CACHE_KEY = "hyperion.activeAccountSummary";
const APP_BOOTSTRAP_FALLBACK_DELAY_MS = 1200;

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
    window.localStorage.setItem(ACTIVE_ACCOUNT_CACHE_KEY, JSON.stringify(nextAccount));
    return;
  }

  window.localStorage.removeItem(ACTIVE_ACCOUNT_CACHE_KEY);
}

function App() {
  const [activeAccount, setActiveAccount] = useState<AccountSummary | null>(() =>
    loadCachedActiveAccount(),
  );
  const [appStage, setAppStage] = useState<AppStage>(() =>
    loadCachedActiveAccount() ? "authenticated" : "loading",
  );
  const [activeScreen, setActiveScreen] = useState<EntryScreen>("login");
  const [loginLaunchState, setLoginLaunchState] = useState<LoginLaunchState | null>(null);

  useEffect(() => {
    persistCachedActiveAccount(activeAccount);
  }, [activeAccount]);

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
        const currentAccount = await invoke<AccountSummary | null>("active_account");
        if (cancelled) {
          return;
        }

        if (currentAccount) {
          setActiveAccount(currentAccount);
          setAppStage("authenticated");
          return;
        }

        setActiveAccount(null);
      } catch {
        // Fall through to the unauthenticated entry flow when the native layer
        // cannot restore an active account yet.
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
      setAppStage("authenticated");
      return;
    }

    setAppStage("unauthenticated");
    setActiveScreen("login");
    setLoginLaunchState(null);
  }

  if (appStage === "loading") {
    return (
      <ScreenShell>
        <ScreenMain largeBlockPadding>
          <Typography variant="body">Loading the application shell...</Typography>
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
      onAuthenticated={(nextAccount) => {
        setActiveAccount(nextAccount);
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
