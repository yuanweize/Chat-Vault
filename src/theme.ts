import { createContext, useContext } from "react";

export interface Theme {
  isDark: boolean;
  // backgrounds
  appBg: string;
  sidebarBg: string;
  cardBg: string;
  // borders
  border: string;
  divider: string;
  // text
  text: string;
  textSub: string;
  textMuted: string;
  // interactive
  hover: string;
  selectedBg: string;
  selectedText: string;
  // messages
  aiBubbleBg: string;
  // topbar
  topBarBg: string;
  // buttons
  btnBg: string;
  btnHoverBg: string;
}

export const lightTheme: Theme = {
  isDark: false,
  appBg: "#f4f6fa",
  sidebarBg: "#f7f8fb",
  cardBg: "#ffffff",
  border: "#dfe3ec",
  divider: "#e7eaf1",
  text: "#171a24",
  textSub: "#5c6475",
  textMuted: "#8a92a3",
  hover: "#eef1f7",
  selectedBg: "#e8ebff",
  selectedText: "#4f56c8",
  aiBubbleBg: "#f7f8fb",
  topBarBg: "#ffffff",
  btnBg: "#ffffff",
  btnHoverBg: "#eef1f7",
};

export const darkTheme: Theme = {
  isDark: true,
  appBg: "#0d1017",
  sidebarBg: "#121620",
  cardBg: "#10141d",
  border: "#282e3b",
  divider: "#232936",
  text: "#f1f3f8",
  textSub: "#a9b0bf",
  textMuted: "#747d90",
  hover: "#1c2230",
  selectedBg: "#252b54",
  selectedText: "#aeb4ff",
  aiBubbleBg: "#171c27",
  topBarBg: "#10141d",
  btnBg: "#171c27",
  btnHoverBg: "#202735",
};

export const ThemeContext = createContext<Theme>(lightTheme);
export const useTheme = () => useContext(ThemeContext);
