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

import { invoke } from '@tauri-apps/api/core';
import {
  AlertTriangle,
  Download,
  KeyRound,
  RefreshCw,
  ShieldCheck,
  Trash2,
  Upload,
  UserCheck,
} from 'lucide-react';
import { useEffect, useState } from 'react';
import { Button, Card, FeedbackMessage, TextField, Toggle, Typography } from '../../components/ui';

type EncryptionOverview = {
  has_active_account: boolean;
  account_key: string | null;
  user_id: string | null;
  device_id: string | null;
  ed25519_key: string | null;
  curve25519_key: string | null;
  recovery_state: string | null;
  backup_state: string | null;
  server_key_storage_opted_out: boolean;
  verified_devices_only: boolean;
};

type GeneratedRecoveryKey = {
  recovery_key: string;
};

type RoomKeyImportSummary = {
  imported_count: number;
  total_count: number;
};

type CryptoIdentityResetOutcome =
  | { kind: 'completed' }
  | { kind: 'uiaa_required' }
  | { kind: 'oauth_required'; approval_url: string };

const elementEncryptionHelpUrl = 'https://element.io/de/help#encryption5';

type RecoveryConfirmation = {
  expectedKey: string;
  action: 'create' | 'rotate';
};

function messageFromError(error: unknown): string {
  if (typeof error === 'string' && error.trim().length > 0) {
    return error;
  }

  if (error instanceof Error && error.message.trim().length > 0) {
    return error.message;
  }

  return 'Encryption settings could not be updated.';
}

function statusLabel(value: string | null): string {
  return value ?? 'Unknown';
}

function normalizeRecoveryKeyForComparison(value: string): string {
  return value.replace(/\s+/g, '');
}

export default function Encryption() {
  const [overview, setOverview] = useState<EncryptionOverview | null>(null);
  const [recoveryKey, setRecoveryKey] = useState('');
  const [generatedRecoveryKey, setGeneratedRecoveryKey] = useState<string | null>(null);
  const [recoveryConfirmation, setRecoveryConfirmation] = useState<RecoveryConfirmation | null>(null);
  const [exportPassphrase, setExportPassphrase] = useState('');
  const [importPassphrase, setImportPassphrase] = useState('');
  const [pendingAction, setPendingAction] = useState<string | null>(null);
  const [message, setMessage] = useState<{ tone: 'success' | 'info' | 'warning'; text: string } | null>(
    null,
  );
  const [error, setError] = useState<string | null>(null);
  const [confirmDeleteRecovery, setConfirmDeleteRecovery] = useState(false);
  const [confirmIdentityReset, setConfirmIdentityReset] = useState(false);

  async function refreshOverview() {
    const nextOverview = await invoke<EncryptionOverview>('get_encryption_overview');
    setOverview(nextOverview);
  }

  useEffect(() => {
    void refreshOverview().catch((loadError) => {
      setError(messageFromError(loadError));
    });
  }, []);

  async function runAction(actionName: string, action: () => Promise<void>) {
    if (pendingAction) {
      return;
    }

    setPendingAction(actionName);
    setError(null);
    setMessage(null);

    try {
      await action();
      await refreshOverview();
    } catch (actionError) {
      setError(messageFromError(actionError));
    } finally {
      setPendingAction(null);
    }
  }

  if (!overview?.has_active_account) {
    return (
      <div className="settings-view-section-body">
        <Card className="settings-view-card">
          <div className="settings-view-card-copy">
            <Typography variant="h3">Encryption</Typography>
            <Typography muted variant="bodySmall">
              Sign in to a Matrix account before changing encryption settings.
            </Typography>
          </div>
          {error ? <FeedbackMessage tone="error">{error}</FeedbackMessage> : null}
        </Card>
      </div>
    );
  }

  const isBusy = pendingAction !== null;
  const isServerKeyStorageEnabled = !overview.server_key_storage_opted_out;
  const recoveryConfirmationPending = recoveryConfirmation !== null;
  const recoveryKeyInputIsEmpty = recoveryKey.trim().length === 0;

  return (
    <div className="settings-view-section-body settings-view-section-body--encryption">
      {error ? <FeedbackMessage tone="error">{error}</FeedbackMessage> : null}
      {message ? <FeedbackMessage tone={message.tone}>{message.text}</FeedbackMessage> : null}

      <Card className="settings-view-card">
        <div
          className={`settings-view-card-head${
            isServerKeyStorageEnabled ? '' : ' settings-view-card-head--inactive'
          }`}
        >
          <ShieldCheck aria-hidden="true" />
          <div className="settings-view-card-copy">
            <Typography variant="h3">Key storage</Typography>
            <Typography muted variant="bodySmall">
              Store encrypted room keys on the homeserver so this account can recover history on
              other devices.
            </Typography>
          </div>
        </div>

        <div className="settings-view-toggle-row">
          <div>
            <Typography variant="label">Server-side key storage</Typography>
            <Typography muted variant="bodySmall">
              {isServerKeyStorageEnabled ? 'Enabled' : 'Disabled'}
            </Typography>
          </div>
          <Toggle
            checked={isServerKeyStorageEnabled}
            disabled={isBusy}
            label="Server-side key storage"
            onClick={() =>
              void runAction('server-key-storage', async () => {
                if (isServerKeyStorageEnabled) {
                  await invoke('disable_server_key_storage');
                  setMessage({ tone: 'warning', text: 'Server-side key storage is disabled.' });
                } else {
                  await invoke('enable_server_key_storage');
                  setMessage({ tone: 'success', text: 'Server-side key storage is enabled.' });
                }
              })
            }
          />
        </div>

        <div className="settings-view-action-row">
          <a className="settings-view-link-button" href={elementEncryptionHelpUrl} target="_blank" rel="noreferrer">
            Learn more
          </a>
        </div>
      </Card>

      <Card className="settings-view-card">
        <div className="settings-view-card-head">
          <KeyRound aria-hidden="true" />
          <div className="settings-view-card-copy">
            <Typography variant="h3">Recovery</Typography>
            <Typography muted variant="bodySmall">
              Create, rotate, delete, or use a recovery key for encrypted account secrets.
            </Typography>
          </div>
        </div>

        <dl className="settings-view-detail-list">
          <div>
            <dt>Recovery state</dt>
            <dd>{statusLabel(overview.recovery_state)}</dd>
          </div>
          <div>
            <dt>Backup state</dt>
            <dd>{statusLabel(overview.backup_state)}</dd>
          </div>
        </dl>

        {generatedRecoveryKey ? (
          <FeedbackMessage tone="success">
            Recovery key: <span className="settings-view-key-value">{generatedRecoveryKey}</span>
          </FeedbackMessage>
        ) : null}
        {recoveryConfirmationPending ? (
          <FeedbackMessage tone="info">
            Paste the newly generated recovery key below to confirm that it was saved.
          </FeedbackMessage>
        ) : null}

        <div className="settings-view-action-row">
          <Button disabled={isBusy || recoveryConfirmationPending} onClick={() => void runAction('create-recovery', async () => {
            const result = await invoke<GeneratedRecoveryKey>('create_recovery_key');
            setGeneratedRecoveryKey(result.recovery_key);
            setRecoveryConfirmation({ expectedKey: result.recovery_key, action: 'create' });
            setRecoveryKey('');
            setMessage({ tone: 'info', text: 'Save this recovery key now. It is shown only once.' });
          })} variant="primary">
            <KeyRound aria-hidden="true" />
            Create key
          </Button>
          <Button disabled={isBusy || recoveryConfirmationPending} onClick={() => void runAction('rotate-recovery', async () => {
            const result = await invoke<GeneratedRecoveryKey>('rotate_recovery_key');
            setGeneratedRecoveryKey(result.recovery_key);
            setRecoveryConfirmation({ expectedKey: result.recovery_key, action: 'rotate' });
            setRecoveryKey('');
            setMessage({ tone: 'info', text: 'Save the new recovery key now.' });
          })} variant="secondary">
            <RefreshCw aria-hidden="true" />
            Rotate
          </Button>
          <Button
            disabled={isBusy || recoveryConfirmationPending}
            onClick={() => setConfirmDeleteRecovery((current) => !current)}
            variant="destructive"
          >
            <Trash2 aria-hidden="true" />
            Delete
          </Button>
        </div>

        {confirmDeleteRecovery ? (
          <div className="settings-view-confirmation">
            <AlertTriangle aria-hidden="true" />
            <Typography variant="bodySmall">
              Delete recovery and server backups for this account?
            </Typography>
            <Button disabled={isBusy} onClick={() => void runAction('delete-recovery', async () => {
              await invoke('delete_recovery');
              setGeneratedRecoveryKey(null);
              setRecoveryConfirmation(null);
              setConfirmDeleteRecovery(false);
              setMessage({ tone: 'warning', text: 'Recovery was deleted.' });
            })} variant="destructive">
              Confirm
            </Button>
          </div>
        ) : null}

        <div className="settings-view-inline-form">
          <TextField
            label="Recovery key"
            onChange={(event) => setRecoveryKey(event.currentTarget.value)}
            type="password"
            value={recoveryKey}
          />
          <Button disabled={isBusy || recoveryKeyInputIsEmpty} onClick={() => void runAction(recoveryConfirmationPending ? 'confirm-recovery-key' : 'recover', async () => {
            if (recoveryConfirmation) {
              const expectedKey = normalizeRecoveryKeyForComparison(recoveryConfirmation.expectedKey);
              const enteredKey = normalizeRecoveryKeyForComparison(recoveryKey);
              if (enteredKey !== expectedKey) {
                throw new Error('The recovery key does not match the newly generated key.');
              }

              const actionLabel = recoveryConfirmation.action === 'rotate' ? 'rotated' : 'created';
              setGeneratedRecoveryKey(null);
              setRecoveryConfirmation(null);
              setRecoveryKey('');
              setMessage({ tone: 'success', text: `Recovery key was ${actionLabel} and confirmed.` });
              return;
            }

            if (overview.recovery_state === 'Disabled') {
              throw new Error('Recovery is disabled for this account. Create a new recovery key first.');
            }

            await invoke('recover_with_recovery_key', { request: { recovery_key: recoveryKey } });
            setRecoveryKey('');
            setMessage({ tone: 'success', text: 'Encryption secrets were recovered.' });
          })} variant="secondary">
            {recoveryConfirmationPending ? 'Confirm' : 'Recover'}
          </Button>
        </div>
      </Card>

      <Card className="settings-view-card">
        <div className="settings-view-card-head">
          <ShieldCheck aria-hidden="true" />
          <div className="settings-view-card-copy">
            <Typography variant="h3">Advanced</Typography>
            <Typography muted variant="bodySmall">
              Inspect this device, export or import encrypted room keys, and reset the crypto
              identity if recovery cannot be repaired.
            </Typography>
          </div>
        </div>

        <dl className="settings-view-detail-list settings-view-detail-list--keys">
          <div>
            <dt>User</dt>
            <dd>{overview.user_id}</dd>
          </div>
          <div>
            <dt>Device ID</dt>
            <dd>{overview.device_id}</dd>
          </div>
          <div>
            <dt>Ed25519 fingerprint</dt>
            <dd>{overview.ed25519_key ?? 'Unknown'}</dd>
          </div>
          <div>
            <dt>Curve25519 key</dt>
            <dd>{overview.curve25519_key ?? 'Unknown'}</dd>
          </div>
        </dl>

        <div className="settings-view-inline-form">
          <TextField
            label="Export passphrase"
            onChange={(event) => setExportPassphrase(event.currentTarget.value)}
            type="password"
            value={exportPassphrase}
          />
          <Button disabled={isBusy || exportPassphrase.trim().length === 0} onClick={() => void runAction('export-keys', async () => {
            const path = await invoke<string | null>('export_room_keys', {
              request: { passphrase: exportPassphrase },
            });
            if (path) {
              setExportPassphrase('');
              setMessage({ tone: 'success', text: `Room keys were exported to ${path}.` });
            }
          })} variant="secondary">
            <Download aria-hidden="true" />
            Export
          </Button>
        </div>

        <div className="settings-view-inline-form">
          <TextField
            label="Import passphrase"
            onChange={(event) => setImportPassphrase(event.currentTarget.value)}
            type="password"
            value={importPassphrase}
          />
          <Button disabled={isBusy || importPassphrase.trim().length === 0} onClick={() => void runAction('import-keys', async () => {
            const result = await invoke<RoomKeyImportSummary | null>('import_room_keys', {
              request: { passphrase: importPassphrase },
            });
            if (result) {
              setImportPassphrase('');
              setMessage({
                tone: 'success',
                text: `Imported ${result.imported_count} of ${result.total_count} room keys.`,
              });
            }
          })} variant="secondary">
            <Upload aria-hidden="true" />
            Import
          </Button>
        </div>

        <div className="settings-view-action-row">
          <Button
            disabled={isBusy}
            onClick={() => setConfirmIdentityReset((current) => !current)}
            variant="destructive"
          >
            <AlertTriangle aria-hidden="true" />
            Reset identity
          </Button>
        </div>

        {confirmIdentityReset ? (
          <div className="settings-view-confirmation">
            <AlertTriangle aria-hidden="true" />
            <Typography variant="bodySmall">
              Reset this account&apos;s crypto identity only if you cannot recover encryption.
            </Typography>
            <Button disabled={isBusy} onClick={() => void runAction('reset-identity', async () => {
              const result = await invoke<CryptoIdentityResetOutcome>('reset_crypto_identity');
              setConfirmIdentityReset(false);
              if (result.kind === 'completed') {
                setMessage({ tone: 'success', text: 'Crypto identity was reset.' });
              } else if (result.kind === 'oauth_required') {
                setMessage({
                  tone: 'warning',
                  text: `Approve the reset in a browser: ${result.approval_url}`,
                });
              } else {
                setMessage({
                  tone: 'warning',
                  text: 'The homeserver requires interactive authentication before reset.',
                });
              }
            })} variant="destructive">
              Confirm reset
            </Button>
          </div>
        ) : null}
      </Card>

      <Card className="settings-view-card">
        <div className="settings-view-card-head">
          <UserCheck aria-hidden="true" />
          <div className="settings-view-card-copy">
            <Typography variant="h3">Other people’s devices</Typography>
            <Typography muted variant="bodySmall">
              Only send new room keys to devices that are trusted. Messages may fail for people with
              unverified devices.
            </Typography>
          </div>
        </div>

        <div className="settings-view-toggle-row">
          <div>
            <Typography variant="label">Only verified devices</Typography>
            <Typography muted variant="bodySmall">
              Applies immediately by reconnecting the active Matrix client.
            </Typography>
          </div>
          <Toggle
            checked={overview.verified_devices_only}
            disabled={isBusy}
            label="Only verified devices"
            onClick={() => void runAction('verified-devices-only', async () => {
              await invoke('set_verified_devices_only', {
                enabled: !overview.verified_devices_only,
              });
              setMessage({ tone: 'success', text: 'Device trust preference was saved.' });
            })}
          />
        </div>
      </Card>
    </div>
  );
}
