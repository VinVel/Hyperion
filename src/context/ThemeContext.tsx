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

import { createContext, useContext, useState, useEffect, type ReactNode } from 'react';
import { primitives, type ThemePrimitives } from '../themes/primitives';
import { colors, type ThemeColors, ThemeMode } from '../themes/theme';
import { useColorScheme } from '../hooks/useColorScheme';

interface ThemeContextType {
  theme: ThemeMode;
  setTheme: (mode: ThemeMode) => void;
  colors: ThemeColors;
  primitives: ThemePrimitives;
}

const ThemeContext = createContext<ThemeContextType | undefined>(undefined);

function toCssVariableName(token: string): string {
  return token.replace(/[A-Z]/g, (match) => `-${match.toLowerCase()}`);
}

function exposeTokenGroup(
  root: HTMLElement,
  prefix: string,
  tokens: Record<string, string>,
) {
  for (const [token, value] of Object.entries(tokens)) {
    root.style.setProperty(`--${prefix}-${toCssVariableName(token)}`, value);
  }
}

export const ThemeProvider = ({ children }: { children: ReactNode }) => {
  const [theme, setTheme] = useState<ThemeMode>('system');
  const systemPrefersDark = useColorScheme() === 'dark';
  const isDark = theme === 'system' ? systemPrefersDark : theme === 'dark';
  const currentColors = colors[isDark ? 'dark' : 'light'];

  // Apply active theme and expose palette and primitive values as CSS variables.
  useEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = isDark ? 'dark' : 'light';

    for (const [token, value] of Object.entries(currentColors)) {
      const cssToken = toCssVariableName(token);
      root.style.setProperty(`--${cssToken}`, value);
    }

    exposeTokenGroup(root, 'typography', primitives.typography);
    exposeTokenGroup(root, 'spacing', primitives.spacing);
    exposeTokenGroup(root, 'sizing', primitives.sizing);
    exposeTokenGroup(root, 'shape', primitives.shape);
    exposeTokenGroup(root, 'elevation', primitives.elevation);
    exposeTokenGroup(root, 'motion', primitives.motion);
    exposeTokenGroup(root, 'layout', primitives.layout);
  }, [isDark, currentColors]);

  return (
    <ThemeContext.Provider value={{ theme, setTheme, colors: currentColors, primitives }}>
      {children}
    </ThemeContext.Provider>
  );
};

export const useTheme = () => {
  const context = useContext(ThemeContext);
  if (!context) throw new Error('useTheme must be used within ThemeProvider');
  return context;
};     
