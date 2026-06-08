/**
 * 极简 className 合并工具 (clsx 替代品,零依赖).
 * 接受字符串/false/undefined,过滤空值后用空格连接.
 */
export type ClassValue =
  | string
  | number
  | false
  | null
  | undefined
  | ClassValue[];

export function cn(...inputs: ClassValue[]): string {
  const out: string[] = [];
  for (const item of inputs) {
    if (!item) continue;
    if (typeof item === "string" || typeof item === "number") {
      out.push(String(item));
    } else if (Array.isArray(item)) {
      const nested = cn(...item);
      if (nested) out.push(nested);
    }
  }
  return out.join(" ");
}
