// SPDX-License-Identifier: GPL-3.0-only

//! Command-group permission system for iroh clients.
//!
//! Every WS/REST command belongs to exactly one [`CommandGroup`].  Built-in
//! scopes (`read`, `full`) expand to a fixed set of groups.  The `custom`
//! scope allows per-client group and per-command allow/deny lists.

use serde::{Deserialize, Serialize};

// ── Command groups ───────────────────────────────────────────────────────────

/// A named set of WS/REST commands.
#[derive(Clone, Debug, Serialize)]
pub struct CommandGroup {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    /// If true, UI renders a danger banner when this group is enabled.
    pub dangerous: bool,
    pub commands: &'static [&'static str],
}

pub const GROUPS: &[CommandGroup] = &[
    // ── Safe / read-oriented ─────────────────────────────────────────────
    CommandGroup {
        id: "status",
        label: "Status & Sessions",
        description: "View device status, sessions, and sleep data.",
        dangerous: false,
        commands: &["status", "sessions", "sleep"],
    },
    CommandGroup {
        id: "search",
        label: "Search & Browse",
        description: "Search EEG embeddings, labels, and run comparisons.",
        dangerous: false,
        commands: &[
            "search",
            "search_labels",
            "interactive_search",
            "compare",
            "session_metrics",
        ],
    },
    CommandGroup {
        id: "screenshots",
        label: "Screenshots",
        description: "Search and browse screenshots, cross-reference with EEG.",
        dangerous: false,
        commands: &[
            "search_screenshots",
            "screenshots_around",
            "search_screenshots_vision",
            "search_screenshots_by_image_b64",
            "screenshots_for_eeg",
            "eeg_for_screenshots",
        ],
    },
    CommandGroup {
        id: "calendar",
        label: "Calendar",
        description: "Read calendar events and check permission status.",
        dangerous: false,
        commands: &["calendar_events", "calendar_status"],
    },
    CommandGroup {
        id: "health_read",
        label: "Health (Read)",
        description: "Query stored HealthKit data and summaries.",
        dangerous: false,
        commands: &["health_query", "health_summary", "health_metric_types"],
    },
    CommandGroup {
        id: "iroh_view",
        label: "Remote Access (View)",
        description: "View tunnel status, credentials, and connected clients.",
        dangerous: false,
        commands: &[
            "iroh_info",
            "iroh_totp_list",
            "iroh_clients_list",
            "iroh_scope_groups",
            "iroh_client_permissions",
        ],
    },
    CommandGroup {
        id: "llm_view",
        label: "LLM (View)",
        description: "Check LLM status, catalog, and logs.",
        dangerous: false,
        commands: &[
            "llm_status",
            "llm_catalog",
            "llm_logs",
            "llm_downloads",
            "llm_hardware_fit",
        ],
    },
    CommandGroup {
        id: "umap",
        label: "UMAP",
        description: "Enqueue and poll 3D UMAP projection jobs.",
        dangerous: false,
        commands: &["umap", "umap_poll"],
    },
    CommandGroup {
        id: "calibration_view",
        label: "Calibrations (View)",
        description: "List and inspect calibration profiles.",
        dangerous: false,
        commands: &["list_calibrations", "get_calibration"],
    },
    // ── Write / mutating ─────────────────────────────────────────────────
    CommandGroup {
        id: "labels",
        label: "Labels (Write)",
        description: "Create labels on the current EEG timeline.",
        dangerous: false,
        commands: &["label"],
    },
    CommandGroup {
        id: "health_write",
        label: "Health (Write)",
        description: "Push HealthKit data from an iOS companion app.",
        dangerous: false,
        commands: &["health_sync"],
    },
    CommandGroup {
        id: "calendar_write",
        label: "Calendar (Write)",
        description: "Request calendar access permission on the host machine.",
        dangerous: false,
        commands: &["calendar_request_permission"],
    },
    // ── Dangerous ────────────────────────────────────────────────────────
    CommandGroup {
        id: "device_control",
        label: "Device Control",
        description: "Trigger calibration, timers, notifications, TTS, and DND. \
                       Can disrupt active recording sessions and change system state.",
        dangerous: true,
        commands: &[
            "calibrate",
            "run_calibration",
            "timer",
            "notify",
            "say",
            "dnd",
            "dnd_set",
            "sleep_schedule",
            "sleep_schedule_set",
        ],
    },
    CommandGroup {
        id: "calibration_write",
        label: "Calibrations (Manage)",
        description: "Create, modify, and delete calibration profiles. \
                       Deleting a profile is irreversible.",
        dangerous: true,
        commands: &["create_calibration", "update_calibration", "delete_calibration"],
    },
    CommandGroup {
        id: "hooks",
        label: "Hooks & Automation",
        description: "View, create, and modify automation hooks. \
                       Hooks can execute shell commands and external scripts on the host.",
        dangerous: true,
        commands: &["hooks_get", "hooks_set", "hooks_status", "hooks_suggest", "hooks_log"],
    },
    CommandGroup {
        id: "llm_control",
        label: "LLM (Manage)",
        description: "Start/stop the LLM engine, download and delete models. \
                       Can use significant CPU/GPU/disk resources.",
        dangerous: true,
        commands: &[
            "llm_start",
            "llm_stop",
            "llm_download",
            "llm_cancel_download",
            "llm_delete",
            "llm_select_model",
            "llm_select_mmproj",
            "llm_pause_download",
            "llm_resume_download",
            "llm_refresh_catalog",
            "llm_set_autoload_mmproj",
            "llm_add_model",
        ],
    },
    CommandGroup {
        id: "iroh_admin",
        label: "Remote Access (Admin)",
        description: "Create and revoke credentials, register and remove clients, \
                       change scopes. A client with this group can lock out other \
                       clients or escalate its own permissions.",
        dangerous: true,
        commands: &[
            "iroh_totp_create",
            "iroh_totp_qr",
            "iroh_totp_revoke",
            "iroh_client_register",
            "iroh_client_revoke",
            "iroh_client_set_scope",
            "iroh_phone_invite",
        ],
    },
];

// ── Built-in scope definitions ───────────────────────────────────────────────

/// Groups included in the `"read"` scope.
pub const READ_GROUPS: &[&str] = &[
    "status",
    "search",
    "screenshots",
    "calendar",
    "health_read",
    "iroh_view",
    "llm_view",
    "umap",
    "calibration_view",
];

// `"full"` scope means every command is allowed — no group list needed.

// ── Scope data stored per client ─────────────────────────────────────────────

/// Granular permission overrides stored alongside each client entry.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ClientScope {
    /// `"read"`, `"full"`, or `"custom"`.
    pub scope: String,
    /// When scope == "custom": which groups are enabled.
    pub groups: Vec<String>,
    /// Individual commands to allow even if not in any enabled group.
    pub allow: Vec<String>,
    /// Individual commands to deny even if their group is enabled.
    pub deny: Vec<String>,
}

impl ClientScope {
    pub fn read() -> Self {
        Self {
            scope: "read".into(),
            groups: Vec::new(),
            allow: Vec::new(),
            deny: Vec::new(),
        }
    }

    pub fn full() -> Self {
        Self {
            scope: "full".into(),
            groups: Vec::new(),
            allow: Vec::new(),
            deny: Vec::new(),
        }
    }
}

// ── Permission check ─────────────────────────────────────────────────────────

/// Look up the [`CommandGroup`] for a given group ID.
pub fn group_by_id(id: &str) -> Option<&'static CommandGroup> {
    GROUPS.iter().find(|g| g.id == id)
}

/// Look up which group a command belongs to (if any).
pub fn group_for_command(command: &str) -> Option<&'static CommandGroup> {
    GROUPS.iter().find(|g| g.commands.contains(&command))
}

/// Check whether a [`ClientScope`] permits a given command.
pub fn is_allowed(cs: &ClientScope, command: &str) -> bool {
    match cs.scope.as_str() {
        "full" => true,
        "read" => {
            // Explicit deny overrides
            if cs.deny.iter().any(|c| c == command) {
                return false;
            }
            // Explicit allow overrides
            if cs.allow.iter().any(|c| c == command) {
                return true;
            }
            // Check built-in read groups
            READ_GROUPS
                .iter()
                .any(|gid| group_by_id(gid).map(|g| g.commands.contains(&command)).unwrap_or(false))
        }
        "custom" => {
            // Explicit deny always wins
            if cs.deny.iter().any(|c| c == command) {
                return false;
            }
            // Explicit allow overrides
            if cs.allow.iter().any(|c| c == command) {
                return true;
            }
            // Check enabled groups
            cs.groups
                .iter()
                .any(|gid| group_by_id(gid).map(|g| g.commands.contains(&command)).unwrap_or(false))
        }
        _ => false,
    }
}

/// Return the set of all commands permitted by a [`ClientScope`].
pub fn allowed_commands(cs: &ClientScope) -> Vec<&'static str> {
    let all_cmds: Vec<&'static str> = GROUPS.iter().flat_map(|g| g.commands.iter().copied()).collect();
    all_cmds.into_iter().filter(|c| is_allowed(cs, c)).collect()
}

/// Return all group IDs that are effectively enabled for a scope.
pub fn effective_groups(cs: &ClientScope) -> Vec<&'static str> {
    match cs.scope.as_str() {
        "full" => GROUPS.iter().map(|g| g.id).collect(),
        "read" => READ_GROUPS.to_vec(),
        "custom" => cs
            .groups
            .iter()
            .filter_map(|gid| group_by_id(gid).map(|g| g.id))
            .collect(),
        _ => Vec::new(),
    }
}

/// Build a detailed permission report for a client.
pub fn permission_report(cs: &ClientScope) -> serde_json::Value {
    let groups_detail: Vec<serde_json::Value> = GROUPS
        .iter()
        .map(|g| {
            let enabled = match cs.scope.as_str() {
                "full" => true,
                "read" => READ_GROUPS.contains(&g.id),
                "custom" => cs.groups.iter().any(|gid| gid == g.id),
                _ => false,
            };
            let commands_detail: Vec<serde_json::Value> = g
                .commands
                .iter()
                .map(|c| {
                    serde_json::json!({
                        "command": c,
                        "allowed": is_allowed(cs, c),
                        "source": if cs.deny.iter().any(|d| d == c) {
                            "denied"
                        } else if cs.allow.iter().any(|a| a == c) {
                            "allowed_override"
                        } else if enabled {
                            "group"
                        } else {
                            "not_granted"
                        }
                    })
                })
                .collect();
            serde_json::json!({
                "id": g.id,
                "label": g.label,
                "description": g.description,
                "dangerous": g.dangerous,
                "enabled": enabled,
                "commands": commands_detail,
            })
        })
        .collect();

    serde_json::json!({
        "scope": cs.scope,
        "groups": groups_detail,
        "total_allowed": allowed_commands(cs).len(),
        "total_commands": GROUPS.iter().map(|g| g.commands.len()).sum::<usize>(),
    })
}

// ── Validation ───────────────────────────────────────────────────────────────

/// Validate and normalise a scope string.
pub fn normalize_scope(scope: &str) -> Result<String, String> {
    let s = scope.trim().to_lowercase();
    match s.as_str() {
        "read" | "readonly" => Ok("read".to_string()),
        "full" => Ok("full".to_string()),
        "custom" => Ok("custom".to_string()),
        _ => Err(format!("invalid scope '{scope}': expected 'read', 'full', or 'custom'")),
    }
}

/// Validate that all group IDs are known.
pub fn validate_groups(groups: &[String]) -> Result<(), String> {
    for gid in groups {
        if group_by_id(gid).is_none() {
            return Err(format!("unknown group '{gid}'"));
        }
    }
    Ok(())
}

/// Validate that all commands are known (belong to some group).
pub fn validate_commands(commands: &[String]) -> Result<(), String> {
    for cmd in commands {
        if group_for_command(cmd).is_none() {
            return Err(format!("unknown command '{cmd}'"));
        }
    }
    Ok(())
}

/// Returns true if the scope enables any dangerous group.
pub fn has_dangerous_groups(cs: &ClientScope) -> bool {
    let active = effective_groups(cs);
    GROUPS.iter().any(|g| g.dangerous && active.contains(&g.id))
}

/// Returns the list of dangerous groups that are enabled.
pub fn dangerous_groups_enabled(cs: &ClientScope) -> Vec<&'static CommandGroup> {
    let active = effective_groups(cs);
    GROUPS
        .iter()
        .filter(|g| g.dangerous && active.contains(&g.id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_groups_have_unique_ids() {
        let mut ids: Vec<&str> = GROUPS.iter().map(|g| g.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), GROUPS.len(), "duplicate group IDs");
    }

    #[test]
    fn no_command_in_multiple_groups() {
        let mut seen = std::collections::HashMap::new();
        for g in GROUPS {
            for c in g.commands {
                if let Some(prev) = seen.insert(*c, g.id) {
                    panic!("command '{c}' in groups '{prev}' and '{}'", g.id);
                }
            }
        }
    }

    #[test]
    fn read_groups_are_all_valid() {
        for gid in READ_GROUPS {
            assert!(group_by_id(gid).is_some(), "unknown read group '{gid}'");
        }
    }

    #[test]
    fn read_groups_are_not_dangerous() {
        for gid in READ_GROUPS {
            let g = group_by_id(gid).unwrap();
            assert!(!g.dangerous, "read group '{gid}' is marked dangerous");
        }
    }

    #[test]
    fn full_scope_allows_everything() {
        let cs = ClientScope::full();
        for g in GROUPS {
            for c in g.commands {
                assert!(is_allowed(&cs, c), "full scope should allow '{c}'");
            }
        }
    }

    #[test]
    fn read_scope_allows_read_groups_only() {
        let cs = ClientScope::read();
        for g in GROUPS {
            let in_read = READ_GROUPS.contains(&g.id);
            for c in g.commands {
                assert_eq!(
                    is_allowed(&cs, c),
                    in_read,
                    "read scope: '{c}' (group '{}') expected {}",
                    g.id,
                    in_read,
                );
            }
        }
    }

    #[test]
    fn custom_scope_respects_groups() {
        let cs = ClientScope {
            scope: "custom".into(),
            groups: vec!["status".into(), "health_read".into()],
            allow: Vec::new(),
            deny: Vec::new(),
        };
        assert!(is_allowed(&cs, "status"));
        assert!(is_allowed(&cs, "sessions"));
        assert!(is_allowed(&cs, "health_query"));
        assert!(!is_allowed(&cs, "search")); // not in enabled groups
        assert!(!is_allowed(&cs, "label")); // not in enabled groups
    }

    #[test]
    fn custom_scope_allow_override() {
        let cs = ClientScope {
            scope: "custom".into(),
            groups: vec!["status".into()],
            allow: vec!["label".into()],
            deny: Vec::new(),
        };
        assert!(is_allowed(&cs, "status"));
        assert!(is_allowed(&cs, "label")); // individually allowed
        assert!(!is_allowed(&cs, "search"));
    }

    #[test]
    fn custom_scope_deny_override() {
        let cs = ClientScope {
            scope: "custom".into(),
            groups: vec!["status".into()],
            allow: Vec::new(),
            deny: vec!["sessions".into()],
        };
        assert!(is_allowed(&cs, "status"));
        assert!(!is_allowed(&cs, "sessions")); // denied even though group is enabled
    }

    #[test]
    fn read_scope_deny_override() {
        let cs = ClientScope {
            scope: "read".into(),
            groups: Vec::new(),
            allow: Vec::new(),
            deny: vec!["search".into()],
        };
        assert!(is_allowed(&cs, "status"));
        assert!(!is_allowed(&cs, "search")); // denied
    }

    #[test]
    fn read_scope_allow_override() {
        let cs = ClientScope {
            scope: "read".into(),
            groups: Vec::new(),
            allow: vec!["label".into()],
            deny: Vec::new(),
        };
        assert!(is_allowed(&cs, "label")); // allowed even though not in read groups
    }

    #[test]
    fn unknown_scope_denies_all() {
        let cs = ClientScope {
            scope: "banana".into(),
            groups: Vec::new(),
            allow: Vec::new(),
            deny: Vec::new(),
        };
        assert!(!is_allowed(&cs, "status"));
    }

    #[test]
    fn normalize_scope_works() {
        assert_eq!(normalize_scope("read").unwrap(), "read");
        assert_eq!(normalize_scope("readonly").unwrap(), "read");
        assert_eq!(normalize_scope("full").unwrap(), "full");
        assert_eq!(normalize_scope("custom").unwrap(), "custom");
        assert!(normalize_scope("admin").is_err());
    }

    #[test]
    fn validate_groups_catches_unknown() {
        assert!(validate_groups(&["status".into()]).is_ok());
        assert!(validate_groups(&["banana".into()]).is_err());
    }

    #[test]
    fn validate_commands_catches_unknown() {
        assert!(validate_commands(&["status".into()]).is_ok());
        assert!(validate_commands(&["banana".into()]).is_err());
    }

    #[test]
    fn has_dangerous_groups_read() {
        assert!(!has_dangerous_groups(&ClientScope::read()));
    }

    #[test]
    fn has_dangerous_groups_full() {
        assert!(has_dangerous_groups(&ClientScope::full()));
    }

    #[test]
    fn has_dangerous_groups_custom_safe() {
        let cs = ClientScope {
            scope: "custom".into(),
            groups: vec!["status".into(), "search".into()],
            allow: Vec::new(),
            deny: Vec::new(),
        };
        assert!(!has_dangerous_groups(&cs));
    }

    #[test]
    fn has_dangerous_groups_custom_dangerous() {
        let cs = ClientScope {
            scope: "custom".into(),
            groups: vec!["status".into(), "iroh_admin".into()],
            allow: Vec::new(),
            deny: Vec::new(),
        };
        assert!(has_dangerous_groups(&cs));
    }

    #[test]
    fn allowed_commands_count() {
        let full = allowed_commands(&ClientScope::full());
        let total: usize = GROUPS.iter().map(|g| g.commands.len()).sum();
        assert_eq!(full.len(), total);

        let read = allowed_commands(&ClientScope::read());
        let read_total: usize = READ_GROUPS
            .iter()
            .filter_map(|gid| group_by_id(gid))
            .map(|g| g.commands.len())
            .sum();
        assert_eq!(read.len(), read_total);
    }

    #[test]
    fn effective_groups_works() {
        assert_eq!(effective_groups(&ClientScope::full()).len(), GROUPS.len());
        assert_eq!(effective_groups(&ClientScope::read()).len(), READ_GROUPS.len());
        let cs = ClientScope {
            scope: "custom".into(),
            groups: vec!["status".into(), "search".into()],
            allow: Vec::new(),
            deny: Vec::new(),
        };
        assert_eq!(effective_groups(&cs).len(), 2);
    }

    #[test]
    fn permission_report_structure() {
        let cs = ClientScope::read();
        let report = permission_report(&cs);
        assert_eq!(report["scope"], "read");
        assert!(report["total_allowed"].as_u64().unwrap() > 0);
        assert!(report["total_commands"].as_u64().unwrap() > 0);
        let groups = report["groups"].as_array().unwrap();
        assert_eq!(groups.len(), GROUPS.len());
        for g in groups {
            assert!(g["id"].as_str().is_some());
            assert!(g["commands"].as_array().is_some());
        }
    }

    #[test]
    fn dangerous_groups_enabled_works() {
        let cs = ClientScope {
            scope: "custom".into(),
            groups: vec!["status".into(), "hooks".into(), "iroh_admin".into()],
            allow: Vec::new(),
            deny: Vec::new(),
        };
        let dg = dangerous_groups_enabled(&cs);
        assert_eq!(dg.len(), 2);
        let ids: Vec<&str> = dg.iter().map(|g| g.id).collect();
        assert!(ids.contains(&"hooks"));
        assert!(ids.contains(&"iroh_admin"));
    }

    #[test]
    fn group_for_command_works() {
        assert_eq!(group_for_command("status").unwrap().id, "status");
        assert_eq!(group_for_command("label").unwrap().id, "labels");
        assert!(group_for_command("nonexistent").is_none());
    }
}
