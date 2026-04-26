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

import { BackButton, Button, Card, FeedbackMessage, Pill, Typography } from "../../components/ui";
import type { FeedbackMessage as RegistrationFeedbackMessage, HomeserverDirectoryEntry } from "./registrationShared";
import {
  boolLabel,
  flowLabel,
  handoffWarning,
  homeserverCopy,
  homeserverHost,
  homeserverTitle,
} from "./registrationShared";
import "./HomeserverDetailsScreen.css";

type HomeserverDetailsScreenProps = {
  feedback: RegistrationFeedbackMessage | null;
  homeserver: HomeserverDirectoryEntry;
  isSubmitting: boolean;
  captchaWarningText: string;
  homepageUrl: string | null;
  rulesUrl: string | null;
  privacyUrl: string | null;
  onBack: () => void;
  onOpenPublishedLink: (url: string, title: string) => void;
  onOpenRegistrationForm: () => void;
  onContinueHomeserverFlow: () => void;
};

export function HomeserverDetailsScreen({
  feedback,
  homeserver,
  isSubmitting,
  captchaWarningText,
  homepageUrl,
  rulesUrl,
  privacyUrl,
  onBack,
  onOpenPublishedLink,
  onOpenRegistrationForm,
  onContinueHomeserverFlow,
}: HomeserverDetailsScreenProps) {
  return (
    <section
      className="registration-screen--narrow registration-screen--details"
      aria-labelledby="registration-details-title"
    >
      <div className="registration-heading-row">
        <BackButton onClick={onBack} />
        <Typography variant="h1" id="registration-details-title">
          {homeserverTitle(homeserver)}
        </Typography>
      </div>
      <Typography variant="body" muted className="registration-screen-copy">
        {homeserverCopy(homeserver)}
      </Typography>

      <div className="registration-detail-tags">
        {homeserver.is_official ? (
          <Pill tone="primary">Official</Pill>
        ) : null}
        <Pill tone="secondary">
          {flowLabel(homeserver.registration_flow)}
        </Pill>
        <Pill>
          {homeserver.homeserver_url ?? homeserverHost(homeserver)}
        </Pill>
        {homeserver.reg_method ? (
          <Pill>{homeserver.reg_method}</Pill>
        ) : null}
      </div>

      {feedback ? (
        <FeedbackMessage tone={feedback.tone}>
          {feedback.text}
        </FeedbackMessage>
      ) : null}

      {homeserver.registration_flow !== "matrix_sdk" ? (
        <FeedbackMessage tone="warning" className="registration-warning">
          {handoffWarning(homeserver, "external_flow")}
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
            {homepageUrl ? (
              <Button
                variant="ghost"
                className="registration-link-button"
                onClick={() =>
                  onOpenPublishedLink(
                    homepageUrl,
                    `${homeserverTitle(homeserver)} homepage`,
                  )
                }
              >
                Homepage
              </Button>
            ) : null}
            {rulesUrl ? (
              <Button
                variant="ghost"
                className="registration-link-button"
                onClick={() =>
                  onOpenPublishedLink(
                    rulesUrl,
                    `${homeserverTitle(homeserver)} rules`,
                  )
                }
              >
                Rules
              </Button>
            ) : null}
            {privacyUrl ? (
              <Button
                variant="ghost"
                className="registration-link-button"
                onClick={() =>
                  onOpenPublishedLink(
                    privacyUrl,
                    `${homeserverTitle(homeserver)} privacy policy`,
                  )
                }
              >
                Privacy policy
              </Button>
            ) : null}
            {!homepageUrl && !rulesUrl && !privacyUrl ? (
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
              <dd>{flowLabel(homeserver.registration_flow)}</dd>
            </div>
            <div>
              <dt>Email</dt>
              <dd>{boolLabel(homeserver.email, "Required")}</dd>
            </div>
            <div>
              <dt>Captcha</dt>
              <dd>{boolLabel(homeserver.captcha, "Required")}</dd>
            </div>
            <div>
              <dt>Display name</dt>
              <dd>{homeserver.supports_display_name ? "Supported" : "Not supported"}</dd>
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
                {homeserver.software
                  ? homeserver.version
                    ? `${homeserver.software} ${homeserver.version}`
                    : homeserver.software
                  : "Unknown"}
              </dd>
            </div>
            <div>
              <dt>Sliding sync</dt>
              <dd>{boolLabel(homeserver.sliding_sync)}</dd>
            </div>
            <div>
              <dt>IPv6</dt>
              <dd>{boolLabel(homeserver.ipv6)}</dd>
            </div>
            <div>
              <dt>Cloudflare</dt>
              <dd>{boolLabel(homeserver.cloudflare)}</dd>
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
              <dd>{homeserver.isp ?? "Unknown"}</dd>
            </div>
            <div>
              <dt>Staff jurisdiction</dt>
              <dd>{homeserver.staff_jur ?? "Unknown"}</dd>
            </div>
            <div>
              <dt>Languages</dt>
              <dd>
                {homeserver.languages.length > 0
                  ? homeserver.languages.join(", ")
                  : "Not listed"}
              </dd>
            </div>
            <div>
              <dt>Features</dt>
              <dd>
                {homeserver.features.length > 0
                  ? homeserver.features.join(", ")
                  : "No extra features listed"}
              </dd>
            </div>
          </dl>
        </Card>
      </div>

      {homeserver.reg_note ? (
        <FeedbackMessage tone="info" className="registration-note">
          {homeserver.reg_note}
        </FeedbackMessage>
      ) : null}

      <div className="registration-action-row">
        {homeserver.registration_flow === "matrix_sdk" ? (
          <Button variant="primary" onClick={onOpenRegistrationForm}>
            Continue to registration form
          </Button>
        ) : (
          <Button
            variant="primary"
            disabled={isSubmitting}
            onClick={onContinueHomeserverFlow}
          >
            {isSubmitting
              ? "Working..."
              : homeserver.registration_flow === "external_link"
                ? "Open registration in Hyperion"
                : "Continue with guidance"}
          </Button>
        )}
      </div>
    </section>
  );
}
