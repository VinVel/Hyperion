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

export type FeedbackTone = 'error' | 'success' | 'info' | 'warning';

type FeedbackMessageProps = HTMLAttributes<HTMLParagraphElement> & {
  tone: FeedbackTone;
  children: ReactNode;
};

export function FeedbackMessage({
  tone,
  className,
  children,
  ...props
}: FeedbackMessageProps) {
  return (
    <p
      className={classNames('ui-feedback', `ui-feedback--${tone}`, className)}
      {...props}
    >
      {children}
    </p>
  );
}
