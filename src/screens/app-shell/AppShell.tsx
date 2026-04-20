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

import { FeedbackMessage, ScreenMain, ScreenShell } from '../../components/ui';
import type { AccountSummary } from './appShellAdapters';
import AppShellMessagesView from './AppShellMessagesView';
import AppShellNavigation from './AppShellNavigation';
import AppShellSearchOverlay from './AppShellSearchOverlay';
import AppShellSettingsView from './AppShellSettingsView';
import AppShellSpacesView from './AppShellSpacesView';
import useAppShellState from './useAppShellState';
import './AppShell.css';

type AppShellProps = {
  activeAccount: AccountSummary;
  onActiveAccountChange: (nextAccount: AccountSummary) => void;
};

export default function AppShell({
  activeAccount,
  onActiveAccountChange,
}: AppShellProps) {
  const shell = useAppShellState({
    activeAccount,
    onActiveAccountChange,
  });

  return (
    <ScreenShell
      className={`app-shell-root${
        shell.isThreadOpen ? ' app-shell-root--thread-selected' : ''
      }`}
    >
      <ScreenMain
        className={`app-shell-screen${
          shell.isThreadOpen ? ' app-shell-screen--thread-selected' : ''
        }`}
        largeBlockPadding
        wide
      >
        <section className="app-shell-layout" aria-label="Authenticated application shell">
          <AppShellNavigation
            activeAccount={activeAccount}
            activeView={shell.activeView}
            isAccountCenterOpen={shell.isAccountCenterOpen}
            isGlobalSearchOpen={shell.isGlobalSearchOpen}
            switchableAccounts={shell.switchableAccounts}
            switchingAccountKey={shell.switchingAccountKey}
            onOpenGlobalSearch={shell.openGlobalSearch}
            onOpenMessages={shell.openMessagesView}
            onOpenSettings={shell.openSettingsView}
            onOpenSpaces={shell.openSpacesView}
            onSwitchAccount={(account) => void shell.switchAccount(account)}
            onToggleAccountCenter={shell.toggleAccountCenter}
          />

          <div
            className={`app-shell-workspace${
              shell.activeView ? ` app-shell-workspace--${shell.activeView}` : ''
            }${
              shell.activeView === 'messages' && shell.selectedThread
                ? ' app-shell-workspace--thread-selected'
                : ''
            }`}
          >
            {shell.activeView === 'messages' ? (
              <AppShellMessagesView
                composerValue={shell.composerValue}
                isLoadingOlderMessages={shell.isLoadingOlderMessages}
                isLoadingShell={shell.isLoadingShell}
                isSendingMessage={shell.isSendingMessage}
                isSortMenuOpen={shell.isSortMenuOpen}
                selectedRoomSummary={shell.selectedRoomSummary}
                selectedThread={shell.selectedThread}
                selectedTimeline={shell.selectedTimeline}
                threadSearchQuery={shell.threadSearchQuery}
                threadSort={shell.threadSort}
                visibleThreads={shell.visibleThreads}
                onCloseThread={shell.closeThread}
                onComposerChange={shell.setComposerValue}
                onLoadOlderMessages={() => void shell.loadOlderMessages()}
                onOpenThread={shell.selectThread}
                onSelectSort={shell.selectSort}
                onSendMessage={() => void shell.sendMessage()}
                onThreadSearchChange={shell.setThreadSearchQuery}
                onToggleSortMenu={shell.toggleSortMenu}
              />
            ) : null}

            {shell.activeView === 'spaces' ? (
              <AppShellSpacesView
                selectedSpace={shell.selectedSpace}
                spaceSearchQuery={shell.spaceSearchQuery}
                visibleSpaces={shell.visibleSpaces}
                onSearchChange={shell.setSpaceSearchQuery}
                onSelectSpace={shell.selectSpace}
              />
            ) : null}

            {shell.activeView === 'settings' ? (
              <AppShellSettingsView onBackToMessages={shell.openMessagesView} />
            ) : null}
          </div>
        </section>

        {shell.feedbackMessage ? (
          <FeedbackMessage className="app-shell-feedback" tone={shell.feedbackMessage.tone}>
            {shell.feedbackMessage.text}
          </FeedbackMessage>
        ) : null}

        <AppShellSearchOverlay
          globalSearchQuery={shell.globalSearchQuery}
          isOpen={shell.isGlobalSearchOpen}
          results={shell.globalSearchResults}
          onClose={shell.closeGlobalSearch}
          onQueryChange={shell.setGlobalSearchQuery}
          onSelectResult={shell.handleGlobalSearchResult}
        />
      </ScreenMain>
    </ScreenShell>
  );
}
