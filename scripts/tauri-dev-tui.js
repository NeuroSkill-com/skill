#!/usr/bin/env node

import { execSync, spawn } from "node:child_process";
import { createWriteStream, mkdirSync } from "node:fs";
import { platform } from "node:os";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const isWin = platform() === "win32";

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
let startupPhase = true;

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
  if (renderQueued || startupPhase) return;
  renderQueued = true;
  setTimeout(() => {
    renderQueued = false;
    if (!startupPhase) render();
  }, 16);
}

function getLayout() {
  const cols = Math.max(40, process.stdout.columns || 120);
  const rows = Math.max(18, process.stdout.rows || 40);
  const headerRows = 8;
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

  const artRaw = [
    "███╗   ██╗███████╗██╗   ██╗██████╗  ██████╗ ███████╗██╗  ██╗██╗██╗     ██╗",
    "████╗  ██║██╔════╝██║   ██║██╔══██╗██╔═══██╗██╔════╝██║ ██╔╝██║██║     ██║",
    "██╔██╗ ██║█████╗  ██║   ██║██████╔╝██║   ██║███████╗█████╔╝ ██║██║     ██║",
    "██║╚██╗██║██╔══╝  ██║   ██║██╔══██╗██║   ██║╚════██║██╔═██╗ ██║██║     ██║",
    "██║ ╚████║███████╗╚██████╔╝██║  ██║╚██████╔╝███████║██║  ██╗██║███████╗███████╗",
    "╚═╝  ╚═══╝╚══════╝ ╚═════╝ ╚═╝  ╚═╝ ╚═════╝ ╚══════╝╚═╝  ╚═╝╚═╝╚══════╝╚══════╝",
  ];
  const artMaxW = Math.max(...artRaw.map((l) => [...l].length));
  const art = artRaw.map((l, i) => {
    const padded = l + " ".repeat(Math.max(0, artMaxW - [...l].length));
    return i === 0 ? `${padded.slice(0, -1)}™` : padded;
  });

  process.stdout.write("\x1b[?25l\x1b[2J\x1b[H");

  // Gradient endpoints per row: left color → right color, fading vertically
  const gradientRows = [
    { l: [255, 0, 200],  r: [255, 80, 120] },   // vivid magenta → coral
    { l: [255, 20, 180],  r: [255, 100, 110] },
    { l: [245, 40, 160],  r: [250, 110, 100] },
    { l: [230, 55, 140],  r: [240, 115, 95] },
    { l: [210, 65, 125],  r: [225, 120, 90] },
    { l: [180, 80, 110],  r: [200, 120, 90] },   // dim mauve → muted coral
  ];

  function lerpColor(c1, c2, t) {
    return [
      Math.round(c1[0] + (c2[0] - c1[0]) * t),
      Math.round(c1[1] + (c2[1] - c1[1]) * t),
      Math.round(c1[2] + (c2[2] - c1[2]) * t),
    ];
  }

  function colorizeLineGradient(line, rowIdx) {
    const chars = [...line];
    const grad = gradientRows[Math.min(rowIdx, gradientRows.length - 1)];
    const len = chars.length;
    let out = "";
    for (let ci = 0; ci < len; ci++) {
      const t = len > 1 ? ci / (len - 1) : 0;
      const [r, g, b] = lerpColor(grad.l, grad.r, t);
      out += `\x1b[38;2;${r};${g};${b}m${chars[ci]}`;
    }
    return out + "\x1b[0m";
  }

  let row = 1;
  const RST = "\x1b[0m";
  const DIMPINK = "\x1b[38;2;140;70;100m";
  for (let i = 0; i < art.length; i++) {
    const line = art[i];
    const centered = center(line, cols);
    if (i === 0) {
      // ™ at the end in dim color
      const tmIdx = centered.lastIndexOf("™");
      const body = centered.slice(0, tmIdx);
      const colored = colorizeLineGradient(body, i) + `${DIMPINK}™${RST}`;
      process.stdout.write(`\x1b[${row};1H${fitText(colored, cols)}`);
    } else {
      process.stdout.write(`\x1b[${row};1H${fitText(colorizeLineGradient(centered, i), cols)}`);
    }
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
      // Unix: detached creates a process group so we can kill the tree with -pid.
      // Windows: detached opens a new console window — don't use it; taskkill /T
      // handles tree killing instead.
      detached: !isWin,
      windowsHide: true,
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
  if (isWin) {
    // Windows: use taskkill /T to kill the whole process tree.
    // process.kill(-pid) is not supported on Windows.
    try {
      const flag = signal === "SIGKILL" ? "/F" : "/F"; // always force on Windows
      execSync(`taskkill /T ${flag} /PID ${child.pid}`, { stdio: "ignore", timeout: 5000 });
    } catch {
      try {
        child.kill();
      } catch {
        /* ignore */
      }
    }
  } else {
    // Unix: kill the process group (negative PID).
    try {
      process.kill(-child.pid, signal);
    } catch {
      try {
        child.kill(signal);
      } catch {
        /* ignore */
      }
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
  try {
    process.stdout.write("\x1b[0m\x1b[?1000l\x1b[?1002l\x1b[?1006l\x1b[?25h\x1b[?1049l");
  } catch {
    /* terminal may already be gone */
  }
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
if (isWin) {
  // Windows doesn't have real POSIX signals. Catch SIGHUP (console close)
  // which Node.js does emulate on Windows.
  process.on("SIGHUP", () => cleanupAndExit(129));
}
process.on("exit", () => {
  if (!shuttingDown) {
    try {
      process.stdout.write("\x1b[0m\x1b[?1000l\x1b[?1002l\x1b[?1006l\x1b[?25h\x1b[?1049l");
    } catch {}
  }
});

if (!process.stdin.isTTY || !process.stdout.isTTY) {
  console.error("tauri-dev-tui requires an interactive TTY.");
  process.exit(1);
}

// Enable alternate screen buffer, mouse tracking, and hide cursor.
// On Windows Terminal these are fully supported; on legacy cmd.exe
// they are no-ops (harmless).
try {
  process.stdout.write("\x1b[?1049h\x1b[?1000h\x1b[?1002h\x1b[?1006h\x1b[2J\x1b[H");
} catch {
  // Ignore — terminal may not support these sequences.
}

try {
  process.stdin.setEncoding("utf8");
  process.stdin.setRawMode(true);
  process.stdin.resume();
} catch (e) {
  console.error(`Failed to set raw mode: ${e.message}`);
  process.exit(1);
}
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

// Startup flash: show logo centered for a moment, then transition to full TUI
function renderStartupFlash(step) {
  if (shuttingDown || !startupPhase) return;
  const cols = Math.max(40, process.stdout.columns || 120);
  const rows = Math.max(18, process.stdout.rows || 40);

  const artRaw = [
    "███╗   ██╗███████╗██╗   ██╗██████╗  ██████╗ ███████╗██╗  ██╗██╗██╗     ██╗",
    "████╗  ██║██╔════╝██║   ██║██╔══██╗██╔═══██╗██╔════╝██║ ██╔╝██║██║     ██║",
    "██╔██╗ ██║█████╗  ██║   ██║██████╔╝██║   ██║███████╗█████╔╝ ██║██║     ██║",
    "██║╚██╗██║██╔══╝  ██║   ██║██╔══██╗██║   ██║╚════██║██╔═██╗ ██║██║     ██║",
    "██║ ╚████║███████╗╚██████╔╝██║  ██║╚██████╔╝███████║██║  ██╗██║███████╗███████╗",
    "╚═╝  ╚═══╝╚══════╝ ╚═════╝ ╚═╝  ╚═╝ ╚═════╝ ╚══════╝╚═╝  ╚═╝╚═╝╚══════╝╚══════╝",
  ];

  // Brightness ramps up over steps 0-4, then holds
  const brightness = Math.min(1, step / 4);

  const gradientRows = [
    { l: [255, 0, 200],  r: [255, 80, 120] },
    { l: [255, 20, 180],  r: [255, 100, 110] },
    { l: [245, 40, 160],  r: [250, 110, 100] },
    { l: [230, 55, 140],  r: [240, 115, 95] },
    { l: [210, 65, 125],  r: [225, 120, 90] },
    { l: [180, 80, 110],  r: [200, 120, 90] },
  ];

  function lerpColor(c1, c2, t) {
    return [
      Math.round(c1[0] + (c2[0] - c1[0]) * t),
      Math.round(c1[1] + (c2[1] - c1[1]) * t),
      Math.round(c1[2] + (c2[2] - c1[2]) * t),
    ];
  }

  function scaleColor(c, s) {
    return [Math.round(c[0] * s), Math.round(c[1] * s), Math.round(c[2] * s)];
  }

  // Pad all lines to the same width so centering is uniform
  const artMaxW = Math.max(...artRaw.map((l) => [...l].length));
  const art = artRaw.map((l) => l + " ".repeat(Math.max(0, artMaxW - [...l].length)));

  process.stdout.write("\x1b[?25l\x1b[2J\x1b[H");

  const startRow = Math.max(1, Math.floor((rows - art.length) / 2));

  for (let i = 0; i < art.length; i++) {
    const line = art[i];
    const centered = center(line, cols);
    const chars = [...centered];
    const grad = gradientRows[Math.min(i, gradientRows.length - 1)];
    let out = "";
    for (let ci = 0; ci < chars.length; ci++) {
      const t = chars.length > 1 ? ci / (chars.length - 1) : 0;
      const base = lerpColor(grad.l, grad.r, t);
      const [r, g, b] = scaleColor(base, brightness);
      out += `\x1b[38;2;${r};${g};${b}m${chars[ci]}`;
    }
    out += "\x1b[0m";
    process.stdout.write(`\x1b[${startRow + i};1H${out}`);
  }

  if (step < 8) {
    setTimeout(() => renderStartupFlash(step + 1), 60);
  } else {
    // Hold the logo briefly, then transition to full TUI
    setTimeout(() => {
      startupPhase = false;
      try { render(); } catch { /* render will be retried by next output */ }
    }, 400);
  }
}

renderStartupFlash(0);

// Safety: ensure we always exit startup phase even if animation errors
setTimeout(() => {
  if (startupPhase) {
    startupPhase = false;
    try { render(); } catch { /* ignore */ }
  }
}, 2000);
