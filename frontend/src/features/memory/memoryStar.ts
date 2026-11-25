export const memoryStarGlyph: Record<string, string> = {
  fact: "◆", preference: "♡", note: "✦", knowledge: "▤",
};

export function versionRings(changes: number | undefined) {
  return Math.min(3, Math.max(0, changes ?? 0));
}
