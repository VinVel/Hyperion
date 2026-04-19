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

import type { ButtonHTMLAttributes, HTMLAttributes, ReactNode } from 'react';
import { classNames } from './classNames';

type AppRailProps = HTMLAttributes<HTMLElement> & {
  children: ReactNode;
};

type AppRailButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  children: ReactNode;
  isActive?: boolean;
};

export function AppRail({ className, children, ...props }: AppRailProps) {
  return (
    <aside className={classNames('ui-app-rail', className)} {...props}>
      {children}
    </aside>
  );
}

export function AppRailButton({
  className,
  children,
  isActive = false,
  type = 'button',
  ...props
}: AppRailButtonProps) {
  return (
    <button
      className={classNames(
        'ui-app-rail-button',
        isActive && 'ui-app-rail-button--active',
        className,
      )}
      type={type}
      {...props}
    >
      {children}
    </button>
  );
}
