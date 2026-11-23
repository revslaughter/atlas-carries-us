// Tests of the text helpers and of the button.

import { render } from "@testing-library/svelte";
import { describe, expect, it } from "vitest";

import Button from "./Button.svelte";
import { plural, slugTitle } from "./format.js";

describe("format", () => {
  it("writes the plural", () => {
    expect(plural(2, "press")).toBe("2 presses");
  });
  it("titles a slug", () => {
    expect(slugTitle("first-light")).toBe("First light");
  });
});

describe("Button", () => {
  it("shows its label", () => {
    render(Button, { props: { label: "Save" } });
  });
});
