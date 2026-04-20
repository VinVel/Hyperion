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

import { Cog } from 'lucide-react';
import { Button, Typography } from '../../components/ui';
import { type AccountSummary, accountInitials } from './appShellAdapters';

type AppShellAccountPopoverContentProps = {
  activeAccount: AccountSummary;
  switchableAccounts: AccountSummary[];
  switchingAccountKey: string | null;
  onSwitchAccount: (account: AccountSummary) => void;
  onOpenSettings: () => void;
};

export default function AppShellAccountPopoverContent({
  activeAccount,
  switchableAccounts,
  switchingAccountKey,
  onSwitchAccount,
  onOpenSettings,
}: AppShellAccountPopoverContentProps) {
  return (
    <>
      <div className="app-shell-account-popover-head">
        <span className="app-shell-account-avatar app-shell-account-avatar--large">
          {accountInitials(activeAccount)}
        </span>
        <div className="app-shell-account-copy">
          <Typography variant="h3">{activeAccount.user_id}</Typography>
          <Typography variant="meta" muted>
            {activeAccount.homeserver_url}
          </Typography>
        </div>
      </div>

      <div className="app-shell-account-section">
        <Typography variant="label" className="app-shell-section-label">
          Switch account
        </Typography>
        {switchableAccounts.length === 0 ? (
          <Typography variant="bodySmall" muted>
            Sign into another account first to switch here later.
          </Typography>
        ) : (
          <div className="app-shell-account-list">
            {switchableAccounts.map((account) => (
              <button
                key={account.account_key}
                className="app-shell-account-option"
                disabled={switchingAccountKey !== null}
                type="button"
                onClick={() => onSwitchAccount(account)}
              >
                <span className="app-shell-account-avatar">
                  {accountInitials(account)}
                </span>
                <span className="app-shell-account-option-copy">
                  <span className="app-shell-account-option-title">{account.user_id}</span>
                  <span className="app-shell-account-option-meta">
                    {switchingAccountKey === account.account_key
                      ? 'Switching...'
                      : account.homeserver_url}
                  </span>
                </span>
              </button>
            ))}
          </div>
        )}
      </div>

      <div className="app-shell-account-actions">
        <Button className="app-shell-settings-button" onClick={onOpenSettings}>
          <Cog aria-hidden="true" />
          <span>Settings</span>
        </Button>
      </div>
    </>
  );
}
