/**
 * 运行时形状校验 — 极简版,无 zod.
 *
 * 用法:
 *   assertShape("listThreads", payload, {
 *     id: "string",
 *     title: "string",
 *     updatedAt: "string",
 *     messageCount: "number",
 *     isStreaming: "boolean",
 *     isPinned: "boolean",
 *     preview: "string?",   // ? 表示可空 (null 或 undefined 也算通过)
 *   });
 *
 * 失败时:
 *   - dev: console.warn + 上报到 devtools.recordShapeMismatch
 *   - prod: 静默 (避免污染线上日志)
 *
 * 不抛错,不打断流程 — 让 UI 继续渲染部分数据,而不是白屏.
 */

import { devtools } from "./devtools";

export type ShapeSpec = Record<string, ShapeFieldType>;

export type ShapeFieldType =
  | "string"
  | "string?"
  | "number"
  | "number?"
  | "boolean"
  | "boolean?"
  | "object"
  | "object?"
  | "array"
  | "array?"
  | "any";

export interface ShapeMismatch {
  endpoint: string;
  field: string;
  expected: ShapeFieldType;
  actual: string; // typeof 结果
}

export function assertShape(
  endpoint: string,
  value: unknown,
  spec: ShapeSpec,
): ShapeMismatch[] {
  const mismatches: ShapeMismatch[] = [];
  if (value === null || typeof value !== "object") {
    mismatches.push({
      endpoint,
      field: "<root>",
      expected: "object",
      actual: actualType(value),
    });
    report(endpoint, mismatches);
    return mismatches;
  }
  const obj = value as Record<string, unknown>;
  for (const field of Object.keys(spec)) {
    const expected = spec[field];
    const actual = obj[field];
    if (!matches(actual, expected)) {
      mismatches.push({
        endpoint,
        field,
        expected,
        actual: actualType(actual),
      });
    }
  }
  if (mismatches.length > 0) report(endpoint, mismatches);
  return mismatches;
}

/** 数组成员校验. */
export function assertArrayShape(
  endpoint: string,
  value: unknown,
  itemSpec: ShapeSpec,
): ShapeMismatch[] {
  if (!Array.isArray(value)) {
    const m: ShapeMismatch[] = [
      { endpoint, field: "<root>", expected: "array", actual: actualType(value) },
    ];
    report(endpoint, m);
    return m;
  }
  const all: ShapeMismatch[] = [];
  for (let i = 0; i < value.length; i++) {
    const m = assertShape(`${endpoint}[${i}]`, value[i], itemSpec);
    all.push(...m);
  }
  return all;
}

// ── helpers ───────────────────────────────────────────────────────────────

function matches(value: unknown, spec: ShapeFieldType): boolean {
  if (spec === "any") return true;
  const optional = spec.endsWith("?");
  const base = (optional ? spec.slice(0, -1) : spec) as Exclude<
    ShapeFieldType,
    "any"
  >;
  if (optional && (value === null || value === undefined)) return true;
  switch (base) {
    case "string":
      return typeof value === "string";
    case "number":
      return typeof value === "number" && Number.isFinite(value);
    case "boolean":
      return typeof value === "boolean";
    case "object":
      return value !== null && typeof value === "object" && !Array.isArray(value);
    case "array":
      return Array.isArray(value);
  }
  return false;
}

function actualType(value: unknown): string {
  if (value === null) return "null";
  if (Array.isArray(value)) return "array";
  return typeof value;
}

function report(endpoint: string, mismatches: ShapeMismatch[]) {
  if (!import.meta.env.DEV) return;
  // eslint-disable-next-line no-console
  console.warn(`[ipc] shape mismatch in ${endpoint}:`, mismatches);
  devtools.recordShapeMismatch(endpoint, mismatches);
}
