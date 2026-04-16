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

export const layout = {
  viewportMinHeight: '100dvh',
  centeredScreenMinHeight: 'calc(100dvh - 5.5rem)',
  contentWidth: '78rem',
  contentWidthWide: '82rem',
  formWidth: '58rem',
  compactFormWidth: '42rem',
  loginPanelWidth: 'clamp(22rem, 50vw, 56rem)',
  directoryCardMinWidth: '18rem',
  webviewHostMinHeight: 'clamp(24rem, 72dvh, 52rem)',
  mobileBreakpoint: '640px',
  tabletBreakpoint: '760px',
  desktopBreakpoint: '960px',
} as const;

export type LayoutTokens = typeof layout;
