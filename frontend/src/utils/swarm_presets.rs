//! Preset "mission templates" for the swarm launcher. Persistent layer over
//! `swarm_role_caps`: each preset records the role/agent-type/model/capability
//! profile per team slot. Built-ins ship with the app; user presets are merged
//! over them by name in the KV store under `swarm_presets` (capped).

use serde::{Deserialize, Serialize};

use crate::types::workspace::RoleCapabilities;

/// Key (+ cap) for user-defined presets. Built-ins are served from
/// [`default_presets`] and are not persisted.
pub const SWARM_PRESETS_KEY: &str = "swarm_presets";
pub const MAX_USER_PRESETS: usize = 32;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresetSlot {
    pub role: String,
    pub agent_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<RoleCapabilities>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SwarmPreset {
    pub id: String,
    pub name: String,
    pub slots: Vec<PresetSlot>,
}

/// Built-in starting templates shipped with the app.
pub fn default_presets() -> Vec<SwarmPreset> {
    let caps_full = RoleCapabilities::default(); // all-allowed
    let caps_readonly = RoleCapabilities {
        shell: crate::types::workspace::ShellPolicy::ReadOnly,
        network: true,
        mcp_tools: true,
    };
    vec![
        SwarmPreset {
            id: "feature-build".to_string(),
            name: "Feature build".to_string(),
            slots: vec![
                PresetSlot {
                    role: "coordinator".to_string(),
                    agent_type: "claude".to_string(),
                    model: None,
                    capabilities: Some(caps_full.clone()),
                },
                PresetSlot {
                    role: "builder".to_string(),
                    agent_type: "claude".to_string(),
                    model: None,
                    capabilities: Some(caps_full.clone()),
                },
                PresetSlot {
                    role: "reviewer".to_string(),
                    agent_type: "claude".to_string(),
                    model: None,
                    capabilities: Some(caps_readonly.clone()),
                },
            ],
        },
        SwarmPreset {
            id: "bug-hunt".to_string(),
            name: "Bug hunt".to_string(),
            slots: vec![
                PresetSlot {
                    role: "coordinator".to_string(),
                    agent_type: "claude".to_string(),
                    model: None,
                    capabilities: Some(caps_full.clone()),
                },
                PresetSlot {
                    role: "scout".to_string(),
                    agent_type: "claude".to_string(),
                    model: None,
                    capabilities: Some(caps_readonly.clone()),
                },
                PresetSlot {
                    role: "builder".to_string(),
                    agent_type: "claude".to_string(),
                    model: None,
                    capabilities: Some(caps_full.clone()),
                },
            ],
        },
        SwarmPreset {
            id: "refactor-tests".to_string(),
            name: "Refactor + tests".to_string(),
            slots: vec![
                PresetSlot {
                    role: "coordinator".to_string(),
                    agent_type: "claude".to_string(),
                    model: None,
                    capabilities: Some(caps_full.clone()),
                },
                PresetSlot {
                    role: "builder".to_string(),
                    agent_type: "claude".to_string(),
                    model: None,
                    capabilities: Some(caps_full.clone()),
                },
                PresetSlot {
                    role: "builder".to_string(),
                    agent_type: "claude".to_string(),
                    model: None,
                    capabilities: Some(caps_full.clone()),
                },
            ],
        },
    ]
}

/// Merged view: built-ins, then user presets (user entries with a built-in id
/// override the built-in entirely).
pub fn merge_with_builtin(custom: Vec<SwarmPreset>) -> Vec<SwarmPreset> {
    let mut out = default_presets();
    for preset in custom {
        if let Some(existing) = out.iter_mut().find(|p| p.id == preset.id) {
            *existing = preset;
        } else {
            out.push(preset);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_have_one_coordinator_each() {
        for preset in default_presets() {
            assert_eq!(
                preset.slots.iter().filter(|s| s.role == "coordinator").count(),
                1,
                "{}",
                preset.name
            );
        }
    }

    #[test]
    fn user_preset_overrides_builtin_by_id() {
        let custom = vec![SwarmPreset {
            id: "feature-build".to_string(),
            name: "My feature build".to_string(),
            slots: vec![],
        }];
        let merged = merge_with_builtin(custom);
        let f = merged.iter().find(|p| p.id == "feature-build").unwrap();
        assert_eq!(f.name, "My feature build");
        assert!(f.slots.is_empty());
        assert!(merged.iter().any(|p| p.id == "bug-hunt"));
    }

    #[test]
    fn user_preset_appends() {
        let custom = vec![SwarmPreset {
            id: "custom-t".to_string(),
            name: "Custom".to_string(),
            slots: vec![],
        }];
        let merged = merge_with_builtin(custom);
        assert_eq!(merged.len(), default_presets().len() + 1);
    }
}
