// SPDX-License-Identifier: GPL-3.0-only
import { describe, expect, it } from "vitest";
import type {
  ClipboardEventRow,
  EditChunkRow,
  FileInteractionRow,
  MeetingEventRow,
  ProductivityScore,
  SessionFileActivity,
  StaleFileRow,
  WeeklyDigest,
} from "$lib/daemon/settings";

describe("activity types", () => {
  it("FileInteractionRow includes undo_count", () => {
    const row: FileInteractionRow = {
      id: 1,
      file_path: "/main.rs",
      app_name: "Code",
      project: "skill",
      language: "rust",
      category: "code",
      git_branch: "main",
      seen_at: 1000,
      duration_secs: 60,
      was_modified: true,
      size_delta: 42,
      lines_added: 10,
      lines_removed: 3,
      words_delta: 5,
      eeg_focus: 72,
      eeg_mood: null,
      undo_count: 2,
    };
    expect(row.undo_count).toBe(2);
  });

  it("EditChunkRow includes undo_estimate", () => {
    const chunk: EditChunkRow = {
      id: 1,
      interaction_id: 1,
      chunk_at: 1000,
      lines_added: 5,
      lines_removed: 2,
      size_delta: 30,
      undo_estimate: 1,
    };
    expect(chunk.undo_estimate).toBe(1);
  });

  it("MeetingEventRow has platform and optional end_at", () => {
    const mtg: MeetingEventRow = {
      id: 1,
      platform: "zoom",
      title: "Standup",
      app_name: "zoom.us",
      start_at: 1000,
      end_at: 2000,
    };
    expect(mtg.platform).toBe("zoom");
    expect(mtg.end_at).toBe(2000);

    const openMtg: MeetingEventRow = { ...mtg, end_at: null };
    expect(openMtg.end_at).toBeNull();
  });

  it("ClipboardEventRow has content_type", () => {
    const ev: ClipboardEventRow = {
      id: 1,
      source_app: "Safari",
      content_type: "text",
      content_size: 42,
      copied_at: 1000,
    };
    expect(ev.content_type).toBe("text");
  });

  it("ProductivityScore has all 4 components", () => {
    const score: ProductivityScore = {
      day_start: 1000,
      score: 75,
      edit_velocity: 20,
      deep_work: 22,
      context_stability: 18,
      eeg_focus: 15,
      deep_work_minutes: 90,
      switch_rate: 2.5,
    };
    expect(score.score).toBe(75);
    expect(score.edit_velocity + score.deep_work + score.context_stability + score.eeg_focus).toBe(75);
  });

  it("WeeklyDigest has days array and peak info", () => {
    const digest: WeeklyDigest = {
      week_start: 1000,
      days: [],
      total_interactions: 100,
      total_edits: 50,
      total_secs: 36000,
      total_lines_added: 500,
      total_lines_removed: 200,
      avg_eeg_focus: 65,
      top_projects: [],
      top_languages: [],
      focus_session_count: 5,
      meeting_count: 3,
      peak_day_idx: 2,
      peak_hour: 10,
    };
    expect(digest.peak_hour).toBe(10);
    expect(digest.days).toHaveLength(0);
  });

  it("SessionFileActivity has files, sessions, and meetings", () => {
    const activity: SessionFileActivity = {
      files: [],
      focus_sessions: [],
      meetings: [],
    };
    expect(activity.files).toHaveLength(0);
    expect(activity.meetings).toHaveLength(0);
  });

  it("StaleFileRow has days_stale", () => {
    const stale: StaleFileRow = {
      file_path: "/old.rs",
      last_seen: 1000,
      total_edits: 5,
      project: "proj",
      language: "rust",
      days_stale: 14,
    };
    expect(stale.days_stale).toBe(14);
  });
});
