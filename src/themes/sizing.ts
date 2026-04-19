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

export const sizing = {
  iconSmall: '1rem',
  iconMedium: '1.2rem',
  iconLarge: '1.4rem',
  iconButtonSize: '3.55rem',
  iconButtonCompactMinHeight: '3.4rem',
  brandLogoSize: '2.85rem',
  appNavLogoSize: '2.3rem',
  loginAvatarSize: '4.75rem',
  loginAvatarIconSize: '2.25rem',
  loginSignUpButtonMinWidth: '7rem',
  appRailButtonSize: '2.9rem',
  accountAvatarSize: '2.7rem',
  accountAvatarLargeSize: '3.35rem',
  roomListAvatarSize: '2.45rem',
  spaceTileIconSize: '2.9rem',
} as const;

export type SizingTokens = typeof sizing;
