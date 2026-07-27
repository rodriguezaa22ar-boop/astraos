import { describe, expect, it } from "vitest";

import { renderMessage } from "./index.js";

describe("renderMessage", () => {
  it("returns the fixture message", () => {
    expect(renderMessage()).toBe("fixture");
  });
});
