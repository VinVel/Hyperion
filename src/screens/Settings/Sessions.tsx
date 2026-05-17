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
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  ChevronDown,
  RefreshCw,
  ShieldAlert,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import {
  Button,
  Card,
  FeedbackMessage,
  ScrollArea,
  TextField,
  Typography,
  useFeedbackToast,
} from "../../components/ui";

type AccountSummary = {
  account_key: string;
  user_id: string;
};

type SessionsProps = {
  activeAccount: AccountSummary;
};

type SessionInfo = {
  device_id: string;
  display_name: string | null;
  trust: {
    verified: boolean;
    verified_with_cross_signing: boolean;
    can_verify_current_session: boolean;
  };
  current: boolean;
  last_seen_ip: string | null;
  last_seen_ts_unix_ms: number | null;
};

type SessionOverview = {
  has_active_account: boolean;
  account_key: string | null;
  user_id: string | null;
  current_device_id: string | null;
  current_session_verified: boolean;
  sessions: SessionInfo[];
  last_refreshed_at_unix_ms?: number | null;
};

type VerificationState = {
  flow_id: string;
  label: string;
  done: boolean;
  cancelled: boolean;
  cancel_reason: string | null;
};

type VerificationStart = {
  flow_id: string;
  device_id: string;
  supported_methods: string[];
  state: VerificationState;
};

type IncomingSessionVerification = {
  account_key: string;
  flow_id: string;
  device_id: string;
  event_kind: "request" | "start";
  supported_methods: string[];
};

type SasEmoji = {
  symbol: string;
  description: string;
};

type SasVerificationView = {
  flow_id: string;
  label: string;
  done: boolean;
  cancelled: boolean;
  cancel_reason: string | null;
  emojis: SasEmoji[];
  decimals: [number, number, number] | null;
  can_be_presented: boolean;
};

type DeauthorizeSessionsOutcome =
  | { kind: "completed" }
  | { kind: "password_required"; auth_session: string | null }
  | { kind: "account_management_required"; account_management_url: string };

type Feedback = {
  tone: "success" | "info" | "warning";
  text: string;
};

type PendingDeauthorization = {
  deviceIds: string[];
  authSession: string | null;
};

const sessionOverviewUpdatedEvent = "hyperion://session-overview-updated";
const sessionVerificationRequestReceivedEvent =
  "hyperion://session-verification-request-received";

// Keeps the verification popup close to live Matrix to-device state without making users refresh manually.
const verificationPollIntervalMs = 2000;

function defaultOverview(activeAccount: AccountSummary): SessionOverview {
  return {
    has_active_account: true,
    account_key: activeAccount.account_key,
    user_id: activeAccount.user_id,
    current_device_id: null,
    current_session_verified: false,
    sessions: [],
    last_refreshed_at_unix_ms: null,
  };
}

function messageFromError(error: unknown): string {
  if (typeof error === "string" && error.trim().length > 0) {
    return error;
  }

  if (error instanceof Error && error.message.trim().length > 0) {
    return error.message;
  }

  return "Session settings could not be updated.";
}

function displayName(session: SessionInfo): string {
  return session.display_name?.trim() || "Unnamed session";
}

function lastSeenLabel(timestamp: number | null): string {
  if (!timestamp) {
    return "Unknown";
  }

  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(timestamp));
}

function verificationFlowIsGone(error: unknown): boolean {
  const message = messageFromError(error).toLowerCase();
  return (
    message.includes("verification request is no longer available") ||
    message.includes("verification flow is no longer available") ||
    message.includes("emoji verification flow is not available")
  );
}

function pendingSasVerification(
  flowId: string,
  label: string,
): SasVerificationView {
  return {
    flow_id: flowId,
    label,
    done: false,
    cancelled: false,
    cancel_reason: null,
    emojis: [],
    decimals: null,
    can_be_presented: false,
  };
}

export default function Sessions({ activeAccount }: SessionsProps) {
  const [overview, setOverview] = useState<SessionOverview>(() =>
    defaultOverview(activeAccount),
  );
  const [selectedDeviceIds, setSelectedDeviceIds] = useState<string[]>([]);
  const [expandedDeviceId, setExpandedDeviceId] = useState<string | null>(null);
  const [verification, setVerification] = useState<VerificationStart | null>(
    null,
  );
  const [sasVerification, setSasVerification] =
    useState<SasVerificationView | null>(null);
  const [isSasOverlayOpen, setIsSasOverlayOpen] = useState(false);
  const [pendingAction, setPendingAction] = useState<string | null>(null);
  const [pendingDeauthorization, setPendingDeauthorization] =
    useState<PendingDeauthorization | null>(null);
  const [deauthorizationPassword, setDeauthorizationPassword] = useState("");
  const [message, setMessage] = useState<Feedback | null>(null);
  const [error, setError] = useState<string | null>(null);
  useFeedbackToast(error ? { tone: "error", text: error } : null);
  useFeedbackToast(message);

  const selectableSessions = useMemo(
    () => overview.sessions.filter((session) => !session.current),
    [overview.sessions],
  );
  const selectedSessions = selectableSessions.filter((session) =>
    selectedDeviceIds.includes(session.device_id),
  );
  const currentSession = overview.sessions.find((session) => session.current);
  const currentSessionVerifiers = overview.sessions.filter(
    (session) => session.trust.can_verify_current_session,
  );
  let currentSessionInstruction =
    "This session is not verified, and no verified session is available to verify it.";
  if (overview.current_session_verified) {
    currentSessionInstruction =
      "This session is verified. Use a session dropdown above to verify another session.";
  } else if (currentSessionVerifiers.length > 0) {
    currentSessionInstruction =
      "This session is not verified. Send a verification request to your verified sessions.";
  }
  const isBusy = pendingAction !== null;

  async function refreshOverview() {
    const nextOverview = await invoke<SessionOverview>("get_session_overview");
    if (!nextOverview.has_active_account) {
      return;
    }

    setOverview(nextOverview);
    setSelectedDeviceIds((current) =>
      current.filter((deviceId) =>
        nextOverview.sessions.some(
          (session) => session.device_id === deviceId && !session.current,
        ),
      ),
    );
  }

  useEffect(() => {
    setOverview(defaultOverview(activeAccount));
    setSelectedDeviceIds([]);
    setExpandedDeviceId(null);
    setVerification(null);
    setSasVerification(null);
    setIsSasOverlayOpen(false);
    void refreshOverview().catch((loadError) => {
      setError(messageFromError(loadError));
    });
  }, [activeAccount.account_key]);

  useEffect(() => {
    let cancelled = false;
    const unlistenPromise = listen<SessionOverview>(
      sessionOverviewUpdatedEvent,
      (event) => {
        if (cancelled) {
          return;
        }

        if (event.payload.account_key !== activeAccount.account_key) {
          return;
        }

        setOverview(event.payload);
      },
    );

    return () => {
      cancelled = true;
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, [activeAccount.account_key]);

  useEffect(() => {
    let cancelled = false;
    const unlistenPromise = listen<IncomingSessionVerification>(
      sessionVerificationRequestReceivedEvent,
      (event) => {
        if (cancelled) {
          return;
        }

        if (event.payload.account_key !== activeAccount.account_key) {
          return;
        }

        const incomingVerification: VerificationStart = {
          flow_id: event.payload.flow_id,
          device_id: event.payload.device_id,
          supported_methods: event.payload.supported_methods,
          state: {
            flow_id: event.payload.flow_id,
            label: "Requested",
            done: false,
            cancelled: false,
            cancel_reason: null,
          },
        };

        setVerification(incomingVerification);
        if (event.payload.event_kind === "start") {
          setSasVerification(
            pendingSasVerification(
              event.payload.flow_id,
              "Verification started",
            ),
          );
          setIsSasOverlayOpen(true);
          setMessage({
            tone: "info",
            text: "Emoji verification was started from another session.",
          });
          return;
        }

        setSasVerification(null);
        setIsSasOverlayOpen(false);
      },
    );

    return () => {
      cancelled = true;
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, [activeAccount.account_key]);

  useEffect(() => {
    if (!isSasOverlayOpen || !verification || sasVerification?.done) {
      return;
    }

    let cancelled = false;
    const pollVerification = async () => {
      try {
        const result = await invoke<SasVerificationView>(
          "start_sas_verification",
          {
            request: { flow_id: verification.flow_id },
          },
        );
        if (cancelled) {
          return;
        }

        setSasVerification(result);
        if (result.done) {
          await refreshOverview();
        }
      } catch (pollError) {
        if (verificationFlowIsGone(pollError)) {
          await refreshOverview();
          if (!cancelled) {
            setIsSasOverlayOpen(false);
            setMessage({
              tone: "success",
              text: "Verification finished. Session state was refreshed.",
            });
          }
          return;
        }

        if (!cancelled) {
          setError(messageFromError(pollError));
        }
      }
    };

    void pollVerification();
    const intervalId = window.setInterval(
      () => void pollVerification(),
      verificationPollIntervalMs,
    );

    return () => {
      cancelled = true;
      window.clearInterval(intervalId);
    };
  }, [isSasOverlayOpen, verification?.flow_id, sasVerification?.done]);

  async function runAction(actionName: string, action: () => Promise<void>) {
    if (pendingAction) {
      return;
    }

    setPendingAction(actionName);
    setError(null);
    setMessage(null);

    try {
      await action();
    } catch (actionError) {
      setError(messageFromError(actionError));
    } finally {
      setPendingAction(null);
    }
  }

  function toggleSelection(deviceId: string) {
    setSelectedDeviceIds((current) => {
      if (current.includes(deviceId)) {
        return current.filter(
          (selectedDeviceId) => selectedDeviceId !== deviceId,
        );
      }

      return [...current, deviceId];
    });
  }

  async function startVerification(session: SessionInfo) {
    const result = await invoke<VerificationStart>(
      "start_session_verification",
      {
        request: { device_id: session.device_id },
      },
    );
    setVerification(result);
    setSasVerification(
      pendingSasVerification(result.flow_id, "Waiting for other session"),
    );
    setIsSasOverlayOpen(true);
    setMessage({
      tone: "info",
      text: `Open ${displayName(session)} to accept the verification request.`,
    });
  }

  async function startCurrentSessionVerification() {
    const result = await invoke<VerificationStart>(
      "start_current_session_verification",
    );
    setVerification(result);
    setSasVerification(
      pendingSasVerification(result.flow_id, "Waiting for another session"),
    );
    setIsSasOverlayOpen(true);
    setMessage({
      tone: "info",
      text: "Open a verified session to accept this verification request.",
    });
  }

  async function refreshEmojiVerification() {
    if (!verification) {
      throw new Error("Start a session verification first.");
    }

    const result = await invoke<SasVerificationView>("start_sas_verification", {
      request: { flow_id: verification.flow_id },
    });
    setSasVerification(result);
    if (result.done) {
      await refreshOverview();
    }
  }

  async function confirmEmojiVerification() {
    if (!verification) {
      throw new Error("Start a session verification first.");
    }

    let result: SasVerificationView;
    try {
      result = await invoke<SasVerificationView>("confirm_sas_verification", {
        request: { flow_id: verification.flow_id },
      });
    } catch (confirmError) {
      if (!verificationFlowIsGone(confirmError)) {
        throw confirmError;
      }

      setIsSasOverlayOpen(false);
      setMessage({
        tone: "success",
        text: "Verification finished. Session state was refreshed.",
      });
      await refreshOverview();
      return;
    }

    setSasVerification(result);
    setMessage({
      tone: result.done ? "success" : "info",
      text: `Emoji verification state: ${result.label}.`,
    });
    await refreshOverview();
  }

  async function mismatchEmojiVerification() {
    if (!verification) {
      throw new Error("Start a session verification first.");
    }

    const result = await invoke<SasVerificationView>(
      "cancel_sas_verification",
      {
        request: { flow_id: verification.flow_id },
      },
    );
    setSasVerification(result);
    setMessage({
      tone: "warning",
      text: "Emoji verification was cancelled.",
    });
  }

  async function deauthorize(deviceIds: string[], password?: string) {
    const result = await invoke<DeauthorizeSessionsOutcome>(
      "deauthorize_sessions",
      {
        request: {
          device_ids: deviceIds,
          password: password ?? null,
          auth_session: pendingDeauthorization?.authSession ?? null,
        },
      },
    );

    if (result.kind === "password_required") {
      setPendingDeauthorization({
        deviceIds,
        authSession: result.auth_session,
      });
      setMessage({
        tone: "warning",
        text: "Enter your account password to deauthorize the selected sessions.",
      });
      return;
    }

    if (result.kind === "account_management_required") {
      await openUrl(result.account_management_url);
      setMessage({
        tone: "info",
        text: "Open account management to finish deauthorizing this session.",
      });
      return;
    }

    setPendingDeauthorization(null);
    setDeauthorizationPassword("");
    setSelectedDeviceIds([]);
    setMessage({
      tone: "success",
      text: "Selected sessions were deauthorized.",
    });
    await refreshOverview();
  }

  async function submitPasswordDeauthorization() {
    if (!pendingDeauthorization) {
      return;
    }

    await deauthorize(
      pendingDeauthorization.deviceIds,
      deauthorizationPassword,
    );
  }

  return (
    <div className="settings-view-section-body settings-view-section-body--sessions">
      <Card className="settings-view-card">
        <div className="settings-view-card-head">
          <ShieldCheck aria-hidden="true" />
          <div className="settings-view-card-copy">
            <Typography variant="h3">Session overview</Typography>
            <Typography muted variant="bodySmall">
              View your current and other sessions.
            </Typography>
          </div>
        </div>
        <div className="settings-view-action-row">
          <Button
            disabled={isBusy}
            onClick={() => void runAction("refresh-sessions", refreshOverview)}
            variant="secondary"
          >
            <RefreshCw aria-hidden="true" />
            Refresh
          </Button>
          <Button
            disabled={isBusy || selectedSessions.length === 0}
            onClick={() =>
              void runAction("bulk-deauthorize", () =>
                deauthorize(
                  selectedSessions.map((session) => session.device_id),
                ),
              )
            }
            variant="destructive"
          >
            <Trash2 aria-hidden="true" />
            Deauthorize selected
          </Button>
        </div>

        {overview.sessions.length === 0 ? (
          <Typography muted variant="bodySmall">
            Sessions are loading or unavailable while offline.
          </Typography>
        ) : (
          <div className="settings-view-session-list">
            {overview.sessions.map((session) => {
              const isExpanded = expandedDeviceId === session.device_id;
              const isSelected = selectedDeviceIds.includes(session.device_id);

              return (
                <article
                  className={`settings-view-session-row${
                    session.current ? " settings-view-session-row--current" : ""
                  }`}
                  key={session.device_id}
                >
                  <label className="settings-view-session-select">
                    <input
                      checked={isSelected}
                      disabled={session.current || isBusy}
                      onChange={() => toggleSelection(session.device_id)}
                      type="checkbox"
                    />
                    <span className="settings-view-session-select-label">
                      Select session
                    </span>
                  </label>
                  <div className="settings-view-session-main">
                    <div className="settings-view-session-title-row">
                      {session.trust.verified ? (
                        <ShieldCheck aria-hidden="true" />
                      ) : (
                        <ShieldAlert aria-hidden="true" />
                      )}
                      <div>
                        <Typography variant="label">
                          {displayName(session)}
                          {session.current ? " (current)" : ""}
                        </Typography>
                        <Typography muted variant="bodySmall">
                          {session.trust.verified ? "Verified" : "Unverified"}
                        </Typography>
                      </div>
                    </div>
                    <dl className="settings-view-session-facts">
                      <div>
                        <dt>Last active</dt>
                        <dd>{lastSeenLabel(session.last_seen_ts_unix_ms)}</dd>
                      </div>
                      <div>
                        <dt>IP address</dt>
                        <dd>{session.last_seen_ip ?? "Unknown"}</dd>
                      </div>
                      <div>
                        <dt>Device ID</dt>
                        <dd>{session.device_id}</dd>
                      </div>
                    </dl>
                    {isExpanded ? (
                      <div className="settings-view-session-expanded">
                        <Typography muted variant="bodySmall">
                          {session.current
                            ? "This device"
                            : session.trust.can_verify_current_session
                              ? "Can verify this session"
                              : "Cannot verify this session"}
                        </Typography>
                        <div className="settings-view-action-row">
                          {!session.current && !session.trust.verified ? (
                            <Button
                              disabled={isBusy}
                              onClick={() =>
                                void runAction(
                                  `verify-${session.device_id}`,
                                  () => startVerification(session),
                                )
                              }
                              variant="secondary"
                            >
                              <ShieldCheck aria-hidden="true" />
                              Verify
                            </Button>
                          ) : null}
                          {!session.current ? (
                            <Button
                              disabled={isBusy}
                              onClick={() =>
                                void runAction(
                                  `deauthorize-${session.device_id}`,
                                  () => deauthorize([session.device_id]),
                                )
                              }
                              variant="destructive"
                            >
                              <Trash2 aria-hidden="true" />
                              Deauthorize
                            </Button>
                          ) : null}
                        </div>
                      </div>
                    ) : null}
                  </div>
                  <Button
                    aria-expanded={isExpanded}
                    aria-label={`Toggle ${displayName(session)} details`}
                    iconOnly
                    onClick={() =>
                      setExpandedDeviceId((current) =>
                        current === session.device_id
                          ? null
                          : session.device_id,
                      )
                    }
                    variant="icon"
                  >
                    <ChevronDown aria-hidden="true" />
                  </Button>
                </article>
              );
            })}
          </div>
        )}
        {pendingDeauthorization ? (
          <div className="settings-view-inline-form">
            <TextField
              label="Account password"
              onChange={(event) =>
                setDeauthorizationPassword(event.currentTarget.value)
              }
              type="password"
              value={deauthorizationPassword}
            />
            <Button
              disabled={isBusy || deauthorizationPassword.trim().length === 0}
              onClick={() =>
                void runAction(
                  "confirm-deauthorization",
                  submitPasswordDeauthorization,
                )
              }
              variant="destructive"
            >
              <Trash2 aria-hidden="true" />
              Confirm
            </Button>
          </div>
        ) : null}
      </Card>

      <Card className="settings-view-card">
        <div
          className={`settings-view-card-head${
            overview.current_session_verified
              ? ""
              : " settings-view-card-head--inactive"
          }`}
        >
          {overview.current_session_verified ? (
            <ShieldCheck aria-hidden="true" />
          ) : (
            <ShieldAlert aria-hidden="true" />
          )}
          <div className="settings-view-card-copy">
            <Typography variant="h3">Session verification</Typography>
            <Typography muted variant="bodySmall">
              {currentSessionInstruction}
            </Typography>
          </div>
        </div>
        {!overview.current_session_verified ? (
          <div className="settings-view-action-row">
            <Button
              disabled={isBusy || currentSessionVerifiers.length === 0}
              onClick={() =>
                void runAction(
                  "verify-current-session",
                  startCurrentSessionVerification,
                )
              }
              variant="secondary"
            >
              <ShieldCheck aria-hidden="true" />
              Verify this session
            </Button>
          </div>
        ) : null}
        {currentSessionVerifiers.length > 0 &&
        !overview.current_session_verified ? (
          <Typography muted variant="bodySmall">
            Request will be sent to:{" "}
            {currentSessionVerifiers.map(displayName).join(", ")}
          </Typography>
        ) : null}
        {currentSession ? (
          <Typography muted variant="bodySmall">
            Current device ID: {currentSession.device_id}
          </Typography>
        ) : null}
      </Card>

      <SasVerificationOverlay
        isBusy={isBusy}
        isOpen={isSasOverlayOpen}
        sas={sasVerification}
        onClose={() => setIsSasOverlayOpen(false)}
        onConfirm={() =>
          void runAction("confirm-emoji-verification", confirmEmojiVerification)
        }
        onMismatch={() =>
          void runAction(
            "mismatch-emoji-verification",
            mismatchEmojiVerification,
          )
        }
        onRefresh={() =>
          void runAction("refresh-emoji-verification", refreshEmojiVerification)
        }
      />
    </div>
  );
}

type SasVerificationOverlayProps = {
  isBusy: boolean;
  isOpen: boolean;
  sas: SasVerificationView | null;
  onClose: () => void;
  onConfirm: () => void;
  onMismatch: () => void;
  onRefresh: () => void;
};

function SasVerificationOverlay({
  isBusy,
  isOpen,
  sas,
  onClose,
  onConfirm,
  onMismatch,
  onRefresh,
}: SasVerificationOverlayProps) {
  if (!isOpen || !sas) {
    return null;
  }

  return (
    <div
      className="ui-overlay settings-view-verification-overlay"
      role="dialog"
      aria-modal="true"
      aria-labelledby="settings-view-verification-title"
    >
      <button
        aria-label="Close emoji verification"
        className="ui-overlay-scrim ui-overlay-scrim--blurred"
        onClick={onClose}
        type="button"
      />
      <ScrollArea
        className="settings-view-verification-dialog"
        contentClassName="settings-view-verification-dialog-content"
      >
        <div className="settings-view-verification-head">
          <div>
            <Typography
              as="h2"
              id="settings-view-verification-title"
              variant="h2"
            >
              Compare emojis
            </Typography>
            <Typography muted variant="body">
              Confirm only if the emojis match on both sessions.
            </Typography>
          </div>
          <Button onClick={onClose} variant="ghost">
            Close
          </Button>
        </div>

        <FeedbackMessage tone={sas.cancelled ? "warning" : "info"}>
          Verification state: {sas.label}
          {sas.cancel_reason ? ` (${sas.cancel_reason})` : ""}
        </FeedbackMessage>

        {sas.emojis.length > 0 ? (
          <div className="settings-view-emoji-grid">
            {sas.emojis.map((emoji) => (
              <div className="settings-view-emoji-item" key={emoji.description}>
                <span className="settings-view-emoji-symbol">
                  {emoji.symbol}
                </span>
                <Typography variant="label">{emoji.description}</Typography>
              </div>
            ))}
          </div>
        ) : sas.decimals ? (
          <div className="settings-view-decimal-row">
            {sas.decimals.map((decimal) => (
              <span key={decimal}>{decimal}</span>
            ))}
          </div>
        ) : (
          <Typography muted variant="body">
            Waiting for the other session to accept and exchange verification
            data.
          </Typography>
        )}

        <div className="settings-view-action-row">
          <Button disabled={isBusy || sas.done} onClick={onRefresh}>
            <RefreshCw aria-hidden="true" />
            Refresh
          </Button>
          <Button
            disabled={isBusy || sas.done || !sas.can_be_presented}
            onClick={onConfirm}
            variant="primary"
          >
            <ShieldCheck aria-hidden="true" />
            They match
          </Button>
          <Button
            disabled={isBusy || sas.done || sas.cancelled}
            onClick={onMismatch}
            variant="destructive"
          >
            <ShieldAlert aria-hidden="true" />
            They do not match
          </Button>
        </div>
      </ScrollArea>
    </div>
  );
}
