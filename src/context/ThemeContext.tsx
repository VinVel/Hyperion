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
import { colors, ThemeMode } from '../themes/theme';
import { useColorScheme } from '../hooks/useColorScheme';

interface ThemeContextType {
  theme: ThemeMode;
  setTheme: (mode: ThemeMode) => void;
  colors: typeof colors.light;
}

const ThemeContext = createContext<ThemeContextType | undefined>(undefined);

export const ThemeProvider = ({ children }: { children: ReactNode }) => {
  const [theme, setTheme] = useState<ThemeMode>('system');
  const systemPrefersDark = useColorScheme() === 'dark';
  const isDark = theme === 'system' ? systemPrefersDark : theme === 'dark';
  const currentColors = colors[isDark ? 'dark' : 'light'];

  // Apply active theme and expose all palette values as CSS variables.
  useEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = isDark ? 'dark' : 'light';

    for (const [token, value] of Object.entries(currentColors)) {
      root.style.setProperty(`--${token}`, value);
    }
  }, [isDark, currentColors]);

  return (
    <ThemeContext.Provider value={{ theme, setTheme, colors: currentColors }}>
      {children}
    </ThemeContext.Provider>
  );
};

export const useTheme = () => {
  const context = useContext(ThemeContext);
  if (!context) throw new Error('useTheme must be used within ThemeProvider');
  return context;
};     
