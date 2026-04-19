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

type EmptyStateProps = HTMLAttributes<HTMLElement> & {
  title: ReactNode;
  copy: ReactNode;
  graphic?: ReactNode;
  actions?: ReactNode;
};

export function EmptyState({
  title,
  copy,
  graphic,
  actions,
  className,
  ...props
}: EmptyStateProps) {
  return (
    <section className={classNames('ui-empty-state', className)} {...props}>
      {graphic ? <div className="ui-empty-state__graphic">{graphic}</div> : null}
      <div className="ui-empty-state__copy">
        <h2 className="ui-empty-state__title">{title}</h2>
        <p className="ui-empty-state__text">{copy}</p>
      </div>
      {actions ? <div className="ui-empty-state__actions">{actions}</div> : null}
    </section>
  );
}
