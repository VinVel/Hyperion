import type { StorybookConfig } from "@storybook/react-vite";

const config = {
  stories: ["../src/components/storybook/**/*.stories.@(ts|tsx|mdx)"],
  addons: ["@storybook/addon-docs"],
  framework: "@storybook/react-vite",
  staticDirs: ["../public"],
} satisfies StorybookConfig;

export default config;
