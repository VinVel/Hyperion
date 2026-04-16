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

type PillTone = 'neutral' | 'primary' | 'secondary';

type PillProps = HTMLAttributes<HTMLSpanElement> & {
  tone?: PillTone;
  children: ReactNode;
};

export function Pill({ tone = 'neutral', className, children, ...props }: PillProps) {
  return (
    <span className={classNames('ui-pill', `ui-pill--${tone}`, className)} {...props}>
      {children}
    </span>
  );
}
