import { hex2, type Span } from "./api";

export const ascii = (b: number) =>
  b >= 0x20 && b <= 0x7e ? String.fromCharCode(b) : "·";

export const frameHex = (bytes: number[]) => bytes.map(hex2).join(" ");

/** 后写的规则覆盖先写的，偏移按修改后的实际帧计算。 */
export function paint(len: number, spans: Span[]): (Span["kind"] | null)[] {
  const marks: (Span["kind"] | null)[] = new Array(len).fill(null);
  for (const span of spans) {
    for (let i = Math.max(0, span.start); i < span.start + span.len && i < len; i++) {
      marks[i] = span.kind;
    }
  }
  return marks;
}
