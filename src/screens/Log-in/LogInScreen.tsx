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
import { ArrowRight, User } from "lucide-react";
import { type SyntheticEvent, useEffect, useState } from "react";
import {
  Button,
  FeedbackMessage,
  Panel,
  ScreenHeader,
  ScreenMain,
  ScreenShell,
  TextField,
  Typography,
} from "../../components/ui";
import "./LogInScreen.css";
import hyperionLogo from "../../../src-tauri/icons/128x128.png";

type FeedbackMessage = {
  tone: "error" | "success" | "info";
  text: string;
};

type AccountSummary = {
  account_key: string;
  user_id: string;
  homeserver_url: string;
  is_active: boolean;
};

type FormValues = {
  username: string;
  homeserver: string;
  password: string;
};

type LogInScreenProps = {
  initialFeedback?: FeedbackMessage | null;
  initialHomeserver?: string;
  onOpenRegistration?: () => void;
};

const DEFAULT_HOMESERVER = "https://matrix.org";
const ACTIVE_SESSION_POLL_INTERVAL_MS = 5000;
const navigationItems = ["Home", "Info", "Terms", "Contact"];

const defaultFormValues: FormValues = {
  username: "",
  homeserver: "",
  password: "",
};

function sortAccounts(accounts: AccountSummary[]): AccountSummary[] {
  return [...accounts].sort((left, right) => {
    if (left.is_active !== right.is_active) {
      return left.is_active ? -1 : 1;
    }

    return left.user_id.localeCompare(right.user_id);
  });
}

function upsertAccount(
  accounts: AccountSummary[],
  nextAccount: AccountSummary,
): AccountSummary[] {
  return sortAccounts([
    ...accounts.filter((account) => account.account_key !== nextAccount.account_key),
    nextAccount,
  ]);
}

function markActiveAccount(
  accounts: AccountSummary[],
  activeAccountKey: string,
): AccountSummary[] {
  return sortAccounts(
    accounts.map((account) => ({
      ...account,
      is_active: account.account_key === activeAccountKey,
    })),
  );
}

function getErrorMessage(error: unknown): string {
  if (typeof error === "string" && error.trim().length > 0) {
    return error;
  }

  if (error instanceof Error && error.message.trim().length > 0) {
    return error.message;
  }

  return "Something went wrong while contacting the account service.";
}

export default function LogInScreen({
  initialFeedback = null,
  initialHomeserver = "",
  onOpenRegistration,
}: LogInScreenProps) {
  const [formValues, setFormValues] = useState<FormValues>(() => ({
    ...defaultFormValues,
    homeserver: initialHomeserver,
  }));
  const [accounts, setAccounts] = useState<AccountSummary[]>([]);
  const [activeAccount, setActiveAccount] = useState<AccountSummary | null>(null);
  const [feedback, setFeedback] = useState<FeedbackMessage | null>(initialFeedback);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [switchingAccountKey, setSwitchingAccountKey] = useState<string | null>(null);
  const [validationRequested, setValidationRequested] = useState(false);
  const [isBootstrapping, setIsBootstrapping] = useState(true);

  async function syncSessionState(): Promise<boolean> {
    try {
      const [knownAccounts, currentAccount] = await Promise.all([
        invoke<AccountSummary[]>("list_accounts"),
        invoke<AccountSummary | null>("active_account"),
      ]);

      const nextAccounts = currentAccount
        ? markActiveAccount(
            upsertAccount(knownAccounts, currentAccount),
            currentAccount.account_key,
          )
        : sortAccounts(knownAccounts);

      setAccounts(nextAccounts);
      setActiveAccount(currentAccount);
      return true;
    } catch {
      return false;
    } finally {
      setIsBootstrapping(false);
    }
  }

  useEffect(() => {
    void syncSessionState();
  }, []);

  useEffect(() => {
    if (initialFeedback) {
      setFeedback(initialFeedback);
    }
  }, [initialFeedback]);

  useEffect(() => {
    if (initialHomeserver.length === 0) {
      return;
    }

    setFormValues((currentValues) => ({
      ...currentValues,
      homeserver: currentValues.homeserver.trim().length > 0
        ? currentValues.homeserver
        : initialHomeserver,
    }));
  }, [initialHomeserver]);

  const usernameMissing = validationRequested && formValues.username.trim().length === 0;
  const passwordMissing = validationRequested && formValues.password.length === 0;
  const currentAccount = activeAccount ?? accounts.find((account) => account.is_active) ?? null;

  useEffect(() => {
    if (!currentAccount) {
      return;
    }

    const currentAccountUserId = currentAccount.user_id;
    let cancelled = false;

    async function validateActiveSession() {
      try {
        const validatedAccount =
          await invoke<AccountSummary | null>("validate_active_account");

        if (cancelled) {
          return;
        }

        if (validatedAccount) {
          return;
        }

        const didSync = await syncSessionState();
        if (cancelled) {
          return;
        }

        setFeedback({
          tone: didSync ? "info" : "error",
          text: didSync
            ? `${currentAccountUserId} was deauthorized and has been logged out locally.`
            : `The active session for ${currentAccountUserId} is no longer valid.`,
        });
      } catch {
        // Validation failures should not interrupt the current UI flow.
      }
    }

    void validateActiveSession();

    const intervalId = window.setInterval(() => {
      void validateActiveSession();
    }, ACTIVE_SESSION_POLL_INTERVAL_MS);

    const handleWindowFocus = () => {
      void validateActiveSession();
    };

    const handleVisibilityChange = () => {
      if (document.visibilityState === "visible") {
        void validateActiveSession();
      }
    };

    window.addEventListener("focus", handleWindowFocus);
    document.addEventListener("visibilitychange", handleVisibilityChange);

    return () => {
      cancelled = true;
      window.clearInterval(intervalId);
      window.removeEventListener("focus", handleWindowFocus);
      document.removeEventListener("visibilitychange", handleVisibilityChange);
    };
  }, [currentAccount?.account_key]);

  function updateField(field: keyof FormValues, value: string) {
    setFormValues((currentValues) => ({
      ...currentValues,
      [field]: value,
    }));
  }

  async function handleSubmit(event: SyntheticEvent<HTMLFormElement, SubmitEvent>) {
    event.preventDefault();
    setValidationRequested(true);

    if (formValues.username.trim().length === 0 || formValues.password.length === 0) {
      setFeedback({
        tone: "error",
        text: "Username and password are required before you can log in.",
      });
      return;
    }

    setIsSubmitting(true);
    setFeedback(null);

    try {
      const account = await invoke<AccountSummary>("login_account", {
        request: {
          homeserver_url:
            formValues.homeserver.trim().length > 0
              ? formValues.homeserver.trim()
              : DEFAULT_HOMESERVER,
          username: formValues.username.trim(),
          password: formValues.password,
        },
      });

      setAccounts((currentAccounts) => upsertAccount(currentAccounts, account));
      if (account.is_active) {
        setActiveAccount(account);
      }

      setFormValues((currentValues) => ({
        ...currentValues,
        password: "",
      }));
      setValidationRequested(false);

      const didSync = await syncSessionState();
      setFeedback({
        tone: didSync ? "success" : "info",
        text: didSync
          ? `Signed in as ${account.user_id}.`
          : `Signed in as ${account.user_id}. The local session list will refresh when the native layer responds.`,
      });
    } catch (error) {
      setFeedback({
        tone: "error",
        text: getErrorMessage(error),
      });
    } finally {
      setIsSubmitting(false);
    }
  }

  async function handleSwitchAccount(account: AccountSummary) {
    setSwitchingAccountKey(account.account_key);
    setFeedback(null);

    try {
      await invoke("switch_active_account", {
        accountKey: account.account_key,
      });

      const nextActiveAccount = {
        ...account,
        is_active: true,
      };

      setAccounts((currentAccounts) =>
        markActiveAccount(currentAccounts, account.account_key),
      );
      setActiveAccount(nextActiveAccount);

      await syncSessionState();

      setFeedback({
        tone: "success",
        text: `Switched to ${account.user_id}.`,
      });
    } catch (error) {
      setFeedback({
        tone: "error",
        text: getErrorMessage(error),
      });
    } finally {
      setSwitchingAccountKey(null);
    }
  }

  return (
    <ScreenShell>
      <ScreenHeader className="login-topbar" wide>
        <div className="login-brand">
          <img src={hyperionLogo} alt="Hyperion logo" className="login-brand-logo" />
          <div className="login-brand-text">
            <span className="login-brand-name">Hyperion</span>
            <span className="login-brand-tag">Matrix client</span>
          </div>
        </div>

        <nav className="login-nav" aria-label="Project links">
          {navigationItems.map((item) => (
            <Button key={item} variant="ghost" className="login-nav-button">
              {item}
            </Button>
          ))}
        </nav>

        <Button
          variant="secondary"
          className="login-signup-button"
          disabled={!onOpenRegistration}
          onClick={() => onOpenRegistration?.()}
        >
          Sign up
        </Button>
      </ScreenHeader>

      <ScreenMain className="login-stage" centered largeBlockPadding wide>
        <Panel className="login-panel" aria-labelledby="login-panel-title">
          <div className="login-avatar">
            <User aria-hidden="true" />
          </div>

          <Typography variant="eyebrow" className="login-panel-kicker">
            Account access
          </Typography>
          <Typography as="h2" variant="h1" className="login-panel-title" id="login-panel-title">
            Log in
          </Typography>
          <Typography variant="body" muted className="login-panel-copy">
            Username and password are required. Homeserver stays optional and falls
            back to the standard Matrix endpoint when left blank.
          </Typography>

          <form className="login-form" noValidate onSubmit={handleSubmit}>
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

            <TextField
              autoCapitalize="none"
              inputMode="url"
              label="Homeserver"
              name="homeserver"
              onChange={(event) => updateField("homeserver", event.currentTarget.value)}
              placeholder={DEFAULT_HOMESERVER}
              spellCheck={false}
              type="text"
              value={formValues.homeserver}
            />

            <div className="login-password-field">
              <TextField
                autoComplete="current-password"
                enterKeyHint="go"
                isInvalid={passwordMissing}
                isRequiredVisible
                label="Password"
                name="password"
                onChange={(event) => updateField("password", event.currentTarget.value)}
                type="password"
                value={formValues.password}
              />

              <div className="login-password-row">
                <Button
                  aria-label={isSubmitting ? "Logging in" : "Log in"}
                  iconOnly
                  className="login-submit-button"
                  disabled={isSubmitting}
                  type="submit"
                  variant="icon"
                >
                  <ArrowRight aria-hidden="true" />
                  <span className="login-submit-label">
                    {isSubmitting ? "Connecting..." : "Log in"}
                  </span>
                </Button>
              </div>
            </div>

            <div className="login-support-row">
              <Button variant="ghost" className="login-text-action">
                Forgot password?
              </Button>

              <span className="login-required-copy">
                <span className="ui-required-marker" aria-hidden="true">
                  *
                </span>{" "}
                Required fields
              </span>
            </div>
          </form>

          {feedback ? (
            <FeedbackMessage tone={feedback.tone} aria-live="polite">
              {feedback.text}
            </FeedbackMessage>
          ) : null}

          <section className="login-session-panel" aria-labelledby="session-panel-title">
            <div className="login-session-head">
              <Typography as="h3" variant="h3" id="session-panel-title">
                Local sessions
              </Typography>
              <span className="login-section-meta">
                {isBootstrapping
                  ? "Loading..."
                  : `${accounts.length} ${accounts.length === 1 ? "known account" : "known accounts"}`}
              </span>
            </div>

            {currentAccount ? (
              <div className="login-active-card">
                <span className="login-active-label">Active now</span>
                <span className="login-active-user">{currentAccount.user_id}</span>
                <span className="login-active-home">{currentAccount.homeserver_url}</span>
              </div>
            ) : (
              <p className="login-session-empty">
                Your first successful login will appear here and can be switched back
                to later.
              </p>
            )}

            {accounts.length > 0 ? (
              <div className="login-account-list">
                {accounts.map((account) => (
                  <Button
                    key={account.account_key}
                    variant="secondary"
                    className={`login-account-button${
                      account.is_active ? " login-account-button--active" : ""
                    }`}
                    disabled={
                      isSubmitting ||
                      switchingAccountKey !== null ||
                      account.is_active
                    }
                    onClick={() => void handleSwitchAccount(account)}
                  >
                    <span className="login-account-copy">
                      <span className="login-account-user">{account.user_id}</span>
                      <span className="login-account-home">{account.homeserver_url}</span>
                    </span>
                    <span className="login-account-state">
                      {account.is_active
                        ? "Active"
                        : switchingAccountKey === account.account_key
                          ? "Switching"
                          : "Use"}
                    </span>
                  </Button>
                ))}
              </div>
            ) : null}
          </section>
        </Panel>
      </ScreenMain>
    </ScreenShell>
  );
}
