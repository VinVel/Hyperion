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

import { ScreenMain, ScreenShell, useFeedbackToast } from "../../components/ui";
import { invoke } from "@tauri-apps/api/core";
import { useEffect } from "react";
import type { AccountSummary } from "./appShellAdapters";
import AppShellMessagesView from "./AppShellMessagesView";
import AppShellNavigation from "./AppShellNavigation";
import AppShellSpacesView from "./AppShellSpacesView";
import { AppShellDiscoveryOverlay } from "./discovery";
import {
  SettingsView,
  encryptionOverviewStorageKey,
  type EncryptionOverview,
} from "../Settings";
import { AppShellSearchOverlay } from "./search";
import useAppShellState from "./useAppShellState";
import "./AppShell.css";

type AppShellProps = {
  activeAccount: AccountSummary;
  onAddAccount: () => void;
  onActiveAccountChange: (nextAccount: AccountSummary) => void;
  onSignedOut: (nextAccount: AccountSummary | null) => void;
};

export default function AppShell({
  activeAccount,
  onAddAccount,
  onActiveAccountChange,
  onSignedOut,
}: AppShellProps) {
  const shell = useAppShellState({
    activeAccount,
    onActiveAccountChange,
  });
  useFeedbackToast(shell.feedbackMessage);

  useEffect(() => {
    let cancelled = false;

    async function refreshEncryptionSettingsSnapshot() {
      try {
        const overview = await invoke<EncryptionOverview>(
          "get_encryption_overview",
        );
        if (cancelled || !overview.has_active_account) {
          return;
        }

        window.localStorage.setItem(
          encryptionOverviewStorageKey(activeAccount.account_key),
          JSON.stringify({ ...overview, has_active_account: true }),
        );
      } catch {
        // Offline startup should keep using the last local settings snapshot.
      }
    }

    void refreshEncryptionSettingsSnapshot();

    return () => {
      cancelled = true;
    };
  }, [activeAccount.account_key]);

  return (
    <ScreenShell
      className={`app-shell-root${
        shell.isThreadOpen ? " app-shell-root--thread-selected" : ""
      }`}
    >
      <ScreenMain
        className={`app-shell-screen${
          shell.isThreadOpen ? " app-shell-screen--thread-selected" : ""
        }`}
        largeBlockPadding
        wide
      >
        <section
          className="app-shell-layout"
          aria-label="Authenticated application shell"
        >
          <AppShellNavigation
            activeAccount={activeAccount}
            activeView={shell.activeView}
            isAccountCenterOpen={shell.isAccountCenterOpen}
            isDiscoveryOpen={shell.isDiscoveryOpen}
            isGlobalSearchOpen={shell.isGlobalSearchOpen}
            switchableAccounts={shell.switchableAccounts}
            switchingAccountKey={shell.switchingAccountKey}
            onOpenGlobalSearch={shell.openGlobalSearch}
            onOpenDiscovery={shell.openDiscovery}
            onOpenMessages={shell.openMessagesView}
            onOpenSettings={shell.openSettingsView}
            onOpenSpaces={shell.openSpacesView}
            onSwitchAccount={(account) => void shell.switchAccount(account)}
            onToggleAccountCenter={shell.toggleAccountCenter}
          />

          <div
            className={`app-shell-workspace${
              shell.activeView
                ? ` app-shell-workspace--${shell.activeView}`
                : ""
            }${
              shell.activeView === "messages" && shell.selectedThread
                ? " app-shell-workspace--thread-selected"
                : ""
            }`}
          >
            {shell.activeView === "messages" ? (
              <AppShellMessagesView
                composerValue={shell.composerValue}
                activeComposerMode={shell.activeComposerMode}
                accountKey={activeAccount.account_key}
                isLoadingOlderMessages={shell.isLoadingOlderMessages}
                paginationState={shell.paginationState}
                isSendingMessage={shell.isSendingMessage}
                isSortMenuOpen={shell.isSortMenuOpen}
                selectedRoomSummary={shell.selectedRoomSummary}
                selectedThread={shell.selectedThread}
                selectedTimeline={shell.selectedTimeline}
                selectedTypingUsers={shell.selectedTypingUsers}
                threadKindFilter={shell.threadKindFilter}
                threadSort={shell.threadSort}
                visibleThreads={shell.visibleThreads}
                onCloseThread={shell.closeThread}
                onBeginEditMessage={shell.beginEditMessage}
                onBeginReplyToMessage={shell.beginReplyToMessage}
                onCancelComposerMode={shell.cancelComposerMode}
                onComposerChange={shell.setComposerValue}
                onLoadOlderMessages={shell.loadOlderMessages}
                onOpenThread={shell.selectThread}
                onRedactMessage={shell.redactMessage}
                onSelectSort={shell.selectSort}
                onSendMessage={shell.sendMessage}
                onThreadKindFilterChange={shell.setThreadKindFilter}
                onToggleReaction={shell.toggleReaction}
                onToggleSortMenu={shell.toggleSortMenu}
              />
            ) : null}

            {shell.activeView === "spaces" ? (
              <AppShellSpacesView
                selectedSpace={shell.selectedSpace}
                visibleSpaces={shell.visibleSpaces}
                onSelectSpace={shell.selectSpace}
              />
            ) : null}

            {shell.activeView === "settings" ? (
              <SettingsView
                activeAccount={activeAccount}
                onAddAccount={onAddAccount}
                onSignedOut={onSignedOut}
              />
            ) : null}
          </div>
        </section>

        <AppShellSearchOverlay
          globalSearchQuery={shell.globalSearchQuery}
          isOpen={shell.isGlobalSearchOpen}
          results={shell.globalSearchResults}
          statusNotice={shell.globalSearchStatusNotice}
          onClose={shell.closeGlobalSearch}
          onQueryChange={shell.setGlobalSearchQuery}
          onSelectResult={shell.handleGlobalSearchResult}
        />

        <AppShellDiscoveryOverlay
          isOpen={shell.isDiscoveryOpen}
          onClose={shell.closeDiscovery}
          onError={shell.handleDiscoveryError}
          onInviteSent={shell.handleDiscoveryInviteSent}
          onJoined={shell.handleDiscoveryJoined}
        />
      </ScreenMain>
    </ScreenShell>
  );
}
