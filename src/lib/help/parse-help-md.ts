// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.

/** Parsed help item (title + body pair). */
export interface HelpEntry {
  id: string;
  title: string;
  body: string;
}

/** A section containing a title, optional description, and child items. */
export interface HelpSectionData {
  title: string;
  description: string;
  items: HelpEntry[];
}

/** A single FAQ pair. */
export interface FaqEntry {
  id: string;
  question: string;
  answer: string;
}

/** Slugify a heading into a stable id. */
function slugify(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");
}

/**
 * Parse a help markdown file into sections.
 *
 * Convention:
 *   # Section Title        → new section
 *   Description text       → section description (text before first ##)
 *   ## Item Title           → new item within current section
 *   Body text              → item body (all text until next heading)
 */
export function parseHelpMd(raw: string): HelpSectionData[] {
  const lines = raw.split("\n");
  const sections: HelpSectionData[] = [];
  let current: HelpSectionData | null = null;
  let currentItem: HelpEntry | null = null;
  let buf: string[] = [];

  function flushBuf() {
    const text = buf.join("\n").trim();
    buf = [];
    return text;
  }

  for (const line of lines) {
    if (line.startsWith("# ") && !line.startsWith("## ")) {
      // Flush previous item
      if (currentItem) {
        currentItem.body = flushBuf();
        currentItem = null;
      } else if (current) {
        // Text before first ## is section description
        const desc = flushBuf();
        if (desc && !current.description) current.description = desc;
      }
      // Start new section
      const title = line.slice(2).trim();
      current = { title, description: "", items: [] };
      sections.push(current);
    } else if (line.startsWith("## ")) {
      // Flush previous item
      if (currentItem) {
        currentItem.body = flushBuf();
      } else if (current) {
        const desc = flushBuf();
        if (desc && !current.description) current.description = desc;
      }
      // Start new item
      const title = line.slice(3).trim();
      currentItem = { id: slugify(title), title, body: "" };
      current?.items.push(currentItem);
    } else {
      buf.push(line);
    }
  }

  // Flush trailing content
  if (currentItem) {
    currentItem.body = flushBuf();
  } else if (current) {
    const desc = flushBuf();
    if (desc && !current.description) current.description = desc;
  }

  return sections;
}

/**
 * Parse a FAQ markdown file into question/answer pairs.
 *
 * Convention:
 *   ## Question text?
 *   Answer text (everything until next ##)
 */
export function parseFaqMd(raw: string): FaqEntry[] {
  const lines = raw.split("\n");
  const entries: FaqEntry[] = [];
  let current: FaqEntry | null = null;
  let buf: string[] = [];

  for (const line of lines) {
    if (line.startsWith("## ")) {
      if (current) {
        current.answer = buf.join("\n").trim();
      }
      const question = line.slice(3).trim();
      current = { id: slugify(question), question, answer: "" };
      entries.push(current);
      buf = [];
    } else {
      buf.push(line);
    }
  }

  if (current) {
    current.answer = buf.join("\n").trim();
  }

  return entries;
}
