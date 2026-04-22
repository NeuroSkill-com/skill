/**
 * Playwright e2e tests for the /chat page.
 *
 * Verifies that the chat page renders its sidebar, message area,
 * input bar, and settings panel without errors.
 *
 * Run:  npx playwright test src/tests/chat.spec.ts
 */
import { expect, type Page, test } from "@playwright/test";
import { buildDaemonMockScript, type CommandMap } from "./helpers/daemon-mock";

// ── Mock data ───────────────────────────────────────────────────────────────

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

const COMMANDS: CommandMap = {
  // Chat sessions
  list_chat_sessions: MOCK_SESSIONS,
  list_archived_chat_sessions: [],
  get_last_chat_session: { session_id: 1, messages: MOCK_MESSAGES },
  load_chat_session: { session_id: 1, messages: MOCK_MESSAGES },
  new_chat_session: { id: 99 },
  save_chat_message: 100,
  save_chat_tool_calls: null,
  rename_chat_session: null,
  delete_chat_session: null,
  archive_chat_session: null,
  unarchive_chat_session: null,
  get_session_params: "{}",

  // LLM server
  get_llm_config: {
    enabled: false,
    model: null,
    ctx_size: 4096,
    gpu_layers: 99,
    port: 11435,
  },
  get_llm_server_status: "stopped",
  get_llm_catalog: { families: [], models: [] },
  get_latest_bands: null,

  // Common
  show_main_window: null,
  show_toast_from_frontend: null,
  submit_label: null,
  open_settings_window: null,
  open_model_tab: null,
  get_settings: {},
  get_app_name: "NeuroSkill Test",
  get_ws_port: 8375,
  get_theme_and_language: ["dark", "en"],
};

// ── Helpers ──────────────────────────────────────────────────────────────────

async function openChat(page: Page) {
  await page.addInitScript({ content: buildDaemonMockScript(COMMANDS) });
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
