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

import type { HTMLAttributes, ReactNode } from 'react';
import { classNames } from './classNames';

type ScreenShellProps = HTMLAttributes<HTMLDivElement> & {
  children: ReactNode;
};

type ScreenHeaderProps = HTMLAttributes<HTMLElement> & {
  children: ReactNode;
  wide?: boolean;
};

type ScreenMainProps = HTMLAttributes<HTMLElement> & {
  children: ReactNode;
  wide?: boolean;
  centered?: boolean;
  largeBlockPadding?: boolean;
};

export function ScreenShell({ className, children, ...props }: ScreenShellProps) {
  return (
    <div className={classNames('ui-screen-shell', className)} {...props}>
      {children}
    </div>
  );
}

export function ScreenHeader({
  wide = false,
  className,
  children,
  ...props
}: ScreenHeaderProps) {
  return (
    <header
      className={classNames(
        'ui-screen-header',
        wide && 'ui-screen-header--wide',
        className,
      )}
      {...props}
    >
      {children}
    </header>
  );
}

export function ScreenMain({
  wide = false,
  centered = false,
  largeBlockPadding = false,
  className,
  children,
  ...props
}: ScreenMainProps) {
  return (
    <main
      className={classNames(
        'ui-screen-main',
        wide && 'ui-screen-main--wide',
        centered && 'ui-screen-main--centered',
        largeBlockPadding && 'ui-screen-main--large-block-padding',
        className,
      )}
      {...props}
    >
      {children}
    </main>
  );
}
