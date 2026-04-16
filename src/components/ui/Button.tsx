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

import type { ButtonHTMLAttributes, ReactNode } from 'react';
import { classNames } from './classNames';

type ButtonVariant = 'primary' | 'secondary' | 'ghost' | 'icon';

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: ButtonVariant;
  fullWidth?: boolean;
  iconOnly?: boolean;
  children: ReactNode;
};

export function Button({
  variant = 'secondary',
  fullWidth = false,
  iconOnly = false,
  className,
  children,
  type = 'button',
  ...props
}: ButtonProps) {
  return (
    <button
      className={classNames(
        'ui-button',
        `ui-button--${variant}`,
        fullWidth && 'ui-button--full-width',
        iconOnly && 'ui-button--icon-only',
        className,
      )}
      type={type}
      {...props}
    >
      {children}
    </button>
  );
}
