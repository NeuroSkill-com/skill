/**
 * Playwright e2e tests for the /chat page.
 *
 * Verifies that the chat page renders its sidebar, message area,
 * input bar, and settings panel without errors.
 *
 * Run:  npx playwright test src/tests/chat.spec.ts
 */
import { expect, type Page, test } from "@playwright/test";

// ── Tauri IPC mock ───────────────────────────────────────────────────────────

const MOCK_SESSIONS = [
  { id: 1, title: "Hello World", preview: "First conversation", created_at: 1711756800, message_count: 4 },
  { id: 2, title: "EEG Analysis", preview: "Let me analyze your brain data", created_at: 1711843200, message_count: 8 },
  { id: 3, title: "Focus Tips", preview: "Here are some tips for focus", created_at: 1711929600, message_count: 2 },
];

const MOCK_MESSAGES = [
  {
    id: 1,
    session_id: 1,
    role: "user",
    content: "Hello! Can you help me?",
    thinking: null,
    created_at: 1711756800,
    tool_calls: [],
  },
  {
    id: 2,
    session_id: 1,
    role: "assistant",
    content: "Of course! I'm here to help. What would you like to know?",
    thinking: null,
    created_at: 1711756801,
    tool_calls: [],
  },
  {
    id: 3,
    session_id: 1,
    role: "user",
    content: "What is neurofeedback?",
    thinking: null,
    created_at: 1711756802,
    tool_calls: [],
  },
  {
    id: 4,
    session_id: 1,
    role: "assistant",
    content:
      "Neurofeedback is a type of biofeedback that uses real-time displays of brain activity — typically EEG — to teach self-regulation of brain function. It's used for attention, relaxation, and cognitive performance.",
    thinking: null,
    created_at: 1711756803,
    tool_calls: [],
  },
];

function buildMockScript() {
  const sessions = JSON.stringify(MOCK_SESSIONS);
  const messages = JSON.stringify(MOCK_MESSAGES);

  return `
    window.__TAURI_INTERNALS__ = window.__TAURI_INTERNALS__ || {};
    window.__TAURI_INTERNALS__.metadata = {
      currentWindow: { label: "main" },
      currentWebview: { label: "main", windowLabel: "main" },
      windows: [{ label: "main" }],
      webviews: [{ label: "main", windowLabel: "main" }],
    };

    const SESSIONS = ${sessions};
    const MESSAGES = ${messages};

    window.__TAURI_INTERNALS__.invoke = function(cmd, args) {
      switch (cmd) {
        // ── Chat sessions ────────────────────────────────────────────────
        case "list_chat_sessions":
          return Promise.resolve(SESSIONS);
        case "list_archived_chat_sessions":
          return Promise.resolve([]);
        case "get_last_chat_session":
          return Promise.resolve({ session_id: 1, messages: MESSAGES });
        case "load_chat_session":
          return Promise.resolve({ session_id: args?.sessionId ?? 1, messages: MESSAGES });
        case "new_chat_session":
          return Promise.resolve({ id: 99 });
        case "save_chat_message":
          return Promise.resolve(100);
        case "save_chat_tool_calls":
          return Promise.resolve();
        case "rename_chat_session":
        case "delete_chat_session":
        case "archive_chat_session":
        case "unarchive_chat_session":
          return Promise.resolve();
        case "get_session_params":
          return Promise.resolve("{}");

        // ── LLM server ───────────────────────────────────────────────────
        case "get_llm_config":
          return Promise.resolve({
            enabled: false,
            model: null,
            ctx_size: 4096,
            gpu_layers: 99,
            port: 11435,
          });
        case "get_llm_server_status":
          return Promise.resolve("stopped");
        case "get_llm_catalog":
          return Promise.resolve({ families: [], models: [] });
        case "get_latest_bands":
          return Promise.resolve(null);

        // ── Common ───────────────────────────────────────────────────────
        case "show_main_window":
        case "show_toast_from_frontend":
        case "submit_label":
        case "open_settings_window":
        case "open_model_tab":
          return Promise.resolve();
        case "get_settings":
          return Promise.resolve({});
        case "get_app_name":
          return Promise.resolve("NeuroSkill Test");
        case "get_ws_port":
          return Promise.resolve(8375);
        case "plugin:event|listen":
          return Promise.resolve(0);
        case "plugin:event|unlisten":
          return Promise.resolve();
        default:
          return Promise.resolve(null);
      }
    };
  `;
}

// ── Helpers ──────────────────────────────────────────────────────────────────

async function openChat(page: Page) {
  await page.addInitScript({ content: buildMockScript() });
  await page.goto("http://localhost:1420/chat", { waitUntil: "networkidle" });
  await page.waitForTimeout(1500);
}

// ── Tests ────────────────────────────────────────────────────────────────────

test.describe("Chat page", () => {
  test("renders with sidebar and message area", async ({ page }) => {
    await openChat(page);

    // Sidebar should show session titles
    await expect(page.locator("text=/Hello Worl/").first()).toBeVisible({ timeout: 5000 });

    await page.screenshot({ path: "test-results/chat-main.png" });
  });

  test("displays chat messages", async ({ page }) => {
    await openChat(page);

    // Should show the mock conversation
    await expect(page.locator("text=/Can you help me/").first()).toBeVisible({ timeout: 5000 });
    await expect(page.locator("text=/here to help/i").first()).toBeVisible();
  });

  test("shows input bar", async ({ page }) => {
    await openChat(page);

    // Input area — textarea or contenteditable
    const input = page.locator("textarea, [contenteditable=true], [role=textbox]").first();
    await expect(input).toBeVisible({ timeout: 5000 });
  });

  test("sidebar shows all sessions", async ({ page }) => {
    await openChat(page);

    await expect(page.locator("text=/Hello Worl/").first()).toBeVisible({ timeout: 5000 });
    await expect(page.locator("text=/EEG Analy/").first()).toBeVisible();
    await expect(page.locator("text=Focus Tips").first()).toBeVisible();
  });

  test("can click a different session", async ({ page }) => {
    await openChat(page);

    const session = page.locator("text=/EEG Analy/").first();
    await expect(session).toBeVisible({ timeout: 5000 });
    await session.click();
    await page.waitForTimeout(500);

    await page.screenshot({ path: "test-results/chat-switch-session.png" });
  });

  test("new chat button exists", async ({ page }) => {
    await openChat(page);

    // Look for new chat button (usually a + icon or "New" text)
    const newBtn = page
      .locator("button")
      .filter({ hasText: /new|create/i })
      .first();
    const plusBtn = page.locator('[aria-label*="new" i], [title*="new" i], [aria-label*="New"]').first();
    const hasNewChat = (await newBtn.isVisible()) || (await plusBtn.isVisible());
    expect(hasNewChat).toBe(true);
  });
});
