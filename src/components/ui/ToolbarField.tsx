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

type ToolbarFieldProps = InputHTMLAttributes<HTMLInputElement> & {
  icon?: ReactNode;
};

export function ToolbarField({
  className,
  icon,
  type = 'text',
  ...props
}: ToolbarFieldProps) {
  return (
    <label className={classNames('ui-toolbar-field', className)}>
      {icon ? <span className="ui-toolbar-field__icon">{icon}</span> : null}
      <input className="ui-toolbar-field__input" type={type} {...props} />
    </label>
  );
}
