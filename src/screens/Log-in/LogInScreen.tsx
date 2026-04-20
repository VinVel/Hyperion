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
  ScreenMain,
  ScreenShell,
  TextField,
  Typography,
} from "../../components/ui";
import type { AccountSummary } from "../app-shell";
import "./LogInScreen.css";

type FeedbackMessage = {
  tone: "error" | "success" | "info";
  text: string;
};

type FormValues = {
  username: string;
  homeserver: string;
  password: string;
};

type LogInScreenProps = {
  initialFeedback?: FeedbackMessage | null;
  initialHomeserver?: string;
  onAuthenticated?: (account: AccountSummary) => void;
  onOpenRegistration?: () => void;
};

const DEFAULT_HOMESERVER = "https://matrix.org";

const defaultFormValues: FormValues = {
  username: "",
  homeserver: "",
  password: "",
};

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
  onAuthenticated,
  onOpenRegistration,
}: LogInScreenProps) {
  const [formValues, setFormValues] = useState<FormValues>(() => ({
    ...defaultFormValues,
    homeserver: initialHomeserver,
  }));
  const [feedback, setFeedback] = useState<FeedbackMessage | null>(initialFeedback);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [validationRequested, setValidationRequested] = useState(false);

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

      setFormValues((currentValues) => ({
        ...currentValues,
        password: "",
      }));
      setValidationRequested(false);
      onAuthenticated?.(account);
    } catch (error) {
      setFeedback({
        tone: "error",
        text: getErrorMessage(error),
      });
    } finally {
      setIsSubmitting(false);
    }
  }

  return (
    <ScreenShell>
      <ScreenMain className="login-stage" largeBlockPadding wide>
        <section className="login-panel" aria-labelledby="login-panel-title">
          <div className="login-avatar">
            <User aria-hidden="true" />
          </div>

          <Typography variant="eyebrow" className="login-panel-kicker">
            Account access
          </Typography>
          <Typography as="h2" variant="h1" className="login-panel-title" id="login-panel-title">
            Log in
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

            <div className="login-registration-row">
              <Typography variant="bodySmall" muted className="login-registration-copy">
                Need a new account?
              </Typography>

              <Button
                variant="secondary"
                className="login-signup-button"
                disabled={!onOpenRegistration}
                onClick={() => onOpenRegistration?.()}
                type="button"
              >
                Sign up
              </Button>
            </div>
          </form>

          <div className="login-feedback-slot" aria-live="polite">
            {feedback ? (
              <FeedbackMessage tone={feedback.tone}>
                {feedback.text}
              </FeedbackMessage>
            ) : null}
          </div>
        </section>
      </ScreenMain>
    </ScreenShell>
  );
}
