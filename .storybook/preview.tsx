import type { Preview } from "@storybook/react-vite";
import {
  createInMemoryThemePreferences,
  ThemeProvider,
} from "../src/components/context";
import { ToastProvider } from "../src/components/ui";
import {
  DEFAULT_THEME_PRESET,
  isThemePresetName,
  type ThemeMode,
  type ThemePresetName,
} from "../src/components/themes";
import "overlayscrollbars/overlayscrollbars.css";
import "../src/components/ui/ui.css";
import "../src/App.css";

const themeModes: ThemeMode[] = ["light", "dark"];
const themePresets: ThemePresetName[] = ["crystal", "ocean", "forest", "sun"];

const preview: Preview = {
  parameters: {
    controls: {
      matchers: {
        color: /(background|color)$/i,
        date: /Date$/,
      },
    },
    layout: "centered",
  },
  globalTypes: {
    themeMode: {
      description: "Color scheme used by the component preview",
      toolbar: {
        icon: "circlehollow",
        items: themeModes,
        title: "Theme mode",
      },
    },
    themePreset: {
      description: "Color palette used by the component preview",
      toolbar: {
        icon: "paintbrush",
        items: themePresets,
        title: "Theme preset",
      },
    },
  },
  decorators: [
    (Story, context) => {
      const themeMode = themeModes.includes(context.globals.themeMode)
        ? context.globals.themeMode
        : "light";
      const themePreset = isThemePresetName(context.globals.themePreset)
        ? context.globals.themePreset
        : DEFAULT_THEME_PRESET;
      const preferences = createInMemoryThemePreferences({
        themeMode,
        themePreset,
      });

      return (
        <ThemeProvider
          key={`${themeMode}-${themePreset}`}
          preferences={preferences}
        >
          <ToastProvider>
            <Story />
          </ToastProvider>
        </ThemeProvider>
      );
    },
  ],
};

export default preview;
