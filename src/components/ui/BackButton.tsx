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

import { ChevronLeft } from 'lucide-react';
import type { ButtonHTMLAttributes } from 'react';
import { Button } from './Button';
import { classNames } from './classNames';

type BackButtonProps = Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'children'> & {
  overlay?: boolean;
};

export function BackButton({
  'aria-label': ariaLabel = 'Back',
  className,
  overlay = false,
  ...props
}: BackButtonProps) {
  return (
    <Button
      aria-label={ariaLabel}
      className={classNames('ui-back-button', overlay && 'ui-back-button--overlay', className)}
      iconOnly
      variant="ghost"
      {...props}
    >
      <ChevronLeft aria-hidden="true" />
    </Button>
  );
}
