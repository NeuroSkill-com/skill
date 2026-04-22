export function normalizeMarkdown(raw: string): string {
  const sanitized = stripLeadingOrphanJsonFence(raw);
  const slots: string[] = [];

  const protectedText = sanitized
    // Preserve fenced code blocks verbatim.
    .replace(/```[\s\S]*?```/g, (block) => stash(block, slots))
    // Preserve inline code spans so emphasis repair does not rewrite examples.
    .replace(/`[^`\n]*`/g, (code) => stash(code, slots));

  const normalized = normalizeEmphasisRuns(normalizeEmphasisRuns(protectedText, "**", "strong"), "*", "em");

  return normalized.replace(/\uE000(\d+)\uE001/g, (_, index) => slots[Number(index)] ?? "");
}

function stash(text: string, slots: string[]): string {
  const id = slots.push(text) - 1;
  return `\uE000${id}\uE001`;
}

function stripLeadingOrphanJsonFence(raw: string): string {
  const open = raw.match(/^\s*```(?:json)?\s*\n/i);
  if (!open) return raw;

  const rest = raw.slice(open[0].length);
  if (/^```|\n```/m.test(rest)) return raw;

  const lines = rest.split("\n");
  let idx = 0;
  let sawJsonish = false;

  while (idx < lines.length) {
    const line = lines[idx].trim();

    if (!line) {
      idx += 1;
      if (sawJsonish) {
        const next = lines.slice(idx).find((candidate) => candidate.trim());
        if (next && !looksLikeJsonFragmentLine(next.trim())) break;
      }
      continue;
    }

    if (!looksLikeJsonFragmentLine(line)) break;
    sawJsonish = true;
    idx += 1;
  }

  if (!sawJsonish) return raw;

  return lines.slice(idx).join("\n").trimStart();
}

function looksLikeJsonFragmentLine(line: string): boolean {
  return (
    /^[{}[\],]+$/.test(line) ||
    /^"[^\n]*$/.test(line) ||
    /^[A-Za-z0-9_.$-]+"?\s*:\s*/.test(line) ||
    /^(true|false|null),?$/.test(line) ||
    /^-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?,?$/.test(line)
  );
}

function normalizeEmphasisRuns(text: string, delimiter: "**" | "*", tag: "strong" | "em"): string {
  const pattern =
    delimiter === "**"
      ? /(?<!\*)\*\*([^*\n][\s\S]*?[^*\n]|[^*\n])\*\*(?!\*)/g
      : /(?<!\*)\*([^*\n][\s\S]*?[^*\n]|[^*\n])\*(?!\*)/g;

  return text.replace(pattern, (match, inner: string, offset: number, source: string) => {
    const trimmed = inner.trim();
    if (!trimmed) return match;

    const left = source[offset - 1] ?? "";
    const right = source[offset + match.length] ?? "";
    const first = trimmed[0] ?? "";
    const last = trimmed[trimmed.length - 1] ?? "";

    const canOpen = delimiterCanOpen(left, first);
    const canClose = delimiterCanClose(last, right);

    if (!canOpen || !canClose) {
      return `<${tag}>${trimmed}</${tag}>`;
    }

    return `${delimiter}${trimmed}${delimiter}`;
  });
}

function delimiterCanOpen(prev: string, next: string): boolean {
  const leftFlanking = isLeftFlanking(prev, next);
  const rightFlanking = isRightFlanking(prev, next);
  return leftFlanking && (!rightFlanking || isPunctuation(prev));
}

function delimiterCanClose(prev: string, next: string): boolean {
  const leftFlanking = isLeftFlanking(prev, next);
  const rightFlanking = isRightFlanking(prev, next);
  return rightFlanking && (!leftFlanking || isPunctuation(next));
}

function isLeftFlanking(prev: string, next: string): boolean {
  return !isWhitespace(next) && (!isPunctuation(next) || isWhitespace(prev) || isPunctuation(prev));
}

function isRightFlanking(prev: string, next: string): boolean {
  return !isWhitespace(prev) && (!isPunctuation(prev) || isWhitespace(next) || isPunctuation(next));
}

function isWhitespace(char: string): boolean {
  return char === "" || /\s/u.test(char);
}

function isPunctuation(char: string): boolean {
  return char !== "" && /[\p{P}\p{S}]/u.test(char);
}
