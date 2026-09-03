declare module "react-syntax-highlighter" {
  import type { CSSProperties, ComponentType, ReactNode } from "react";

  type SyntaxHighlighterProps = {
    children: ReactNode;
    language: string;
    PreTag?: "div" | "pre";
    style?: Record<string, CSSProperties>;
    wrapLongLines?: boolean;
  };

  export const Prism: ComponentType<SyntaxHighlighterProps>;
}
