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

import { UserRoundPlus, LogOut } from 'lucide-react';
import { useState } from 'react';
import { Button, Card, Typography } from '../../components/ui';

type AccountProps = {
  onAddAccount: () => void;
  onSignOut: () => Promise<void>;
};

function getErrorMessage(error: unknown): string {
  if (typeof error === 'string' && error.trim().length > 0) {
    return error;
  }

  if (error instanceof Error && error.message.trim().length > 0) {
    return error.message;
  }

  return 'The account action could not be completed.';
}

export default function Account({ onAddAccount, onSignOut }: AccountProps) {
  const [isSigningOut, setIsSigningOut] = useState(false);
  const [signOutError, setSignOutError] = useState<string | null>(null);

  async function handleSignOut() {
    if (isSigningOut) {
      return;
    }

    setSignOutError(null);
    setIsSigningOut(true);

    try {
      await onSignOut();
    } catch (error) {
      setSignOutError(getErrorMessage(error));
    } finally {
      setIsSigningOut(false);
    }
  }

  return (
    <div className="settings-view-section-body">
      <Card className="settings-view-card">
        <div className="settings-view-card-copy">
          <Typography variant="h3">Account access</Typography>
          <Typography muted variant="bodySmall">
            Open the existing login and registration flow to add another account, or sign out of
            the current one.
          </Typography>
        </div>

        <div className="settings-view-action-row">
          <Button onClick={onAddAccount} variant="secondary">
            <UserRoundPlus aria-hidden="true" />
            Add account
          </Button>

          <Button disabled={isSigningOut} onClick={() => void handleSignOut()} variant="destructive">
            <LogOut aria-hidden="true" />
            {isSigningOut ? 'Signing out...' : 'Sign out'}
          </Button>
        </div>

        {signOutError ? (
          <Typography className="settings-view-error" variant="bodySmall">
            {signOutError}
          </Typography>
        ) : null}
      </Card>
    </div>
  );
}
