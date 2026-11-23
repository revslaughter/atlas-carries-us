// Colours for the canvas. CSS tokens in styles.css drive the page; the canvas
// reads them here. Region and line colours come from a fixed colour-blind-safe
// set based on Okabe–Ito, with no red and green pair. Every colour also has a
// second cue: a dash pattern, a shape, or a text label.

/** The theme's colour tokens, read from styles.css, by name: bg, surface, fg, muted, link and so on. */
export const T: Record<string, string> & { light?: boolean } = {};
/** The sans-serif font stack, from the --sans token. */
let sans = "system-ui, sans-serif",
  /** The monospace font stack, from the --mono token. */
  mono = "ui-monospace, monospace";

/** Reads one CSS custom property from the page. */
function cssVar(n: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(n).trim();
}

/** Reads the colour and font tokens of the theme in force; call it again after the theme changes. */
export function readTheme() {
  for (const k of ["bg", "surface", "surface-2", "line", "edge", "fg", "muted", "faint", "focus", "link"])
    T[k.replace("-2", "2")] = cssVar("--" + k);
  T.light = lum(T.bg) > 0.5;
  sans = cssVar("--sans") || sans;
  mono = cssVar("--mono") || mono;
}

/** Returns the sans-serif font stack. */
export const SANS = () => sans;
/** Returns the monospace font stack, for names and code. */
export const MONO = () => mono;

/** Returns a colour's perceived brightness, from 0 to 1, to tell a light theme from a dark one. */
function lum(hex: string): number {
  const n = parseInt((hex || "#000").replace("#", ""), 16);
  return (((n >> 16) & 255) * 0.299 + ((n >> 8) & 255) * 0.587 + (n & 255) * 0.114) / 255;
}

/** Turns a hex colour into an rgba() colour with alpha a. */
export function rgba(hex: string, a: number): string {
  const n = parseInt(hex.replace("#", ""), 16);
  return `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${a})`;
}

/** Region colours: [dark theme, light theme]. Each reaches 4.5:1 on its ground. */
const REGION: [string, string][] = [
  ["#56b4e9", "#1f6fa8"], // sky blue
  ["#e69f00", "#9a5a00"], // orange
  ["#b9a3e3", "#6a4fb3"], // lavender
  ["#f0e442", "#7a6b00"], // yellow
  ["#dd92bd", "#a3457a"], // pink
  ["#5b9cf0", "#2456a6"], // blue
  ["#a9bcd0", "#52697f"], // grey blue
  ["#e4e7ec", "#5f6670"], // light grey
  ["#9aa3af", "#5c6672"], // grey
];

/** Returns region colour i (0 to 8) for the theme in force, or the muted colour for none. */
export function regionColor(i: number | undefined): string {
  if (i === undefined) return T.muted;
  const c = REGION[((i % 9) + 9) % 9];
  return T.light ? c[1] : c[0];
}

/** How the map draws one kind of relation: two colours, a dash pattern and a verb. */
export interface EdgeStyle {
  /** The colour on the dark theme. */
  dark: string;
  /** The colour on the light theme. */
  light: string;
  /** The dash pattern; empty for a solid line. Colour never works alone. */
  dash: number[];
  /** The verb that the legend shows, such as "has a field of". */
  name: string;
}

/** Line styles. The name reads as a verb: "A calls B". */
export const EDGE: Record<string, EdgeStyle> = {
  calls: { dark: "#e9e2d6", light: "#3b2f27", dash: [], name: "calls" },
  implements: { dark: "#f0e442", light: "#7a6b00", dash: [12, 6], name: "implements" },
  extends: { dark: "#b9a3e3", light: "#6a4fb3", dash: [12, 4, 2, 4], name: "extends" },
  param: { dark: "#e69f00", light: "#9a5a00", dash: [], name: "takes as a parameter" },
  result: { dark: "#dd92bd", light: "#a3457a", dash: [7, 4], name: "returns" },
  field: { dark: "#56b4e9", light: "#1f6fa8", dash: [2, 5], name: "has a field of" },
  overrides: { dark: "#5b9cf0", light: "#2456a6", dash: [4, 3], name: "overrides" },
  uses: { dark: "#a9bcd0", light: "#52697f", dash: [], name: "uses" },
  runs: { dark: "#f0e442", light: "#7a6b00", dash: [12, 6], name: "runs as" },
};

/** Returns the colour of a relation's line for the theme in force. */
export function edgeColor(kind: string): string {
  const e = EDGE[kind] ?? EDGE.uses;
  return T.light ? e.light : e.dark;
}

/** Turns a token name ("fg", "muted") or a colour into a colour. */
export function color(c: string): string {
  return c.startsWith("#") || c.startsWith("rgb") ? c : T[c] || T.fg;
}
