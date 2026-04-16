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

import { createElement, type HTMLAttributes, type ReactNode } from 'react';
import { classNames } from './classNames';

type TypographyVariant =
  | 'h1'
  | 'h2'
  | 'h3'
  | 'body'
  | 'bodySmall'
  | 'meta'
  | 'label'
  | 'eyebrow';

type TypographyElement = 'h1' | 'h2' | 'h3' | 'p' | 'span';

type TypographyProps = HTMLAttributes<HTMLElement> & {
  as?: TypographyElement;
  variant: TypographyVariant;
  muted?: boolean;
  children: ReactNode;
};

const defaultElements: Record<TypographyVariant, TypographyElement> = {
  h1: 'h1',
  h2: 'h2',
  h3: 'h3',
  body: 'p',
  bodySmall: 'p',
  meta: 'span',
  label: 'span',
  eyebrow: 'p',
};

export function Typography({
  as,
  variant,
  muted = false,
  className,
  children,
  ...props
}: TypographyProps) {
  return createElement(
    as ?? defaultElements[variant],
    {
      className: classNames(
        'ui-typography',
        `ui-typography--${variant}`,
        muted && 'ui-typography--muted',
        className,
      ),
      ...props,
    },
    children,
  );
}
