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

import type { ButtonHTMLAttributes } from 'react';
import { classNames } from './classNames';

type ToggleProps = Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'aria-pressed'> & {
  checked: boolean;
  label: string;
};

export function Toggle({ checked, label, className, type = 'button', ...props }: ToggleProps) {
  return (
    <button
      aria-label={label}
      aria-pressed={checked}
      className={classNames('ui-toggle', checked && 'ui-toggle--checked', className)}
      type={type}
      {...props}
    >
      <span className="ui-toggle__track" aria-hidden="true">
        <span className="ui-toggle__thumb" />
      </span>
    </button>
  );
}
