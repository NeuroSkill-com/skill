#!/usr/bin/env node

import fs from 'node:fs/promises';
import path from 'node:path';

const ROOT = process.cwd();
const TARGET = path.join(ROOT, 'src', 'lib', 'MarkdownRenderer.svelte');

function fail(message) {
  console.error(`[check-markdown-renderer] ${message}`);
  process.exit(1);
}

async function main() {
  let source = '';
  try {
    source = await fs.readFile(TARGET, 'utf8');
  } catch (error) {
    fail(`Unable to read ${TARGET}: ${String(error)}`);
  }

  if (/\bnew\s+Marked\s*\(/.test(source)) {
    fail('Found `new Marked(...)` in MarkdownRenderer.svelte. Use `marked.parse(...)` with a `Renderer` instance to avoid Tailwind parser regressions.');
  }

  if (/<style(?:\s[^>]*)?>[\s\S]*?<\/style>/i.test(source)) {
    fail('Found a local `<style>` block in MarkdownRenderer.svelte. Keep styles in src/app.css (`.mdr*`) to avoid Tailwind parser regressions.');
  }

  console.log('[check-markdown-renderer] OK');
}

main();