/**
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

export type ThemeMode = 'light' | 'dark' | 'system';

type ColorSchemeName = Exclude<ThemeMode, 'system'>;

const lightColors = {
  //Primary colors
  primary: '#6750A4', //Main color used across screens and components
  onPrimary: '#FFFFFF', //Text and icons shown against the primary color
  primaryContainer: '#EADDFF', //Standout container color for key components
  onPrimaryContainer: '#4F378B', //Contrast-passing color shown against the primary container

  //Secondary colors
  secondary: '#625B71', //Accent color used across screens and components
  onSecondary: '#FFFFFF', //Text and icons shown against the secondary color
  secondaryContainer: '#E8DEF8', //Less prominent container color, for components like tonal buttons
  onSecondaryContainer: '#4A4458', //Contrast-passing color shown against the secondary container

  //Tertiary colors
  tertiary: '#7D5260', //Contrasting accent color used across screens and components
  onTertiary: '#FFFFFF', //Text and icons shown against the tertiary color
  tertiaryContainer: '#FFD8E4', //Contrasting container color, for components like input fields
  onTertiaryContainer: '#633B48', //Contrast-passing color shown against the tertiary container

  //Error colors
  error: '#B3261E', //Indicates errors, such as invalid input in a date picker
  onError: '#FFFFFF', //Used for text and icons on the error color
  errorContainer: '#F9DEDC', //Container color for error messages and badges
  onErrorContainer: '#8C1D18', //Used for text and icons on the error-container color
  
  //Surface colors
  surface: '#FEF7FF', //Surface color for components like cards, sheets, and menus
  onSurface: '#1D1B20', //Text and icons shown against the surface color
  surfaceVariant: '#E7E0EC', //Alternate surface color, can be used for active states
  onSurfaceVariant: '#49454F', //For text and icons to indicate active or inactive component state
  surfaceContainerHighest: '#E6E0E9',
  surfaceContainerHigh: '#ECE6F0',
  surfaceContainer: '#F3EDF7',
  surfaceContainerLow: '#F7F2FA',
  surfaceContainerLowest: '#FFFFFF',
  inverseSurface: '#322F35', //Displays opposite color of the surrounding UI
  inverseOnSurface: '#F5EFF7', //Used for text and icons shown against the inverse surface color
  surfaceTint: '#6750A4',
  surfaceTintColors: '#6750A4',
  
  //Outline colors
  outline: '#79747E', //Subtle color used for boundaries
  outlineVariant: '#CAC4D0', //Outline-variant is used to define the border of a component where 3:1 contrast ratio isn’t required, a container, or a divider.

  //Add-ons

  //Add-on primary colors
  primaryFixed: '#EADDFF', //Primary color that doesn't change for light or dark theme.
  onPrimaryFixed: '#21005D', //Used for text and icons shown against the primary fixed color
  primaryFixedDim: '#D0BCFF', //Dimmer version of primary fixed color that doesn't change for light or dark theme.
  onPrimaryFixedVariant: '#4F378B', //Stronger hue variant used for text and icons shown against the primary fixed color
  inversePrimary: '#D0BCFF', //Displays opposite of the primary color
  
  //Add-on secondary colors
  secondaryFixed: '#E8DEF8', //Secondary color that doesn't change for light or dark theme.
  onSecondaryFixed: '#1D192B', //Used for text and icons shown against the secondary fixed color
  secondaryFixedDim: '#CCC2DC', //Dimmer version of secondary fixed color that doesn't change for light or dark theme.
  onSecondaryFixedVariant: '#4A4458', //Stronger hue variant used for text and icons shown against the secondary fixed color

  //Add-on tertiary colors
  tertiaryFixed: '#FFD8E4', //Tertiary color that doesn't change for light or dark theme.
  onTertiaryFixed: '#31111D', //Used for text and icons shown against the tertiary fixed color
  tertiaryFixedDim: '#EFB8C8', //Dimmer version of tertiary fixed color that doesn't change for light or dark theme.
  onTertiaryFixedVariant: '#633B48', //Stronger hue variant used for text and icons shown against the tertiary fixed color

  //Add-on surface colors
  background: '#FEF7FF', //Note: Background is a legacy color role. It is recommended to use Surface instead of Background.
  onBackground: '#1D1B20', //Used for text and icons shown against the background color
  surfaceBright: '#FEF7FF', //Surface that is brighter in both light and dark theme.
  surfaceDim: '#DED8E1', //Surface that is dimmer in both light and dark theme.
  scrim: '#000000', //Used for scrims which help separate floating components from the background.
  shadow: '#000000', //For shadows applied to elevated components
};

export type ThemeColors = {
  [K in keyof typeof lightColors]: string;
};

const darkColors: ThemeColors = {
  //Primary colors
  primary: '#D0BCFF', //Main color used across screens and components
  onPrimary: '#381E72', //Text and icons shown against the primary color
  primaryContainer: '#4F378B', //Standout container color for key components
  onPrimaryContainer: '#EADDFF', //Contrast-passing color shown against the primary container

  //Secondary colors
  secondary: '#CCC2DC', //Accent color used across screens and components
  onSecondary: '#332D41', //Text and icons shown against the secondary color
  secondaryContainer: '#4A4458', //Less prominent container color, for components like tonal buttons
  onSecondaryContainer: '#E8DEF8', //Contrast-passing color shown against the secondary container

  //Tertiary colors
  tertiary: '#EFB8C8', //Contrasting accent color used across screens and components
  onTertiary: '#492532', //Text and icons shown against the tertiary color
  tertiaryContainer: '#633B48', //Contrasting container color, for components like input fields
  onTertiaryContainer: '#FFD8E4', //Contrast-passing color shown against the tertiary container

  //Error colors
  error: '#F2B8B5', //Indicates errors, such as invalid input in a date picker
  onError: '#601410', //Used for text and icons on the error color
  errorContainer: '#8C1D18', //Container color for error messages and badges
  onErrorContainer: '#F9DEDC', //Used for text and icons on the error-container color
  
  //Surface colors
  surface: '#141218', //Surface color for components like cards, sheets, and menus
  onSurface: '#E6E0E9', //Text and icons shown against the surface color
  surfaceVariant: '#49454F', //Alternate surface color, can be used for active states
  onSurfaceVariant: '#CAC4D0', //For text and icons to indicate active or inactive component state
  surfaceContainerHighest: '#36343B',
  surfaceContainerHigh: '#2B2930',
  surfaceContainer: '#211F26',
  surfaceContainerLow: '#1D1B20',
  surfaceContainerLowest: '#0F0D13',
  inverseSurface: '#E6E0E9', //Displays opposite color of the surrounding UI
  inverseOnSurface: '#322F35', //Used for text and icons shown against the inverse surface color
  surfaceTint: '#D0BCFF',
  surfaceTintColors: '#D0BCFF',
  
  //Outline colors
  outline: '#938F99', //Subtle color used for boundaries
  outlineVariant: '#49454F', //Outline-variant is used to define the border of a component where 3:1 contrast ratio isn’t required, a container, or a divider.

  //Add-ons

  //Add-on primary colors
  primaryFixed: '#EADDFF', //Primary color that doesn't change for light or dark theme.
  onPrimaryFixed: '#21005D', //Used for text and icons shown against the primary fixed color
  primaryFixedDim: '#D0BCFF', //Dimmer version of primary fixed color that doesn't change for light or dark theme.
  onPrimaryFixedVariant: '#4F378B', //Stronger hue variant used for text and icons shown against the primary fixed color
  inversePrimary: '#6750A4', //Displays opposite of the primary color
  
  //Add-on secondary colors
  secondaryFixed: '#E8DEF8', //Secondary color that doesn't change for light or dark theme.
  onSecondaryFixed: '#1D192B', //Used for text and icons shown against the secondary fixed color
  secondaryFixedDim: '#CCC2DC', //Dimmer version of secondary fixed color that doesn't change for light or dark theme.
  onSecondaryFixedVariant: '#4A4458', //Stronger hue variant used for text and icons shown against the secondary fixed color

  //Add-on tertiary colors
  tertiaryFixed: '#FFD8E4', //Tertiary color that doesn't change for light or dark theme.
  onTertiaryFixed: '#31111D', //Used for text and icons shown against the tertiary fixed color
  tertiaryFixedDim: '#EFB8C8', //Dimmer version of tertiary fixed color that doesn't change for light or dark theme.
  onTertiaryFixedVariant: '#633B48', //Stronger hue variant used for text and icons shown against the tertiary fixed color

  //Add-on surface colors
  background: '#141218', //Note: Background is a legacy color role. It is recommended to use Surface instead of Background.
  onBackground: '#E6E0E9', //Used for text and icons shown against the background color
  surfaceBright: '#3B383E', //Surface that is brighter in both light and dark theme.
  surfaceDim: '#141218', //Surface that is dimmer in both light and dark theme.
  scrim: '#000000', //Used for scrims which help separate floating components from the background.
  shadow: '#000000', //For shadows applied to elevated components
};

export const colors: Record<ColorSchemeName, ThemeColors> = {
  light: lightColors,
  dark: darkColors,
};
