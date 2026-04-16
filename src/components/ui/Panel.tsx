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

type PanelProps = HTMLAttributes<HTMLElement> & {
  children: ReactNode;
  narrow?: boolean;
  as?: 'section' | 'article' | 'div';
};

export function Panel({
  as: Element = 'section',
  narrow = false,
  className,
  children,
  ...props
}: PanelProps) {
  return (
    <Element
      className={classNames('ui-panel', narrow && 'ui-panel--narrow', className)}
      {...props}
    >
      {children}
    </Element>
  );
}

export function Card({ className, children, ...props }: HTMLAttributes<HTMLElement>) {
  return (
    <article className={classNames('ui-card', className)} {...props}>
      {children}
    </article>
  );
}
