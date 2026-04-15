// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
/**
 * Damerau-Levenshtein distance for typo tolerance in Cmd-K search.
 */

/**
 * Compute the Damerau-Levenshtein distance between two strings.
 * Supports insertions, deletions, substitutions, and transpositions.
 */
export function damerauLevenshtein(a: string, b: string): number {
  const la = a.length;
  const lb = b.length;
  if (la === 0) return lb;
  if (lb === 0) return la;

  // Use flat array for speed
  const d = new Uint16Array((la + 1) * (lb + 1));
  const w = lb + 1;

  for (let i = 0; i <= la; i++) d[i * w] = i;
  for (let j = 0; j <= lb; j++) d[j] = j;

  for (let i = 1; i <= la; i++) {
    for (let j = 1; j <= lb; j++) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      d[i * w + j] = Math.min(
        d[(i - 1) * w + j] + 1, // deletion
        d[i * w + (j - 1)] + 1, // insertion
        d[(i - 1) * w + (j - 1)] + cost, // substitution
      );
      // transposition
      if (i > 1 && j > 1 && a[i - 1] === b[j - 2] && a[i - 2] === b[j - 1]) {
        d[i * w + j] = Math.min(d[i * w + j], d[(i - 2) * w + (j - 2)] + cost);
      }
    }
  }
  return d[la * w + lb];
}

/**
 * Check if query approximately matches any word in text.
 * Returns a reduced score if a word-level edit distance <= maxDist, else null.
 */
export function typoMatch(query: string, text: string, maxDist = 2): number | null {
  const q = query.toLowerCase();
  const words = text
    .toLowerCase()
    .split(/[\s\-_./›]+/)
    .filter((w) => w.length > 0);

  let bestScore = Infinity;
  for (const word of words) {
    // Only compare if lengths are reasonably close
    if (Math.abs(word.length - q.length) > maxDist) continue;
    const dist = damerauLevenshtein(q, word);
    if (dist <= maxDist && dist < bestScore) {
      bestScore = dist;
    }
  }

  if (bestScore === Infinity) return null;
  // Lower distance = higher score. Max score for dist=1 is ~5, dist=2 is ~2
  return Math.max(1, 8 - bestScore * 3);
}
