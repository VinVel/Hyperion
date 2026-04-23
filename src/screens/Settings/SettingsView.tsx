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
import { ChevronLeft, ChevronRight } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Button, Card, Typography } from '../../components/ui';
import Account from './Account';
import Appearance from './Appearance';
import Calls from './Calls';
import Chats from './Chats';
import Encryption from './Encryption';
import HelpAbout from './HelpAbout';
import Hotkeys from './Hotkeys';
import Notifications from './Notifications';
import Security from './Security';
import Sessions from './Sessions';
import {
  defaultSettingsSectionId,
  sectionDescriptions,
  settingsSections,
  type SettingsSectionId,
} from './settingsSections';
import './SettingsView.css';

type SettingsViewProps = {
  onAddAccount: () => void;
  onSignedOut: (nextAccount: AccountSummary | null) => void;
};

type AccountSummary = {
  account_key: string;
  user_id: string;
  homeserver_url: string;
  is_active: boolean;
};

const mobileBreakpoint = 760;

function getIsMobileViewport() {
  return window.matchMedia(`(max-width: ${mobileBreakpoint}px)`).matches;
}

type SettingsSectionContentProps = {
  onAddAccount: () => void;
  onSignOut: () => Promise<void>;
  sectionId: SettingsSectionId;
};

function SettingsSectionContent({
  onAddAccount,
  onSignOut,
  sectionId,
}: SettingsSectionContentProps) {
  switch (sectionId) {
    case 'account':
      return <Account onAddAccount={onAddAccount} onSignOut={onSignOut} />;
    case 'sessions':
      return <Sessions />;
    case 'appearance':
      return <Appearance />;
    case 'notifications':
      return <Notifications />;
    case 'chats':
      return <Chats />;
    case 'hotkeys':
      return <Hotkeys />;
    case 'calls':
      return <Calls />;
    case 'security':
      return <Security />;
    case 'encryption':
      return <Encryption />;
    case 'help-about':
      return <HelpAbout />;
  }
}

export default function SettingsView({
  onAddAccount,
  onSignedOut,
}: SettingsViewProps) {
  const [activeSectionId, setActiveSectionId] =
    useState<SettingsSectionId>(defaultSettingsSectionId);
  const [isMobile, setIsMobile] = useState(getIsMobileViewport);
  const [mobileOpenedSectionId, setMobileOpenedSectionId] = useState<SettingsSectionId | null>(
    null,
  );

  useEffect(() => {
    const mediaQuery = window.matchMedia(`(max-width: ${mobileBreakpoint}px)`);

    const handleViewportChange = (event: MediaQueryListEvent) => {
      setIsMobile(event.matches);
    };

    setIsMobile(mediaQuery.matches);
    mediaQuery.addEventListener('change', handleViewportChange);

    return () => mediaQuery.removeEventListener('change', handleViewportChange);
  }, []);

  useEffect(() => {
    if (!isMobile) {
      setMobileOpenedSectionId(null);
    }
  }, [isMobile]);

  const activeSection =
    settingsSections.find((section) => section.id === activeSectionId) ?? settingsSections[0];
  const showMobileSectionDetail = isMobile && mobileOpenedSectionId !== null;

  async function handleSignOut() {
    const nextAccount = await invoke<AccountSummary | null>('sign_out_active_account');
    onSignedOut(nextAccount);
  }

  function openSection(sectionId: SettingsSectionId) {
    setActiveSectionId(sectionId);

    if (isMobile) {
      setMobileOpenedSectionId(sectionId);
    }
  }

  if (showMobileSectionDetail && mobileOpenedSectionId) {
    const mobileSection =
      settingsSections.find((section) => section.id === mobileOpenedSectionId) ??
      settingsSections[0];

    return (
      <section
        className="app-shell-main-pane app-shell-main-pane--full settings-view-root"
        aria-label="Settings"
      >
        <div className="settings-view-mobile-detail">
          <header className="settings-view-mobile-detail-head">
            <Button onClick={() => setMobileOpenedSectionId(null)} variant="secondary">
              <ChevronLeft aria-hidden="true" />
              All settings
            </Button>
          </header>

          <div className="settings-view-title">
            <Typography variant="eyebrow">Settings</Typography>
            <Typography variant="h2">{mobileSection.label}</Typography>
            <Typography muted variant="body">
              {sectionDescriptions[mobileSection.id]}
            </Typography>
          </div>

          <SettingsSectionContent
            onAddAccount={onAddAccount}
            onSignOut={handleSignOut}
            sectionId={mobileSection.id}
          />
        </div>
      </section>
    );
  }

  return (
    <section
      className="app-shell-main-pane app-shell-main-pane--full settings-view-root"
      aria-label="Settings"
    >
      <div className="settings-view-shell">
        <header className="settings-view-topbar">
          <div className="settings-view-title">
            <Typography variant="eyebrow">Settings</Typography>
            <Typography variant="h2">
              {isMobile ? 'Choose a section' : activeSection.label}
            </Typography>
            <Typography muted variant="body">
              {isMobile
                ? 'Select a settings area to drill into its detailed controls.'
                : sectionDescriptions[activeSection.id]}
            </Typography>
          </div>
        </header>

        <div className="settings-view-columns">
          <nav className="settings-view-nav" aria-label="Settings sections">
            <Card className="settings-view-nav-card">
              <div className="settings-view-nav-list" role="list">
                {settingsSections.map((section) => {
                  const isActive = section.id === activeSectionId;

                  return (
                    <button
                      key={section.id}
                      aria-current={isActive ? 'page' : undefined}
                      className={`settings-view-nav-item${
                        isActive ? ' settings-view-nav-item--active' : ''
                      }`}
                      onClick={() => openSection(section.id)}
                      type="button"
                    >
                      <div className="settings-view-nav-copy">
                        <Typography variant="label">{section.label}</Typography>
                      </div>
                      <ChevronRight aria-hidden="true" />
                    </button>
                  );
                })}
              </div>
            </Card>
          </nav>

          {!isMobile ? (
            <div className="settings-view-detail">
              <SettingsSectionContent
                onAddAccount={onAddAccount}
                onSignOut={handleSignOut}
                sectionId={activeSection.id}
              />
            </div>
          ) : null}
        </div>
      </div>
    </section>
  );
}
