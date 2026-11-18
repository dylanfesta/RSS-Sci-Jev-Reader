use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;

pub const OPENROUTER_DECISIONS_URL: &str = "https://openrouter.ai/api/alpha/decisions";
pub const OPENROUTER_MODELS_URL: &str =
    "https://openrouter.ai/api/v1/models?output_modalities=decisions";
pub const DEFAULT_PROVIDER_ID: &str = "openrouter";
pub const DEFAULT_JEV_MODEL: &str = "typesafe/jev-1.14";
pub const FALLBACK_JEV_MODELS: &[&str] = &[
    "typesafe/jev-1.14",
    "typesafe/jev-1.13",
    "typesafe/jev-router",
];

#[derive(Debug, Clone, Serialize)]
pub struct ProviderSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub group: &'static str,
    pub base_url: &'static str,
    pub portal_url: &'static str,
    pub default_model: &'static str,
    pub recommended_models: &'static [&'static str],
    pub requires_key: bool,
    pub supports_jev_decisions: bool,
    pub description: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveProviderConfig {
    pub provider_id: String,
    pub provider_name: String,
    pub base_url: String,
    pub portal_url: String,
    pub model: String,
    pub requires_key: bool,
    pub supports_jev_decisions: bool,
}

pub const BUILTIN_PROVIDERS: &[ProviderSpec] = &[
    // --- Featured & Aggregators (OpenCode / Multi-Model Gateways) ---
    ProviderSpec {
        id: "openrouter",
        name: "OpenRouter (TypeSafe Jev + 300+ Models)",
        group: "Featured and Aggregators",
        base_url: "https://openrouter.ai/api/v1",
        portal_url: "https://openrouter.ai/keys",
        default_model: "typesafe/jev-1.14",
        recommended_models: &[
            "typesafe/jev-1.14",
            "typesafe/jev-1.13",
            "typesafe/jev-router",
            "google/gemini-2.5-flash",
            "openai/gpt-4o-mini",
            "anthropic/claude-3.5-haiku",
            "deepseek/deepseek-chat",
        ],
        requires_key: true,
        supports_jev_decisions: true,
        description: "Supports native TypeSafe Jev decision models (/api/alpha/decisions) and 300+ chat models.",
    },
    ProviderSpec {
        id: "nanogpt",
        name: "NanoGPT",
        group: "Featured and Aggregators",
        base_url: "https://nano-gpt.com/api/v1",
        portal_url: "https://nano-gpt.com/api",
        default_model: "gpt-4o-mini",
        recommended_models: &[
            "gpt-4o-mini",
            "chatgpt-4o-latest",
            "claude-3-5-sonnet-20241022",
            "claude-3-5-haiku-20241022",
            "gemini-2.0-flash",
            "deepseek-chat",
            "deepseek-reasoner",
            "llama-3.3-70b",
        ],
        requires_key: true,
        supports_jev_decisions: false,
        description: "Pay-per-prompt access to OpenAI, Claude, Gemini, DeepSeek, and open-weight models via NanoGPT.",
    },
    ProviderSpec {
        id: "opencode",
        name: "OpenCode Zen",
        group: "Featured and Aggregators",
        base_url: "https://opencode.ai/zen/v1",
        portal_url: "https://opencode.ai/auth",
        default_model: "claude-3-5-haiku",
        recommended_models: &[
            "claude-3-5-haiku",
            "claude-sonnet-4",
            "gpt-4.1-mini",
            "gpt-4.1",
            "gemini-2.5-flash",
        ],
        requires_key: true,
        supports_jev_decisions: false,
        description: "OpenCode Zen unified gateway for coding and reasoning models.",
    },
    // --- Frontier Model Labs ---
    ProviderSpec {
        id: "openai",
        name: "OpenAI",
        group: "Frontier Labs",
        base_url: "https://api.openai.com/v1",
        portal_url: "https://platform.openai.com/api-keys",
        default_model: "gpt-4o-mini",
        recommended_models: &[
            "gpt-4o-mini",
            "gpt-4.1-mini",
            "gpt-4.1-nano",
            "gpt-4.1",
            "gpt-4o",
            "o4-mini",
        ],
        requires_key: true,
        supports_jev_decisions: false,
        description: "Official OpenAI API (GPT-4o, GPT-4.1, o3/o4 series).",
    },
    ProviderSpec {
        id: "anthropic",
        name: "Anthropic (Claude)",
        group: "Frontier Labs",
        base_url: "https://api.anthropic.com/v1",
        portal_url: "https://console.anthropic.com/settings/keys",
        default_model: "claude-3-5-haiku-latest",
        recommended_models: &[
            "claude-3-5-haiku-latest",
            "claude-sonnet-4-20250514",
            "claude-3-7-sonnet-latest",
            "claude-3-5-sonnet-latest",
        ],
        requires_key: true,
        supports_jev_decisions: false,
        description: "Official Anthropic Messages API for Claude Haiku, Sonnet, and Opus.",
    },
    ProviderSpec {
        id: "google",
        name: "Google Gemini (AI Studio)",
        group: "Frontier Labs",
        base_url: "https://generativelanguage.googleapis.com/v1beta/openai",
        portal_url: "https://aistudio.google.com/app/apikey",
        default_model: "gemini-2.5-flash",
        recommended_models: &[
            "gemini-2.5-flash",
            "gemini-2.5-pro",
            "gemini-2.0-flash",
            "gemini-2.0-flash-lite",
        ],
        requires_key: true,
        supports_jev_decisions: false,
        description: "Google AI Studio OpenAI-compatible endpoint for Gemini 2.5 / 2.0 models.",
    },
    ProviderSpec {
        id: "deepseek",
        name: "DeepSeek",
        group: "Frontier Labs",
        base_url: "https://api.deepseek.com/v1",
        portal_url: "https://platform.deepseek.com/api_keys",
        default_model: "deepseek-chat",
        recommended_models: &["deepseek-chat", "deepseek-reasoner"],
        requires_key: true,
        supports_jev_decisions: false,
        description: "Official DeepSeek API (DeepSeek-V3 and DeepSeek-R1).",
    },
    ProviderSpec {
        id: "xai",
        name: "xAI (Grok)",
        group: "Frontier Labs",
        base_url: "https://api.x.ai/v1",
        portal_url: "https://console.x.ai",
        default_model: "grok-3-mini",
        recommended_models: &["grok-3-mini", "grok-3", "grok-2-latest"],
        requires_key: true,
        supports_jev_decisions: false,
        description: "Official xAI API for Grok-3 and Grok-2 models.",
    },
    ProviderSpec {
        id: "mistral",
        name: "Mistral AI",
        group: "Frontier Labs",
        base_url: "https://api.mistral.ai/v1",
        portal_url: "https://console.mistral.ai/api-keys",
        default_model: "mistral-small-latest",
        recommended_models: &[
            "mistral-small-latest",
            "mistral-large-latest",
            "ministral-8b-latest",
            "codestral-latest",
        ],
        requires_key: true,
        supports_jev_decisions: false,
        description: "Mistral La Plateforme API.",
    },
    // --- Fast Inference & OpenCode Ecosystem Providers ---
    ProviderSpec {
        id: "groq",
        name: "Groq",
        group: "Fast Inference and Cloud",
        base_url: "https://api.groq.com/openai/v1",
        portal_url: "https://console.groq.com/keys",
        default_model: "llama-3.3-70b-versatile",
        recommended_models: &[
            "llama-3.3-70b-versatile",
            "llama-3.1-8b-instant",
            "deepseek-r1-distill-llama-70b",
            "qwen-qwq-32b",
        ],
        requires_key: true,
        supports_jev_decisions: false,
        description: "Ultra-fast LPU inference for Llama, DeepSeek, and Qwen models.",
    },
    ProviderSpec {
        id: "cerebras",
        name: "Cerebras",
        group: "Fast Inference and Cloud",
        base_url: "https://api.cerebras.ai/v1",
        portal_url: "https://cloud.cerebras.ai",
        default_model: "llama-3.3-70b",
        recommended_models: &["llama-3.3-70b", "llama3.1-8b", "qwen-3-32b"],
        requires_key: true,
        supports_jev_decisions: false,
        description: "Wafer-scale high-speed inference cloud.",
    },
    ProviderSpec {
        id: "together",
        name: "Together AI",
        group: "Fast Inference and Cloud",
        base_url: "https://api.together.xyz/v1",
        portal_url: "https://api.together.ai/settings/api-keys",
        default_model: "meta-llama/Llama-3.3-70B-Instruct-Turbo",
        recommended_models: &[
            "meta-llama/Llama-3.3-70B-Instruct-Turbo",
            "Qwen/Qwen2.5-72B-Instruct-Turbo",
            "deepseek-ai/DeepSeek-V3",
        ],
        requires_key: true,
        supports_jev_decisions: false,
        description: "Together AI cloud for open-source models.",
    },
    ProviderSpec {
        id: "fireworks",
        name: "Fireworks AI",
        group: "Fast Inference and Cloud",
        base_url: "https://api.fireworks.ai/inference/v1",
        portal_url: "https://fireworks.ai/account/api-keys",
        default_model: "accounts/fireworks/models/llama-v3p3-70b-instruct",
        recommended_models: &[
            "accounts/fireworks/models/llama-v3p3-70b-instruct",
            "accounts/fireworks/models/deepseek-v3",
            "accounts/fireworks/models/qwen2p5-72b-instruct",
        ],
        requires_key: true,
        supports_jev_decisions: false,
        description: "Fast serverless open-weight model inference.",
    },
    ProviderSpec {
        id: "venice",
        name: "Venice AI",
        group: "Fast Inference and Cloud",
        base_url: "https://api.venice.ai/api/v1",
        portal_url: "https://venice.ai/settings/api",
        default_model: "llama-3.3-70b",
        recommended_models: &["llama-3.3-70b", "deepseek-r1-671b", "qwen-2.5-vl"],
        requires_key: true,
        supports_jev_decisions: false,
        description: "Privacy-first uncensored open-model API.",
    },
    ProviderSpec {
        id: "chutes",
        name: "Chutes AI",
        group: "Fast Inference and Cloud",
        base_url: "https://llm.chutes.ai/v1",
        portal_url: "https://chutes.ai",
        default_model: "deepseek-ai/DeepSeek-V3",
        recommended_models: &[
            "deepseek-ai/DeepSeek-V3",
            "deepseek-ai/DeepSeek-R1",
            "unsloth/Llama-3.3-70B-Instruct",
        ],
        requires_key: true,
        supports_jev_decisions: false,
        description: "Decentralized serverless AI compute (Bittensor SN64).",
    },
    ProviderSpec {
        id: "siliconflow",
        name: "SiliconFlow",
        group: "Fast Inference and Cloud",
        base_url: "https://api.siliconflow.cn/v1",
        portal_url: "https://cloud.siliconflow.cn/account/ak",
        default_model: "deepseek-ai/DeepSeek-V3",
        recommended_models: &[
            "deepseek-ai/DeepSeek-V3",
            "Qwen/Qwen2.5-72B-Instruct",
            "deepseek-ai/DeepSeek-R1",
        ],
        requires_key: true,
        supports_jev_decisions: false,
        description: "High-speed inference platform for Qwen and DeepSeek.",
    },
    ProviderSpec {
        id: "zai",
        name: "Z.ai (Zhipu GLM)",
        group: "Fast Inference and Cloud",
        base_url: "https://api.z.ai/api/paas/v4",
        portal_url: "https://z.ai",
        default_model: "glm-4.5-flash",
        recommended_models: &["glm-4.5-flash", "glm-4.5", "glm-4-plus"],
        requires_key: true,
        supports_jev_decisions: false,
        description: "Z.ai / Zhipu GLM-4.5 series API.",
    },
    ProviderSpec {
        id: "perplexity",
        name: "Perplexity (Sonar)",
        group: "Fast Inference and Cloud",
        base_url: "https://api.perplexity.ai",
        portal_url: "https://www.perplexity.ai/settings/api",
        default_model: "sonar",
        recommended_models: &["sonar", "sonar-pro", "sonar-reasoning"],
        requires_key: true,
        supports_jev_decisions: false,
        description: "Perplexity Sonar API.",
    },
    ProviderSpec {
        id: "deepinfra",
        name: "DeepInfra",
        group: "Fast Inference and Cloud",
        base_url: "https://api.deepinfra.com/v1/openai",
        portal_url: "https://deepinfra.com/dash/api_keys",
        default_model: "meta-llama/Llama-3.3-70B-Instruct-Turbo",
        recommended_models: &[
            "meta-llama/Llama-3.3-70B-Instruct-Turbo",
            "deepseek-ai/DeepSeek-V3",
            "Qwen/Qwen2.5-72B-Instruct",
        ],
        requires_key: true,
        supports_jev_decisions: false,
        description: "Low-cost serverless inference for open models.",
    },
    ProviderSpec {
        id: "hyperbolic",
        name: "Hyperbolic",
        group: "Fast Inference and Cloud",
        base_url: "https://api.hyperbolic.xyz/v1",
        portal_url: "https://app.hyperbolic.xyz/settings",
        default_model: "meta-llama/Llama-3.3-70B-Instruct",
        recommended_models: &[
            "meta-llama/Llama-3.3-70B-Instruct",
            "deepseek-ai/DeepSeek-V3",
            "Qwen/Qwen2.5-72B-Instruct",
        ],
        requires_key: true,
        supports_jev_decisions: false,
        description: "Open-access AI cloud for frontier open-weight models.",
    },
    ProviderSpec {
        id: "featherless",
        name: "Featherless AI",
        group: "Fast Inference and Cloud",
        base_url: "https://api.featherless.ai/v1",
        portal_url: "https://featherless.ai/account/api-keys",
        default_model: "meta-llama/Meta-Llama-3.1-70B-Instruct",
        recommended_models: &[
            "meta-llama/Meta-Llama-3.1-70B-Instruct",
            "Qwen/Qwen2.5-72B-Instruct",
        ],
        requires_key: true,
        supports_jev_decisions: false,
        description: "Serverless access to thousands of HuggingFace models.",
    },
    ProviderSpec {
        id: "sambanova",
        name: "SambaNova Cloud",
        group: "Fast Inference and Cloud",
        base_url: "https://api.sambanova.ai/v1",
        portal_url: "https://cloud.sambanova.ai/apis",
        default_model: "Meta-Llama-3.3-70B-Instruct",
        recommended_models: &[
            "Meta-Llama-3.3-70B-Instruct",
            "DeepSeek-R1-Distill-Llama-70B",
            "Qwen2.5-72B-Instruct",
        ],
        requires_key: true,
        supports_jev_decisions: false,
        description: "SambaNova RDU-accelerated inference.",
    },
    ProviderSpec {
        id: "nebius",
        name: "Nebius AI Studio",
        group: "Fast Inference and Cloud",
        base_url: "https://api.studio.nebius.ai/v1",
        portal_url: "https://studio.nebius.ai/settings/api-keys",
        default_model: "meta-llama/Llama-3.3-70B-Instruct",
        recommended_models: &[
            "meta-llama/Llama-3.3-70B-Instruct",
            "deepseek-ai/DeepSeek-V3",
            "Qwen/Qwen2.5-72B-Instruct",
        ],
        requires_key: true,
        supports_jev_decisions: false,
        description: "Nebius AI Studio OpenAI-compatible inference.",
    },
    ProviderSpec {
        id: "nvidia",
        name: "NVIDIA NIM",
        group: "Fast Inference and Cloud",
        base_url: "https://integrate.api.nvidia.com/v1",
        portal_url: "https://build.nvidia.com",
        default_model: "meta/llama-3.3-70b-instruct",
        recommended_models: &[
            "meta/llama-3.3-70b-instruct",
            "deepseek-ai/deepseek-r1",
            "nvidia/llama-3.1-nemotron-70b-instruct",
        ],
        requires_key: true,
        supports_jev_decisions: false,
        description: "NVIDIA NIM API catalog.",
    },
    ProviderSpec {
        id: "github-models",
        name: "GitHub Models",
        group: "Fast Inference and Cloud",
        base_url: "https://models.inference.ai.azure.com",
        portal_url: "https://github.com/settings/tokens",
        default_model: "gpt-4o-mini",
        recommended_models: &["gpt-4o-mini", "gpt-4o", "Llama-3.3-70B-Instruct"],
        requires_key: true,
        supports_jev_decisions: false,
        description: "GitHub Models Azure inference endpoint (uses GitHub PAT).",
    },
    ProviderSpec {
        id: "novita",
        name: "Novita AI",
        group: "Fast Inference and Cloud",
        base_url: "https://api.novita.ai/v3/openai",
        portal_url: "https://novita.ai/settings/key-management",
        default_model: "meta-llama/llama-3.3-70b-instruct",
        recommended_models: &[
            "meta-llama/llama-3.3-70b-instruct",
            "deepseek/deepseek-v3",
            "qwen/qwen-2.5-72b-instruct",
        ],
        requires_key: true,
        supports_jev_decisions: false,
        description: "Novita AI serverless LLM API.",
    },
    // --- Local & Self-Hosted / Custom ---
    ProviderSpec {
        id: "ollama",
        name: "Ollama (Local)",
        group: "Local and Custom Providers",
        base_url: "http://127.0.0.1:11434/v1",
        portal_url: "https://ollama.com/library",
        default_model: "llama3.2",
        recommended_models: &["llama3.2", "qwen2.5:14b", "deepseek-r1:14b", "mistral"],
        requires_key: false,
        supports_jev_decisions: false,
        description: "Local Ollama server on 127.0.0.1:11434 (no API key required).",
    },
    ProviderSpec {
        id: "lmstudio",
        name: "LM Studio (Local)",
        group: "Local and Custom Providers",
        base_url: "http://127.0.0.1:1234/v1",
        portal_url: "https://lmstudio.ai",
        default_model: "local-model",
        recommended_models: &["local-model"],
        requires_key: false,
        supports_jev_decisions: false,
        description: "Local LM Studio OpenAI server on 127.0.0.1:1234 (no API key required).",
    },
    ProviderSpec {
        id: "custom",
        name: "Custom / Own Provider (OpenAI-Compatible)",
        group: "Local and Custom Providers",
        base_url: "https://your-provider.example.com/v1",
        portal_url: "https://your-provider.example.com",
        default_model: "gpt-4o-mini",
        recommended_models: &["gpt-4o-mini", "local-model"],
        requires_key: false,
        supports_jev_decisions: false,
        description: "Connect your own OpenAI-compatible provider, LiteLLM proxy, vLLM server, or custom gateway with your own Base URL and Portal link.",
    },
];

pub fn list_providers_json() -> Vec<Value> {
    BUILTIN_PROVIDERS
        .iter()
        .map(|p| serde_json::to_value(p).unwrap_or_else(|_| json!({})))
        .collect()
}

pub fn get_provider_spec(provider_id: &str) -> &'static ProviderSpec {
    let clean = provider_id.trim().to_lowercase();
    BUILTIN_PROVIDERS
        .iter()
        .find(|p| p.id == clean)
        .unwrap_or(&BUILTIN_PROVIDERS[0])
}

pub fn resolve_active_provider_config(settings: &HashMap<String, String>) -> ActiveProviderConfig {
    let provider_id = settings
        .get("ai_provider")
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_PROVIDER_ID.to_string());
    let spec = get_provider_spec(&provider_id);

    let custom_name = settings
        .get("custom_provider_name")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let custom_base = settings
        .get("custom_base_url")
        .map(|s| s.trim().trim_end_matches('/'))
        .filter(|s| !s.is_empty());
    let custom_portal = settings
        .get("custom_portal_url")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    let provider_name = if spec.id == "custom" {
        custom_name.unwrap_or("Custom Provider").to_string()
    } else {
        custom_name.unwrap_or(spec.name).to_string()
    };

    let base_url = custom_base
        .map(|s| s.to_string())
        .unwrap_or_else(|| spec.base_url.to_string());

    let portal_url = custom_portal
        .map(|s| s.to_string())
        .unwrap_or_else(|| spec.portal_url.to_string());

    let raw_model = settings
        .get("jev_model")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or(spec.default_model);

    let model = if spec.id == "openrouter" {
        resolve_model_alias(raw_model)
    } else {
        raw_model.to_string()
    };

    ActiveProviderConfig {
        provider_id: spec.id.to_string(),
        provider_name,
        base_url,
        portal_url,
        model,
        requires_key: spec.requires_key,
        supports_jev_decisions: spec.supports_jev_decisions,
    }
}

fn round4(val: f64) -> f64 {
    (val * 10000.0).round() / 10000.0
}

pub fn resolve_model_alias(model: &str) -> String {
    let m = model.trim();
    if m.is_empty()
        || matches!(
            m,
            "typesafe/jev-latest" | "jev-latest" | "jev" | "typesafe/jev"
        )
    {
        DEFAULT_JEV_MODEL.to_string()
    } else {
        m.to_string()
    }
}

pub fn is_jev_decisions_model(provider_id: &str, model: &str) -> bool {
    if provider_id.trim().to_lowercase() != "openrouter" {
        return false;
    }
    let m = resolve_model_alias(model).to_lowercase();
    m.starts_with("typesafe/jev") || m.starts_with("jev")
}

pub fn build_entry_state(entry: &Value) -> Value {
    let get_str = |k: &str| {
        entry
            .get(k)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    json!({
        "journal": get_str("journal_title"),
        "title": get_str("title"),
        "authors": get_str("authors"),
        "published_at": get_str("published_at"),
        "abstract": get_str("summary"),
        "link": get_str("link"),
    })
}

pub fn build_question_spec(query: &Value) -> Result<Value, String> {
    let qtype = query
        .get("query_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let instructions = query
        .get("instructions")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let criteria = query.get("criteria");

    if qtype == "noul" {
        let mut true_desc = String::new();
        let mut false_desc = String::new();
        if let Some(obj) = criteria.and_then(|c| c.as_object()) {
            true_desc = obj
                .get("true")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            false_desc = obj
                .get("false")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
        }
        return Ok(json!({
            "type": "noul",
            "instructions": instructions,
            "criteria": {
                "true": if true_desc.is_empty() {
                    "Yes, the paper satisfies the instructions.".to_string()
                } else {
                    true_desc
                },
                "false": if false_desc.is_empty() {
                    "No, the paper does not satisfy the instructions.".to_string()
                } else {
                    false_desc
                },
            }
        }));
    }

    if qtype == "score" {
        let arr = criteria
            .and_then(|c| c.as_array())
            .ok_or_else(|| {
                format!(
                    "Score query '{}' must have at least 2 criteria levels.",
                    query.get("name").and_then(|v| v.as_str()).unwrap_or("")
                )
            })?;
        let clean_levels: Vec<String> = arr
            .iter()
            .filter_map(|v| {
                let s = v.as_str().unwrap_or("").trim().to_string();
                if s.is_empty() { None } else { Some(s) }
            })
            .collect();
        if clean_levels.len() < 2 {
            return Err(format!(
                "Score query '{}' must have at least 2 criteria levels.",
                query.get("name").and_then(|v| v.as_str()).unwrap_or("")
            ));
        }
        return Ok(json!({
            "type": "score",
            "instructions": instructions,
            "criteria": clean_levels,
        }));
    }

    Err(format!("Unsupported query_type: {}", qtype))
}

fn value_as_f64(v: &Value) -> Option<f64> {
    if let Some(f) = v.as_f64() {
        Some(f)
    } else if let Some(b) = v.as_bool() {
        Some(if b { 1.0 } else { 0.0 })
    } else if let Some(s) = v.as_str() {
        s.parse::<f64>().ok()
    } else {
        None
    }
}

pub fn normalize_jev_answer(query: &Value, answer: &Value) -> Result<Value, String> {
    let qtype = query
        .get("query_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let criteria = query.get("criteria").cloned().unwrap_or(Value::Null);
    let qname = query
        .get("name")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("Unnamed Query")
        .to_string();
    let qweight = query
        .get("weight")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0);
    let qinstructions = query
        .get("instructions")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let query_snapshot = json!({
        "id": query.get("id").cloned().unwrap_or(Value::Null),
        "name": qname,
        "query_type": qtype,
        "weight": qweight,
        "instructions": qinstructions,
        "criteria": criteria,
    });

    if qtype == "noul" {
        let mut raw_val = answer.get("noul").and_then(value_as_f64);
        let probs = answer.get("probabilities");

        if raw_val.is_none() {
            if let Some(obj) = probs.and_then(|p| p.as_object()) {
                raw_val = obj
                    .get("true")
                    .or_else(|| obj.get("yes"))
                    .or_else(|| obj.get("True"))
                    .and_then(value_as_f64);
            }
        }
        if raw_val.is_none() {
            if let Some(arr) = probs.and_then(|p| p.as_array()) {
                if arr.len() >= 2 {
                    raw_val = arr.last().and_then(value_as_f64);
                }
            }
        }
        if raw_val.is_none() {
            raw_val = answer
                .get("probability")
                .or_else(|| answer.get("value"))
                .and_then(value_as_f64);
        }

        let prob = raw_val.unwrap_or(0.0).clamp(0.0, 1.0);
        let verdict = if prob >= 0.5 { "Yes" } else { "No" };
        let explicit_conf = answer.get("confidence").and_then(value_as_f64);
        let conf_val = explicit_conf
            .unwrap_or_else(|| prob.max(1.0 - prob))
            .clamp(0.0, 1.0);
        let weighted_val = round4(qweight * prob);
        let raw_outcome_text = format!(
            "{} ({:.1}% conf · P(Yes)={:.1}%)",
            verdict,
            conf_val * 100.0,
            prob * 100.0
        );

        let probs_out = if let Some(p) = probs {
            if !p.is_null() {
                p.clone()
            } else {
                json!({
                    "true": round4(prob),
                    "false": round4(1.0 - prob),
                })
            }
        } else {
            json!({
                "true": round4(prob),
                "false": round4(1.0 - prob),
            })
        };

        return Ok(json!({
            "query_name_snapshot": qname,
            "query_type_snapshot": qtype,
            "weight_snapshot": qweight,
            "normalized_score": round4(prob),
            "weighted_score_snapshot": weighted_val,
            "confidence": round4(conf_val),
            "raw_outcome_text": raw_outcome_text,
            "display_value": raw_outcome_text,
            "query_snapshot": query_snapshot,
            "raw_response": {
                "type": "noul",
                "verdict": verdict,
                "noul": round4(prob),
                "confidence": round4(conf_val),
                "probabilities": probs_out,
            },
        }));
    }

    if qtype == "score" {
        let levels: Vec<String> = if let Some(arr) = criteria.as_array() {
            let v: Vec<String> = arr
                .iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect();
            if v.len() >= 2 {
                v
            } else {
                vec!["Low".to_string(), "High".to_string()]
            }
        } else {
            vec!["Low".to_string(), "High".to_string()]
        };

        let k = levels.len();
        let probs = answer.get("probabilities");
        let raw_score = answer.get("score");

        let mut clean_probs: Vec<f64> = Vec::new();
        if let Some(arr) = probs.and_then(|p| p.as_array()) {
            if arr.len() == k {
                clean_probs = arr
                    .iter()
                    .map(|p| value_as_f64(p).unwrap_or(0.0).max(0.0))
                    .collect();
            }
        } else if let Some(obj) = probs.and_then(|p| p.as_object()) {
            clean_probs = levels
                .iter()
                .enumerate()
                .map(|(i, lvl)| {
                    obj.get(lvl)
                        .or_else(|| obj.get(&i.to_string()))
                        .and_then(value_as_f64)
                        .unwrap_or(0.0)
                        .max(0.0)
                })
                .collect();
        }

        let (mut norm, computed_raw_score, best_idx) = if clean_probs.len() == k
            && clean_probs.iter().sum::<f64>() > 0.0
        {
            let total_p: f64 = clean_probs.iter().sum();
            for p in &mut clean_probs {
                *p /= total_p;
            }
            let expected_zero_idx: f64 = clean_probs
                .iter()
                .enumerate()
                .map(|(i, p)| (i as f64) * (*p))
                .sum();
            let n = expected_zero_idx / ((k - 1) as f64);
            let comp = expected_zero_idx + 1.0;
            let mut b_idx = 0usize;
            let mut max_p = -1.0f64;
            for (i, &p) in clean_probs.iter().enumerate() {
                if p > max_p {
                    max_p = p;
                    b_idx = i;
                }
            }
            (n, comp, b_idx)
        } else if let Some(rs) = raw_score {
            if let Some(rs_str) = rs.as_str() {
                if let Some(idx) = levels.iter().position(|l| l == rs_str) {
                    let n = (idx as f64) / ((k - 1) as f64);
                    (n, (idx + 1) as f64, idx)
                } else {
                    let val = rs_str.parse::<f64>().unwrap_or(1.0);
                    if (1.0..=(k as f64)).contains(&val) {
                        let n = (val - 1.0) / ((k - 1) as f64);
                        let b = ((val - 1.0).round() as isize).clamp(0, (k - 1) as isize) as usize;
                        (n, val, b)
                    } else {
                        let n = val / ((k - 1) as f64);
                        let b = (val.round() as isize).clamp(0, (k - 1) as isize) as usize;
                        (n, val, b)
                    }
                }
            } else {
                let val = value_as_f64(rs).unwrap_or(1.0);
                if (1.0..=(k as f64)).contains(&val) {
                    let n = (val - 1.0) / ((k - 1) as f64);
                    let b = ((val - 1.0).round() as isize).clamp(0, (k - 1) as isize) as usize;
                    (n, val, b)
                } else {
                    let n = val / ((k - 1) as f64);
                    let b = (val.round() as isize).clamp(0, (k - 1) as isize) as usize;
                    (n, val, b)
                }
            }
        } else {
            (0.0, 1.0, 0)
        };

        norm = norm.clamp(0.0, 1.0);
        let explicit_conf = answer.get("confidence").and_then(value_as_f64);
        let conf_val: Option<f64> = if let Some(ec) = explicit_conf {
            Some(ec.clamp(0.0, 1.0))
        } else if !clean_probs.is_empty() {
            let max_p = clean_probs
                .iter()
                .copied()
                .fold(0.0f64, f64::max)
                .clamp(0.0, 1.0);
            Some(max_p)
        } else {
            None
        };

        let top_label = levels.get(best_idx).cloned().unwrap_or_default();
        let conf_part = if let Some(cv) = conf_val {
            format!(" · {:.1}% conf", cv * 100.0)
        } else {
            String::new()
        };
        let mut raw_outcome_text = format!(
            "{:.2} / {} ({:.0}%{})",
            computed_raw_score,
            k,
            norm * 100.0,
            conf_part
        );
        if !top_label.is_empty() {
            raw_outcome_text.push_str(&format!(" — {}", top_label));
        }

        let weighted_val = round4(qweight * norm);
        let display_val = format!(
            "{:.2} / {} ({:.1}%{})",
            computed_raw_score,
            k,
            norm * 100.0,
            conf_part
        );

        let probs_out = if !clean_probs.is_empty() {
            Value::Array(clean_probs.iter().map(|&p| json!(round4(p))).collect())
        } else {
            answer
                .get("probabilities")
                .cloned()
                .unwrap_or_else(|| json!([]))
        };

        let legend_out = answer
            .get("legend")
            .cloned()
            .unwrap_or_else(|| json!(levels));

        return Ok(json!({
            "query_name_snapshot": qname,
            "query_type_snapshot": qtype,
            "weight_snapshot": qweight,
            "normalized_score": round4(norm),
            "weighted_score_snapshot": weighted_val,
            "confidence": conf_val.map(round4),
            "raw_outcome_text": raw_outcome_text,
            "display_value": display_val,
            "query_snapshot": query_snapshot,
            "raw_response": {
                "type": "score",
                "score": round4(computed_raw_score),
                "max_level": k,
                "top_level": top_label,
                "confidence": conf_val.map(round4),
                "legend": legend_out,
                "probabilities": probs_out,
            },
        }));
    }

    Err(format!("Unsupported query_type: {}", qtype))
}

async fn discover_openrouter_decision_models(client: &reqwest::Client) -> Vec<String> {
    if let Ok(resp) = client
        .get(OPENROUTER_MODELS_URL)
        .timeout(Duration::from_secs(10))
        .send()
        .await
    {
        if resp.status().is_success() {
            if let Ok(data) = resp.json::<Value>().await {
                if let Some(arr) = data.get("data").and_then(|d| d.as_array()) {
                    let ids: Vec<String> = arr
                        .iter()
                        .filter_map(|item| {
                            item.get("id")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                        })
                        .collect();
                    let jev_ids: Vec<String> = ids
                        .iter()
                        .filter(|m| {
                            let l = m.to_lowercase();
                            l.contains("jev") && !l.contains("router")
                        })
                        .cloned()
                        .collect();
                    let other_ids: Vec<String> = ids
                        .into_iter()
                        .filter(|m| !jev_ids.contains(m))
                        .collect();
                    let mut combined = jev_ids;
                    combined.extend(other_ids);
                    return combined;
                }
            }
        }
    }
    Vec::new()
}

pub async fn fetch_provider_models(
    provider_cfg: &ActiveProviderConfig,
    api_key: Option<&str>,
) -> Result<Vec<String>, String> {
    let spec = get_provider_spec(&provider_cfg.provider_id);
    let mut models: Vec<String> = spec
        .recommended_models
        .iter()
        .map(|s| s.to_string())
        .collect();

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let models_url = format!("{}/models", provider_cfg.base_url.trim_end_matches('/'));
    let mut req = client.get(&models_url).header("Accept", "application/json");
    if let Some(k) = api_key {
        if !k.trim().is_empty() {
            req = req
                .header("Authorization", format!("Bearer {}", k.trim()))
                .header("x-api-key", k.trim());
        }
    }

    if let Ok(resp) = req.send().await {
        if resp.status().is_success() {
            if let Ok(val) = resp.json::<Value>().await {
                let arr_opt = val
                    .get("data")
                    .and_then(|d| d.as_array())
                    .or_else(|| val.get("models").and_then(|d| d.as_array()))
                    .or_else(|| val.as_array());
                if let Some(arr) = arr_opt {
                    for item in arr.iter().take(120) {
                        let id_opt = item
                            .get("id")
                            .and_then(|v| v.as_str())
                            .or_else(|| item.get("name").and_then(|v| v.as_str()))
                            .or_else(|| item.as_str());
                        if let Some(id_str) = id_opt {
                            let clean = id_str.trim().to_string();
                            if !clean.is_empty() && !models.contains(&clean) {
                                models.push(clean);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(models)
}

fn extract_json_object(raw_text: &str) -> Option<Value> {
    let trimmed = raw_text.trim();
    if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        if v.is_object() {
            return Some(v);
        }
    }
    // Strip ```json ... ``` fences or locate outer {...}
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if start < end {
            let slice = &trimmed[start..=end];
            if let Ok(v) = serde_json::from_str::<Value>(slice) {
                if v.is_object() {
                    return Some(v);
                }
            }
        }
    }
    None
}

async fn call_chat_completions_decisions(
    state: &Value,
    questions: &Value,
    api_key: &str,
    provider_cfg: &ActiveProviderConfig,
) -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let system_prompt = "You are a calibrated scientific research relevance evaluator (TypeSafe Jev compatible). \
Given a paper's metadata/abstract (`state`) and a dictionary of `questions`, evaluate every question key and respond ONLY with a valid JSON object (no markdown prose outside JSON). \
The returned JSON object MUST map each question key (or inside an `\"answers\"` object) to: \
- For `\"type\": \"noul\"` (Yes/No): `{\"noul\": <float in [0.0, 1.0] representing P(true)>, \"confidence\": <float in [0.0, 1.0]>, \"probabilities\": {\"true\": <P(true)>, \"false\": <1 - P(true)>}}`. \
- For `\"type\": \"score\"` (Ordered rubric with K criteria levels from index 0 lowest to K-1 highest): `{\"probabilities\": [<p_0>, ..., <p_{K-1}>], \"score\": <float in [1.0, K]>, \"confidence\": <float in [0.0, 1.0]>}` where `probabilities` has exact length K and sums to 1.0.";

    let user_prompt = serde_json::to_string_pretty(&json!({
        "state": state,
        "questions": questions,
    }))
    .unwrap_or_default();

    let base = provider_cfg.base_url.trim_end_matches('/');
    let is_native_anthropic = provider_cfg.provider_id == "anthropic" && !base.ends_with("/openai");

    let effective_key = if api_key.trim().is_empty() {
        "local-key"
    } else {
        api_key.trim()
    };

    if is_native_anthropic {
        let url = format!("{}/messages", base);
        let payload = json!({
            "model": provider_cfg.model,
            "max_tokens": 1024,
            "temperature": 0.0,
            "system": system_prompt,
            "messages": [
                { "role": "user", "content": user_prompt }
            ]
        });

        let resp = client
            .post(&url)
            .header("x-api-key", effective_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("{} request error: {}", provider_cfg.provider_name, e))?;

        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(format!(
                "{} API error ({}) for model '{}': {}",
                provider_cfg.provider_name,
                status.as_u16(),
                provider_cfg.model,
                body_text
            ));
        }
        let data: Value = serde_json::from_str(&body_text)
            .map_err(|e| format!("Invalid JSON from {}: {}", provider_cfg.provider_name, e))?;
        let text_content = data
            .get("content")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|item| item.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("");
        let parsed = extract_json_object(text_content).ok_or_else(|| {
            format!(
                "Could not parse structured decision JSON from {} response.",
                provider_cfg.provider_name
            )
        })?;
        if let Some(ans) = parsed.get("answers").filter(|a| a.is_object()) {
            return Ok(ans.clone());
        }
        return Ok(parsed);
    }

    // Standard OpenAI-compatible `/chat/completions` endpoint (NanoGPT, OpenCode, OpenAI, Gemini, DeepSeek, Groq, Custom, etc.)
    let url = if base.ends_with("/chat/completions") {
        base.to_string()
    } else {
        format!("{}/chat/completions", base)
    };

    let payload = json!({
        "model": provider_cfg.model,
        "temperature": 0.0,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": user_prompt }
        ]
    });

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", effective_key))
        .header("Content-Type", "application/json")
        .header("HTTP-Referer", "http://127.0.0.1:8765")
        .header("X-Title", "SmartRSSReader")
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("{} request error: {}", provider_cfg.provider_name, e))?;

    let status = resp.status();
    let body_text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        let mut detail = body_text.clone();
        if let Ok(err_json) = serde_json::from_str::<Value>(&body_text) {
            if let Some(msg) = err_json
                .get("error")
                .and_then(|e| e.get("message").or(Some(e)))
                .and_then(|m| m.as_str())
            {
                detail = msg.to_string();
            }
        }
        return Err(format!(
            "{} API error ({}) for model '{}': {}",
            provider_cfg.provider_name,
            status.as_u16(),
            provider_cfg.model,
            detail
        ));
    }

    let data: Value = serde_json::from_str(&body_text)
        .map_err(|e| format!("Invalid JSON from {}: {}", provider_cfg.provider_name, e))?;
    let content_str = data
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|msg| msg.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("");

    let parsed = extract_json_object(content_str).ok_or_else(|| {
        format!(
            "Could not parse structured decision JSON from {} response: {}",
            provider_cfg.provider_name, content_str
        )
    })?;
    if let Some(ans) = parsed.get("answers").filter(|a| a.is_object()) {
        return Ok(ans.clone());
    }
    Ok(parsed)
}

pub async fn call_provider_decisions(
    state: &Value,
    questions: &Value,
    api_key: &str,
    provider_cfg: &ActiveProviderConfig,
) -> Result<Value, String> {
    if provider_cfg.requires_key && api_key.trim().is_empty() {
        return Err(format!(
            "{} API key is missing or locked.",
            provider_cfg.provider_name
        ));
    }

    if is_jev_decisions_model(&provider_cfg.provider_id, &provider_cfg.model) {
        return call_jev_decisions(state, questions, api_key, &provider_cfg.model).await;
    }

    call_chat_completions_decisions(state, questions, api_key, provider_cfg).await
}

pub async fn call_jev_decisions(
    state: &Value,
    questions: &Value,
    api_key: &str,
    model: &str,
) -> Result<Value, String> {
    if api_key.trim().is_empty() {
        return Err("OpenRouter API key is missing or locked.".to_string());
    }

    let resolved_model = resolve_model_alias(model);
    let mut candidate_models = vec![resolved_model];
    for &fb in FALLBACK_JEV_MODELS {
        let fb_s = fb.to_string();
        if !candidate_models.contains(&fb_s) {
            candidate_models.push(fb_s);
        }
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let mut last_error = String::new();
    let mut idx = 0usize;
    while idx < candidate_models.len() {
        let candidate = candidate_models[idx].clone();
        let payload = json!({
            "model": candidate,
            "state": state,
            "questions": questions,
        });

        let resp = client
            .post(OPENROUTER_DECISIONS_URL)
            .header("Authorization", format!("Bearer {}", api_key.trim()))
            .header("Content-Type", "application/json")
            .header("HTTP-Referer", "http://127.0.0.1:8765")
            .header("X-Title", "SmartRSSReader")
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("OpenRouter request error: {}", e))?;

        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();

        if status.is_success() {
            let data: Value = serde_json::from_str(&body_text).map_err(|_| {
                "Unexpected response format from OpenRouter Decisions API.".to_string()
            })?;
            if let Some(answers) = data.get("answers") {
                if answers.is_object() {
                    return Ok(answers.clone());
                }
            }
            if data.is_object() {
                return Ok(data);
            }
            return Err("Unexpected response format from OpenRouter Decisions API.".to_string());
        }

        let mut detail = body_text.clone();
        if let Ok(err_json) = serde_json::from_str::<Value>(&body_text) {
            if let Some(msg) = err_json
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
            {
                detail = msg.to_string();
            }
        }
        last_error = format!(
            "OpenRouter Decisions API error ({}) for model '{}': {}",
            status.as_u16(),
            candidate,
            detail
        );

        let lower_detail = detail.to_lowercase();
        let is_model_err = [
            "not a valid model",
            "does not exist",
            "no endpoints found",
            "model not found",
            "invalid model",
        ]
        .iter()
        .any(|tok| lower_detail.contains(tok));

        if !is_model_err {
            return Err(last_error);
        }

        if idx == 0 {
            let live_models = discover_openrouter_decision_models(&client).await;
            for lm in live_models {
                if !candidate_models.contains(&lm) {
                    candidate_models.push(lm);
                }
            }
        }
        idx += 1;
    }

    Err(last_error)
}

pub async fn evaluate_entry_queries_with_provider(
    entry: &Value,
    queries: &[Value],
    api_key: &str,
    provider_cfg: &ActiveProviderConfig,
) -> Result<HashMap<i64, Value>, String> {
    let active_queries: Vec<&Value> = queries
        .iter()
        .filter(|q| {
            q.get("enabled")
                .and_then(|v| v.as_bool().or_else(|| v.as_i64().map(|n| n != 0)))
                .unwrap_or(true)
        })
        .collect();
    if active_queries.is_empty() {
        return Ok(HashMap::new());
    }

    let state = build_entry_state(entry);
    let mut questions_map = serde_json::Map::new();
    let mut query_by_key: HashMap<String, &Value> = HashMap::new();

    for q in active_queries {
        let qid = q.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
        let key = format!("q_{}", qid);
        let spec = build_question_spec(q)?;
        questions_map.insert(key.clone(), spec);
        query_by_key.insert(key, q);
    }

    let answers =
        call_provider_decisions(&state, &Value::Object(questions_map), api_key, provider_cfg)
            .await?;
    let mut results = HashMap::new();

    for (key, q) in query_by_key {
        let qid = q.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
        let qtype = q.get("query_type").and_then(|v| v.as_str()).unwrap_or("noul");
        let raw_ans = answers.get(&key).cloned().unwrap_or_else(|| json!({}));
        let ans_obj = if raw_ans.is_object() {
            raw_ans
        } else if qtype == "noul" {
            json!({ "noul": raw_ans })
        } else {
            json!({ "score": raw_ans })
        };
        let norm = normalize_jev_answer(q, &ans_obj)?;
        results.insert(qid, norm);
    }

    Ok(results)
}

pub async fn evaluate_entry_queries(
    entry: &Value,
    queries: &[Value],
    api_key: &str,
    model: &str,
) -> Result<HashMap<i64, Value>, String> {
    let mut settings = HashMap::new();
    settings.insert("ai_provider".to_string(), "openrouter".to_string());
    settings.insert("jev_model".to_string(), model.to_string());
    let cfg = resolve_active_provider_config(&settings);
    evaluate_entry_queries_with_provider(entry, queries, api_key, &cfg).await
}

pub async fn test_single_query_with_provider(
    entry: &Value,
    query: &Value,
    api_key: &str,
    provider_cfg: &ActiveProviderConfig,
) -> Result<Value, String> {
    let state = build_entry_state(entry);
    let q_spec = build_question_spec(query)?;
    let key = "test_query";
    let questions = json!({ key: q_spec });

    let answers = call_provider_decisions(&state, &questions, api_key, provider_cfg).await?;
    let qtype = query
        .get("query_type")
        .and_then(|v| v.as_str())
        .unwrap_or("noul");
    let raw_ans = answers.get(key).cloned().unwrap_or_else(|| json!({}));
    let ans_obj = if raw_ans.is_object() {
        raw_ans
    } else if qtype == "noul" {
        json!({ "noul": raw_ans })
    } else {
        json!({ "score": raw_ans })
    };

    let norm_info = normalize_jev_answer(query, &ans_obj)?;
    let weight = query.get("weight").and_then(|v| v.as_f64()).unwrap_or(1.0);
    let norm_score = norm_info
        .get("normalized_score")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let weighted_contrib = round4(weight * norm_score);

    Ok(json!({
        "provider_id": provider_cfg.provider_id,
        "provider_name": provider_cfg.provider_name,
        "model": provider_cfg.model,
        "query_name": query.get("name").and_then(|v| v.as_str()).unwrap_or("Test Query"),
        "query_type": qtype,
        "weight": weight,
        "normalized_score": norm_score,
        "weighted_contribution": weighted_contrib,
        "display_value": norm_info.get("display_value").cloned().unwrap_or(Value::Null),
        "raw_response": norm_info.get("raw_response").cloned().unwrap_or(Value::Null),
        "request_preview": {
            "provider": provider_cfg.provider_id,
            "base_url": provider_cfg.base_url,
            "model": provider_cfg.model,
            "state": state,
            "questions": questions,
        }
    }))
}

pub async fn test_single_query_on_entry(
    entry: &Value,
    query: &Value,
    api_key: &str,
    model: &str,
) -> Result<Value, String> {
    let mut settings = HashMap::new();
    settings.insert("ai_provider".to_string(), "openrouter".to_string());
    settings.insert("jev_model".to_string(), model.to_string());
    let cfg = resolve_active_provider_config(&settings);
    test_single_query_with_provider(entry, query, api_key, &cfg).await
}
