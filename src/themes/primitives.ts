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

import { elevation } from './elevation';
import { layout } from './layout';
import { motion } from './motion';
import { shape } from './shape';
import { sizing } from './sizing';
import { spacing } from './spacing';
import { typography } from './typography';

export const primitives = {
  typography,
  spacing,
  sizing,
  shape,
  elevation,
  motion,
  layout,
} as const;

export type ThemePrimitives = typeof primitives;
