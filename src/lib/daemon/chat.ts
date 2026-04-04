// SPDX-License-Identifier: GPL-3.0-only

import { daemonGet, daemonPost } from "./http";

export interface ChatSessionSummary {
  id: number;
  title: string;
  preview: string;
  created_at: number;
  message_count: number;
}

export function listChatSessions(): Promise<ChatSessionSummary[]> {
  return daemonGet<ChatSessionSummary[]>("/v1/llm/chat/sessions");
}

export function listArchivedChatSessions(): Promise<ChatSessionSummary[]> {
  return daemonGet<ChatSessionSummary[]>("/v1/llm/chat/archived-sessions");
}

export async function renameChatSession(id: number, title: string): Promise<void> {
  await daemonPost("/v1/llm/chat/rename", { id, title });
}

export async function deleteChatSession(id: number): Promise<void> {
  await daemonPost("/v1/llm/chat/delete", { id });
}

export async function archiveChatSession(id: number): Promise<void> {
  await daemonPost("/v1/llm/chat/archive", { id });
}

export async function unarchiveChatSession(id: number): Promise<void> {
  await daemonPost("/v1/llm/chat/unarchive", { id });
}

export async function cancelToolCall(toolCallId: string): Promise<void> {
  await daemonPost("/v1/llm/cancel-tool-call", { tool_call_id: toolCallId });
}
