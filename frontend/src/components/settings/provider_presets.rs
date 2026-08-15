//! Provider presets for the Athena settings section.
//!
//! Each preset knows its persisted id (matches `LLMProvider`'s serde names so
//! the backend `infer_provider` explicit-match branch handles them), the base
//! URL it auto-fills, and whether it exposes an OpenAI-compatible `/models`
//! endpoint that the Settings "Fetch models" button can query.

/// A known LLM provider the Settings UI can prefill.
pub struct ProviderPreset {
    pub id: &'static str,
    pub label: &'static str,
    pub default_base_url: &'static str,
    /// Whether `{base_url}/models` returns an OpenAI-compatible model list.
    pub supports_model_list: bool,
    /// Model id pre-filled into the Model field when this preset is selected
    /// and no saved value exists yet. `None` leaves the field blank (the user
    /// types or fetches one).
    pub default_model: Option<&'static str>,
    /// Short guidance shown under the Model field for this preset (e.g. a
    /// reasoning-mode note for GLM 5.2 on NIM). `None` hides the hint.
    pub model_hint: Option<&'static str>,
}

pub const LLM_PROVIDERS: &[ProviderPreset] = &[
    ProviderPreset {
        id: "openai",
        label: "OpenAI",
        default_base_url: "https://api.openai.com/v1",
        supports_model_list: true,
        default_model: None,
        model_hint: None,
    },
    ProviderPreset {
        id: "anthropic",
        label: "Anthropic",
        default_base_url: "https://api.anthropic.com/v1",
        // Anthropic uses the Messages API, not the OpenAI /models shape.
        supports_model_list: false,
        default_model: None,
        model_hint: None,
    },
    ProviderPreset {
        id: "nvidia_nim",
        label: "NVIDIA NIM",
        default_base_url: "https://integrate.api.nvidia.com/v1",
        supports_model_list: true,
        // GLM 5.2 is the recommended orchestrator default on NIM (strongest
        // agentic/tool-loop model there). Verify it's still live via "Fetch
        // models" — the catalog rotates and the field stays editable.
        default_model: Some("z-ai/glm-5.2"),
        model_hint: Some("GLM 5.2 (default) is a reasoning model — keep the prompt concise; verbose reasoning is billed as output tokens. Fetch models to see the live catalog."),
    },
    ProviderPreset {
        id: "lmstudio",
        label: "LM Studio",
        default_base_url: "http://localhost:1234/v1",
        supports_model_list: true,
        default_model: None,
        model_hint: None,
    },
    ProviderPreset {
        id: "custom",
        label: "Custom (OpenAI-compatible)",
        default_base_url: "",
        supports_model_list: true,
        default_model: None,
        model_hint: None,
    },
];

/// Look up a preset by its id.
pub fn provider_preset(id: &str) -> Option<&'static ProviderPreset> {
    LLM_PROVIDERS.iter().find(|preset| preset.id == id)
}

/// Infer which preset a saved base URL corresponds to, mirroring the backend's
/// `infer_provider` host heuristics.
///
/// Used only as the mount-time default for the Settings dropdown so existing
/// users see the right preset selected; the backend stays authoritative at
/// request time. Known trade-off: this duplicates `infer_provider`'s
/// heuristics (accepted for v1 — see the provider-presets plan §4a).
pub fn infer_provider_id(base_url: &str) -> &'static str {
    let host = base_url.split_once("://").map(|(_, rest)| rest).unwrap_or(base_url);
    let host = host.rsplit('@').next().unwrap_or(host);
    let host = host.split('/').next().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host).to_ascii_lowercase();

    if host.contains("anthropic.com") {
        "anthropic"
    } else if host.contains("integrate.api.nvidia.com") {
        "nvidia_nim"
    } else if host.contains("openai.com") {
        "openai"
    } else if host == "localhost"
        || host.ends_with(".local")
        || host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
    {
        "lmstudio"
    } else {
        "custom"
    }
}
