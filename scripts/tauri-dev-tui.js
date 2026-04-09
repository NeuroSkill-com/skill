#!/usr/bin/env node

import { spawn } from "node:child_process";
import { createWriteStream, mkdirSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = resolve(fileURLToPath(new URL(".", import.meta.url)));
const root = resolve(__dirname, "..");
const forwardedArgs = process.argv.slice(2);

const runId = new Date().toISOString().replace(/[:.]/g, "-");
const logDir = resolve(root, "logs", "dev", runId);
mkdirSync(logDir, { recursive: true });
const daemonLogPath = resolve(logDir, "daemon.log");
const tauriLogPath = resolve(logDir, "tauri.log");

const daemonLog = createWriteStream(daemonLogPath, { flags: "a" });
const tauriLog = createWriteStream(tauriLogPath, { flags: "a" });

const panes = {
  daemon: {
    name: "daemon",
    title: "Daemon",
    lines: [],
    partial: "",
    scrollBack: 0,
    follow: true,
    status: "starting",
  },
  tauri: {
    name: "tauri",
    title: "Tauri",
    lines: [],
    partial: "",
    scrollBack: 0,
    follow: true,
    status: "starting",
  },
};

let activePane = "tauri";
let daemonChild = null;
let tauriChild = null;
let shuttingDown = false;
let showHelp = false;
let renderQueued = false;
let forceKillTimer = null;

const ESC = String.fromCharCode(27);
// Strip all ANSI/VT escape sequences:
//   CSI:  ESC [ <params> <final-byte>   e.g. \x1b[1;32m  \x1b[K  \x1b[2J
//   OSC:  ESC ] <body> BEL              e.g. \x1b]8;;url\x07text\x1b]8;;\x07
//         ESC ] <body> ST               e.g. \x1b]8;;url\x1b\\text\x1b]8;;\x1b\\
// Modern cargo uses OSC 8 hyperlinks (\x1b]8;;…\x1b\\) which the old
// colour-only regex left intact, inflating visibleLength and garbling layout.
const ANSI_RE = new RegExp(
  `${ESC}(?:` +
    `\\[[0-9;]*[A-Za-z]` + // CSI sequence
    `|\\][^\\x07]*(?:\\x07|${ESC}\\\\)` + // OSC sequence (BEL or ST terminator)
    `)`,
  "g",
);

function stripAnsi(s) {
  return s.replace(ANSI_RE, "").replace(/\r/g, "");
}

function visibleLength(s) {
  return [...stripAnsi(s)].length;
}

function takeVisible(text, maxVisible) {
  if (maxVisible <= 0) return { text: "", hadAnsi: false, truncated: visibleLength(text) > 0 };

  let out = "";
  let visible = 0;
  let hadAnsi = false;
  let truncated = false;
  let idx = 0;

  for (const match of text.matchAll(ANSI_RE)) {
    const mIdx = match.index ?? 0;
    const plainChunk = text.slice(idx, mIdx);

    for (const ch of plainChunk) {
      if (visible >= maxVisible) {
        truncated = true;
        break;
      }
      out += ch;
      visible += 1;
    }
    if (truncated) break;

    // Keep CSI colour/style codes in output; drop OSC (hyperlinks etc.)
    const isOsc = match[0].charCodeAt(1) === 0x5d; // ESC ]
    if (!isOsc) {
      out += match[0];
      hadAnsi = true;
    }
    idx = mIdx + match[0].length;
  }

  if (!truncated) {
    const tail = text.slice(idx);
    for (const ch of tail) {
      if (visible >= maxVisible) {
        truncated = true;
        break;
      }
      out += ch;
      visible += 1;
    }
  }

  return { text: out, hadAnsi, truncated };
}

function fitText(text, width) {
  if (width <= 0) return "";

  const len = visibleLength(text);
  if (len < width) {
    return text + " ".repeat(width - len);
  }

  if (len === width) {
    return text;
  }

  if (width === 1) return "…";

  const taken = takeVisible(text, width - 1);
  const reset = taken.hadAnsi ? "\x1b[0m" : "";
  return `${taken.text}${reset}…`;
}

function center(text, width) {
  if (text.length >= width) return text.slice(0, width);
  const left = Math.floor((width - text.length) / 2);
  return `${" ".repeat(left)}${text}`;
}

function requestRender() {
  if (renderQueued) return;
  renderQueued = true;
  setTimeout(() => {
    renderQueued = false;
    render();
  }, 16);
}

function getLayout() {
  const cols = Math.max(40, process.stdout.columns || 120);
  const rows = Math.max(18, process.stdout.rows || 40);
  const headerRows = 7;
  const footerRows = 1;
  const contentTop = headerRows + 1;
  const contentBottom = rows - footerRows;
  const contentHeight = Math.max(3, contentBottom - contentTop + 1);
  const paneHeaderHeight = 1;
  const paneBodyHeight = Math.max(1, contentHeight - paneHeaderHeight);
  const leftWidth = Math.floor((cols - 1) / 2);
  const rightWidth = cols - leftWidth - 1;
  const dividerCol = leftWidth + 1;
  return {
    cols,
    rows,
    contentTop,
    contentBottom,
    paneBodyHeight,
    leftWidth,
    rightWidth,
    dividerCol,
  };
}

function paneViewportHeight() {
  return getLayout().paneBodyHeight;
}

function paneAtPosition(row, col) {
  const layout = getLayout();
  const inPaneBody = row >= layout.contentTop + 1 && row <= layout.contentBottom;
  if (!inPaneBody) return activePane;
  return col <= layout.leftWidth ? "daemon" : "tauri";
}

function clampScroll(pane, height) {
  const maxScrollBack = Math.max(0, pane.lines.length - height);
  pane.scrollBack = Math.min(maxScrollBack, Math.max(0, pane.scrollBack));
  if (pane.scrollBack === 0) pane.follow = true;
}

function pushChunk(pane, chunk, writer) {
  writer.write(chunk);
  const data = pane.partial + chunk.toString("utf8");
  const parts = data.split(/\n/);
  pane.partial = parts.pop() ?? "";
  for (const part of parts) {
    pane.lines.push(part);
  }
  if (pane.lines.length > 10000) {
    pane.lines.splice(0, pane.lines.length - 10000);
  }
  if (pane.follow) pane.scrollBack = 0;
}

function visibleLines(pane, height) {
  clampScroll(pane, height);
  const total = pane.lines.length;
  const start = Math.max(0, total - height - pane.scrollBack);
  return pane.lines.slice(start, start + height);
}

function paneStatusLine(pane, isActive) {
  const follow = pane.follow ? "follow" : `scroll +${pane.scrollBack}`;
  return `${isActive ? "▶" : " "} ${pane.title} [${pane.status}] (${pane.lines.length} lines · ${follow})`;
}

function render() {
  const { cols, rows, contentTop, contentBottom, paneBodyHeight, leftWidth, rightWidth, dividerCol } = getLayout();

  const art = [
    " _   _                        ____  _    _ _ _ ",
    "| \\ | | ___ _   _ _ __ ___   / ___|| | _(_) | |",
    "|  \\| |/ _ \\ | | | '__/ _ \\  \\___ \\| |/ / | | |",
    "| |\\  |  __/ |_| | | | (_) |  ___) |   <| | | |",
    "|_| \\_|\\___|\\__,_|_|  \\___/  |____/|_|\\_\\_|_|_|",
  ];

  process.stdout.write("\x1b[?25l\x1b[2J\x1b[H");

  let row = 1;
  for (const line of art) {
    process.stdout.write(`\x1b[${row};1H\x1b[95m${fitText(center(line, cols), cols)}\x1b[0m`);
    row += 1;
  }

  const logLine = `Logs: daemon=${daemonLogPath} | tauri=${tauriLogPath}`;
  process.stdout.write(`\x1b[${row};1H${fitText(logLine, cols)}`);
  row += 1;

  const helpLine = showHelp
    ? "Keys: Tab switch pane | ↑/k up | ↓/j down | PgUp/PgDn page | g top | G bottom | f follow | mouse wheel scroll | ? help | q quit"
    : "Press ? for key help (mouse wheel supported)";
  process.stdout.write(`\x1b[${row};1H${fitText(helpLine, cols)}`);

  for (let r = contentTop; r <= contentBottom; r++) {
    process.stdout.write(`\x1b[${r};${dividerCol}H│`);
  }

  const daemonHeader = paneStatusLine(panes.daemon, activePane === "daemon");
  const tauriHeader = paneStatusLine(panes.tauri, activePane === "tauri");
  const daemonHdrColor = activePane === "daemon" ? "\x1b[30;105m" : "\x1b[30;47m";
  const tauriHdrColor = activePane === "tauri" ? "\x1b[30;105m" : "\x1b[30;47m";
  process.stdout.write(`\x1b[${contentTop};1H${daemonHdrColor}${fitText(daemonHeader, leftWidth)}\x1b[0m`);
  process.stdout.write(
    `\x1b[${contentTop};${dividerCol + 1}H${tauriHdrColor}${fitText(tauriHeader, rightWidth)}\x1b[0m`,
  );

  const daemonVisible = visibleLines(panes.daemon, paneBodyHeight);
  const tauriVisible = visibleLines(panes.tauri, paneBodyHeight);

  for (let i = 0; i < paneBodyHeight; i++) {
    const r = contentTop + 1 + i;
    const left = daemonVisible[i] ?? "";
    const right = tauriVisible[i] ?? "";
    process.stdout.write(`\x1b[${r};1H${fitText(left, leftWidth)}`);
    process.stdout.write(`\x1b[${r};${dividerCol + 1}H${fitText(right, rightWidth)}`);
  }

  const footer = `Active: ${activePane} | daemon=${panes.daemon.status} | tauri=${panes.tauri.status}`;
  process.stdout.write(`\x1b[${rows};1H\x1b[37;100m${fitText(footer, cols)}\x1b[0m`);
}

function scrollPane(name, delta) {
  const pane = panes[name];
  const height = paneViewportHeight();
  const maxScrollBack = Math.max(0, pane.lines.length - height);
  pane.scrollBack = Math.min(maxScrollBack, Math.max(0, pane.scrollBack + delta));
  pane.follow = pane.scrollBack === 0;
}

function setBottom(name) {
  panes[name].scrollBack = 0;
  panes[name].follow = true;
}

function setTop(name) {
  const pane = panes[name];
  const height = paneViewportHeight();
  const maxScrollBack = Math.max(0, pane.lines.length - height);
  pane.scrollBack = maxScrollBack;
  pane.follow = maxScrollBack === 0;
}

function toggleFollow(name) {
  const pane = panes[name];
  pane.follow = !pane.follow;
  if (pane.follow) pane.scrollBack = 0;
}

function startProcess(role, pane, writer) {
  const child = spawn(
    process.execPath,
    [resolve(__dirname, "tauri-build.js"), "dev", `--__tui-pane-role=${role}`, ...forwardedArgs],
    {
      cwd: root,
      env: {
        ...process.env,
        SKILL_TAURI_TUI: "0",
        FORCE_COLOR: process.env.FORCE_COLOR || "1",
        CLICOLOR_FORCE: process.env.CLICOLOR_FORCE || "1",
        CARGO_TERM_COLOR: process.env.CARGO_TERM_COLOR || "always",
      },
      stdio: ["ignore", "pipe", "pipe"],
      detached: true,
    },
  );

  child.stdout.on("data", (chunk) => {
    pushChunk(pane, chunk, writer);
    requestRender();
  });
  child.stderr.on("data", (chunk) => {
    pushChunk(pane, chunk, writer);
    requestRender();
  });

  child.on("exit", (code, signal) => {
    pane.status = signal ? `signal ${signal}` : `exit ${code ?? 0}`;
    if (pane.partial) {
      pane.lines.push(pane.partial);
      pane.partial = "";
    }
    requestRender();
    maybeExit();
  });

  child.on("error", (err) => {
    pane.status = `error: ${err.message}`;
    requestRender();
    maybeExit();
  });

  return child;
}

function maybeExit() {
  if (!daemonChild || !tauriChild) return;
  if (daemonChild.exitCode === null || tauriChild.exitCode === null) return;
  cleanupAndExit(daemonChild.exitCode || tauriChild.exitCode || 0);
}

function killChildTree(child, signal = "SIGTERM") {
  if (!child || child.exitCode !== null || !child.pid) return;
  try {
    process.kill(-child.pid, signal);
  } catch {
    try {
      child.kill(signal);
    } catch {
      // ignore
    }
  }
}

function finalizeExit(code = 0) {
  if (forceKillTimer) {
    clearTimeout(forceKillTimer);
    forceKillTimer = null;
  }
  daemonLog.end();
  tauriLog.end();
  process.stdout.write("\x1b[0m\x1b[?1000l\x1b[?1002l\x1b[?1006l\x1b[?25h\x1b[?1049l");
  console.log(`\nLogs saved:\n  ${daemonLogPath}\n  ${tauriLogPath}`);
  process.exit(code);
}

function cleanupAndExit(code = 0) {
  if (shuttingDown) return;
  shuttingDown = true;

  try {
    if (process.stdin.isTTY) {
      process.stdin.setRawMode(false);
      process.stdin.pause();
    }
  } catch {
    // ignore
  }

  killChildTree(daemonChild, "SIGTERM");
  killChildTree(tauriChild, "SIGTERM");

  forceKillTimer = setTimeout(() => {
    killChildTree(daemonChild, "SIGKILL");
    killChildTree(tauriChild, "SIGKILL");
    finalizeExit(code);
  }, 1200);

  const done = () => {
    const daemonDone = !daemonChild || daemonChild.exitCode !== null;
    const tauriDone = !tauriChild || tauriChild.exitCode !== null;
    if (daemonDone && tauriDone) finalizeExit(code);
  };

  if (daemonChild && daemonChild.exitCode === null) daemonChild.once("exit", done);
  if (tauriChild && tauriChild.exitCode === null) tauriChild.once("exit", done);
  done();
}

process.on("SIGINT", () => cleanupAndExit(130));
process.on("SIGTERM", () => cleanupAndExit(143));
process.on("exit", () => {
  if (!shuttingDown) process.stdout.write("\x1b[0m\x1b[?1000l\x1b[?1002l\x1b[?1006l\x1b[?25h\x1b[?1049l");
});

if (!process.stdin.isTTY || !process.stdout.isTTY) {
  console.error("tauri-dev-tui requires an interactive TTY.");
  process.exit(1);
}

process.stdout.write("\x1b[?1049h\x1b[?1000h\x1b[?1002h\x1b[?1006h\x1b[2J\x1b[H");

process.stdin.setEncoding("utf8");
process.stdin.setRawMode(true);
process.stdin.resume();
process.stdin.on("data", (input) => {
  let handled = false;
  const mouseRe = new RegExp(`${ESC}\\[<(\\d+);(\\d+);(\\d+)([mM])`, "g");
  for (const match of input.matchAll(mouseRe)) {
    const code = Number(match[1]);
    const col = Number(match[2]);
    const row = Number(match[3]);
    const pane = paneAtPosition(row, col);
    if (code === 64) {
      scrollPane(pane, 3);
      handled = true;
    } else if (code === 65) {
      scrollPane(pane, -3);
      handled = true;
    }
  }
  const key = input.replace(mouseRe, "");

  if (key === "\u0003" || key === "q") {
    cleanupAndExit(0);
    return;
  }
  if (key === "\t") {
    activePane = activePane === "daemon" ? "tauri" : "daemon";
    handled = true;
  } else if (key === "\u001b[A" || key === "k") {
    scrollPane(activePane, 1);
    handled = true;
  } else if (key === "\u001b[B" || key === "j") {
    scrollPane(activePane, -1);
    handled = true;
  } else if (key === "\u001b[5~") {
    scrollPane(activePane, 15);
    handled = true;
  } else if (key === "\u001b[6~") {
    scrollPane(activePane, -15);
    handled = true;
  } else if (key === "g") {
    setTop(activePane);
    handled = true;
  } else if (key === "G") {
    setBottom(activePane);
    handled = true;
  } else if (key === "f") {
    toggleFollow(activePane);
    handled = true;
  } else if (key === "?") {
    showHelp = !showHelp;
    handled = true;
  }

  if (handled) requestRender();
});

process.stdout.on("resize", requestRender);

panes.daemon.status = "running";
panes.tauri.status = "running";

daemonChild = startProcess("daemon", panes.daemon, daemonLog);
tauriChild = startProcess("tauri", panes.tauri, tauriLog);

render();
