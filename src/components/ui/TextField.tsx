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

import type { InputHTMLAttributes, ReactNode } from 'react';
import { classNames } from './classNames';

type TextFieldProps = InputHTMLAttributes<HTMLInputElement> & {
  label: string;
  isInvalid?: boolean;
  isRequiredVisible?: boolean;
  helperText?: ReactNode;
  controlClassName?: string;
};

export function TextField({
  label,
  isInvalid = false,
  isRequiredVisible = false,
  helperText,
  className,
  controlClassName,
  ...inputProps
}: TextFieldProps) {
  return (
    <label className={classNames('ui-field', className)}>
      <span className="ui-field__label">
        {label}
        {isRequiredVisible ? (
          <span className="ui-required-marker" aria-hidden="true">
            *
          </span>
        ) : null}
      </span>
      <input
        className={classNames('ui-field__control', controlClassName)}
        data-invalid={isInvalid || undefined}
        {...inputProps}
      />
      {helperText ? <span className="ui-field__helper">{helperText}</span> : null}
    </label>
  );
}
