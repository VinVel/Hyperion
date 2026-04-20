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

import { Blocks, MessagesSquare, Search } from 'lucide-react';
import { AppRail, AppRailButton } from '../../components/ui';
import {
  type AccountSummary,
  type AuthenticatedShellView,
  accountInitials,
} from './appShellAdapters';
import AppShellAccountPopoverContent from './AppShellAccountPopoverContent';

type AppShellNavigationProps = {
  activeAccount: AccountSummary;
  activeView: AuthenticatedShellView;
  isGlobalSearchOpen: boolean;
  isAccountCenterOpen: boolean;
  switchableAccounts: AccountSummary[];
  switchingAccountKey: string | null;
  onOpenGlobalSearch: () => void;
  onOpenMessages: () => void;
  onOpenSpaces: () => void;
  onToggleAccountCenter: () => void;
  onSwitchAccount: (account: AccountSummary) => void;
  onOpenSettings: () => void;
};

export default function AppShellNavigation({
  activeAccount,
  activeView,
  isGlobalSearchOpen,
  isAccountCenterOpen,
  switchableAccounts,
  switchingAccountKey,
  onOpenGlobalSearch,
  onOpenMessages,
  onOpenSpaces,
  onToggleAccountCenter,
  onSwitchAccount,
  onOpenSettings,
}: AppShellNavigationProps) {
  return (
    <>
      <AppRail className="app-shell-rail">
        <div className="app-shell-rail-group">
          <AppRailButton aria-label="Open global search" onClick={onOpenGlobalSearch}>
            <Search aria-hidden="true" />
          </AppRailButton>

          <AppRailButton
            aria-label="Open messages"
            isActive={activeView === 'messages'}
            onClick={onOpenMessages}
          >
            <MessagesSquare aria-hidden="true" />
          </AppRailButton>

          <AppRailButton
            aria-label="Open spaces"
            isActive={activeView === 'spaces'}
            onClick={onOpenSpaces}
          >
            <Blocks aria-hidden="true" />
          </AppRailButton>
        </div>

        <div className="app-shell-account-center">
          {isAccountCenterOpen ? (
            <section className="app-shell-account-popover" aria-label="Account center">
              <AppShellAccountPopoverContent
                activeAccount={activeAccount}
                switchableAccounts={switchableAccounts}
                switchingAccountKey={switchingAccountKey}
                onOpenSettings={onOpenSettings}
                onSwitchAccount={onSwitchAccount}
              />
            </section>
          ) : null}

          <button
            aria-expanded={isAccountCenterOpen}
            aria-haspopup="menu"
            className="app-shell-account-trigger"
            type="button"
            onClick={onToggleAccountCenter}
          >
            <span className="app-shell-account-avatar">{accountInitials(activeAccount)}</span>
          </button>
        </div>
      </AppRail>

      <nav className="app-shell-mobile-nav" aria-label="Primary navigation">
        <button
          aria-label="Open global search"
          className={`app-shell-mobile-nav-button${
            isGlobalSearchOpen ? ' app-shell-mobile-nav-button--active' : ''
          }`}
          type="button"
          onClick={onOpenGlobalSearch}
        >
          <Search aria-hidden="true" />
        </button>

        <button
          aria-label="Open messages"
          className={`app-shell-mobile-nav-button${
            activeView === 'messages' ? ' app-shell-mobile-nav-button--active' : ''
          }`}
          type="button"
          onClick={onOpenMessages}
        >
          <MessagesSquare aria-hidden="true" />
        </button>

        <button
          aria-label="Open spaces"
          className={`app-shell-mobile-nav-button${
            activeView === 'spaces' ? ' app-shell-mobile-nav-button--active' : ''
          }`}
          type="button"
          onClick={onOpenSpaces}
        >
          <Blocks aria-hidden="true" />
        </button>

        <button
          aria-expanded={isAccountCenterOpen}
          aria-haspopup="menu"
          aria-label="Open account center"
          className={`app-shell-mobile-nav-button app-shell-mobile-nav-button--account${
            isAccountCenterOpen ? ' app-shell-mobile-nav-button--active' : ''
          }`}
          type="button"
          onClick={onToggleAccountCenter}
        >
          <span className="app-shell-account-avatar">{accountInitials(activeAccount)}</span>
        </button>

        {isAccountCenterOpen ? (
          <section
            className="app-shell-account-popover app-shell-account-popover--mobile"
            aria-label="Account center"
          >
            <AppShellAccountPopoverContent
              activeAccount={activeAccount}
              switchableAccounts={switchableAccounts}
              switchingAccountKey={switchingAccountKey}
              onOpenSettings={onOpenSettings}
              onSwitchAccount={onSwitchAccount}
            />
          </section>
        ) : null}
      </nav>
    </>
  );
}
