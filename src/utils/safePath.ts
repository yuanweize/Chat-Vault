export function isSafePathComponent(value: string): boolean {
  const trimmed = value.trim();
  return trimmed.length > 0
    && trimmed.length <= 255
    && trimmed !== "."
    && trimmed !== ".."
    && !/[\\/\u0000]/.test(trimmed);
}
