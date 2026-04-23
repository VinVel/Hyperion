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

export const settingsSections = [
  { id: 'account', label: 'Account' },
  { id: 'sessions', label: 'Sessions' },
  { id: 'appearance', label: 'Appearance' },
  { id: 'notifications', label: 'Notifications' },
  { id: 'chats', label: 'Chats' },
  { id: 'hotkeys', label: 'Hotkeys' },
  { id: 'calls', label: 'Calls' },
  { id: 'security', label: 'Security' },
  { id: 'encryption', label: 'Encryption' },
  { id: 'help-about', label: 'Help & About' },
] as const;

export type SettingsSectionId = (typeof settingsSections)[number]['id'];

export const sectionDescriptions: Record<SettingsSectionId, string> = {
  account: 'Manage account-level settings here once the account preferences land.',
  sessions: 'Review signed-in sessions and related session controls here later on.',
  appearance: 'Pick one preset palette.',
  notifications: 'Notification delivery, sounds, and attention settings will live here.',
  chats: 'Message list, room behavior, and chat-level preferences will live here.',
  hotkeys: 'Keyboard shortcut controls will appear here once hotkey settings are available.',
  calls: 'Audio, video, and call handling preferences will be configured here later.',
  security: 'General security controls and protections will live in this section.',
  encryption: 'Encryption-related controls and status details will be surfaced here later.',
  'help-about': 'Support, diagnostics, and product information will be available here later.',
};

export const defaultSettingsSectionId: SettingsSectionId = 'appearance';
