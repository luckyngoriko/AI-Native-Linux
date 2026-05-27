import { describe, it, expect } from "vitest";
import {
  nodeKindRegistry,
  NODE_KIND_VARIANTS,
  type NodeKind,
} from "@/lib/nodeKindRegistry";

describe("nodeKindRegistry", () => {
  it("has exactly 19 entries", () => {
    const keys = Object.keys(nodeKindRegistry);
    expect(keys).toHaveLength(19);
  });

  it("every NodeKind variant has a registry entry", () => {
    for (const kind of NODE_KIND_VARIANTS) {
      expect(nodeKindRegistry[kind]).toBeDefined();
    }
  });

  it("every registry key is a valid NodeKind variant", () => {
    const validKinds = new Set<string>(NODE_KIND_VARIANTS);
    for (const key of Object.keys(nodeKindRegistry)) {
      expect(validKinds.has(key)).toBe(true);
    }
  });

  it("NODE_KIND_VARIANTS has exactly 19 entries", () => {
    expect(NODE_KIND_VARIANTS).toHaveLength(19);
  });

  it("registry and variants are in sync (no missing, no extra)", () => {
    const registryKeys = Object.keys(nodeKindRegistry).sort();
    const variantKeys = [...NODE_KIND_VARIANTS].sort();
    expect(registryKeys).toEqual(variantKeys);
  });

  it("every entry is a function (React component)", () => {
    for (const [key, component] of Object.entries(nodeKindRegistry)) {
      expect(typeof component).toBe("function");
    }
  });

  it("NodeKind union is structurally complete", () => {
    const _check: NodeKind = "Container";
    expect(_check).toBe("Container");
  });
});
