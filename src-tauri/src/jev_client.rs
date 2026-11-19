use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;

pub const OPENROUTER_DECISIONS_URL: &str = "https://openrouter.ai/api/alpha/decisions";
pub const OPENROUTER_MODELS_URL: &str =
    "https://openrouter.ai/api/v1/models?output_modalities=decisions";
pub const NANOGPT_SYSTEMONE_URL: &str = "https://api.nano-gpt.com/api/v1/systemone";
pub const NANOGPT_MODELS_URL: &str = "https://nano-gpt.com/api/v1/decision-models";
pub const DEFAULT_PROVIDER_ID: &str = "openrouter";
pub const DEFAULT_JEV_MODEL: &str = "typesafe/jev-1.14";

pub const WIRE_FORMAT_SYSTEMONE: &str = "systemone";
pub const WIRE_FORMAT_OPENAI_DECISIONS: &str = "openai_decisions";

#[derive(Debug, Clone, Serialize)]
pub struct ProviderSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub group: &'static str,
    pub api_url: &'static str,
    pub models_url: &'static str,
    pub portal_url: &'static str,
    pub wire_format: &'static str,
    pub default_model: &'static str,
    pub recommended_models: &'static [&'static str],
    pub requires_key: bool,
    pub description: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveProviderConfig {
    pub provider_id: String,
    pub provider_name: String,
    pub api_url: String,
    pub portal_url: String,
    pub wire_format: String,
    pub model: String,
    pub requires_key: bool,
}

pub const BUILTIN_PROVIDERS: &[ProviderSpec] = &[
    // --- Multi-Model Decision Gateways ---
    ProviderSpec {
        id: "openrouter",
        name: "OpenRouter (Jev, Clef & Decider)",
        group: "Multi-Model Decision Gateways",
        api_url: OPENROUTER_DECISIONS_URL,
        models_url: OPENROUTER_MODELS_URL,
        portal_url: "https://openrouter.ai/keys",
        wire_format: WIRE_FORMAT_SYSTEMONE,
        default_model: "typesafe/jev-1.14",
        recommended_models: &[
            "typesafe/jev-1.14",
            "typesafe/jev-1.13",
            "typesafe/jev-router",
            "cloudflare/clef",
            "cloudflare/clef-flash",
            "perplexity/pplx-decider-v1.3",
        ],
        requires_key: true,
        description: "OpenRouter Decisions API hosting TypeSafe Jev, Cloudflare Clef, and Perplexity Decider models.",
    },
    ProviderSpec {
        id: "nanogpt",
        name: "NanoGPT (Jev, Rune, Kev & Luna)",
        group: "Multi-Model Decision Gateways",
        api_url: NANOGPT_SYSTEMONE_URL,
        models_url: NANOGPT_MODELS_URL,
        portal_url: "https://nano-gpt.com/api",
        wire_format: WIRE_FORMAT_SYSTEMONE,
        default_model: "typesafe/jev-1.14",
        recommended_models: &[
            "typesafe/jev-1.14",
            "typesafe/jev-latest",
            "typesafe/jev-1.13",
            "surogate/rune-26b-a4b",
            "jaredpalmer/kev-9b",
            "openai/gpt-6-luna-decisions",
        ],
        requires_key: true,
        description: "Pay-per-request System One endpoint on NanoGPT supporting TypeSafe Jev, Surogate Rune, Kev, and more.",
    },
    ProviderSpec {
        id: "ofox",
        name: "OfoxAI (System One Gateway)",
        group: "Multi-Model Decision Gateways",
        api_url: "https://api.ofox.ai/typesafe/v1/systemone",
        models_url: "",
        portal_url: "https://ofox.ai",
        wire_format: WIRE_FORMAT_SYSTEMONE,
        default_model: "typesafe/jev-latest",
        recommended_models: &[
            "typesafe/jev-latest",
            "typesafe/jev-1.14",
            "typesafe/jev-1.13",
        ],
        requires_key: true,
        description: "OfoxAI unified System One gateway for TypeSafe Jev decision models.",
    },
    // --- First-Party Decision Model Labs ---
    ProviderSpec {
        id: "typesafe",
        name: "TypeSafe AI (Direct Jev API)",
        group: "First-Party Decision Labs",
        api_url: "https://api.typesafe.ai/v1/systemone",
        models_url: "https://api.typesafe.ai/v1/models",
        portal_url: "https://typesafe.ai",
        wire_format: WIRE_FORMAT_SYSTEMONE,
        default_model: "jev-latest",
        recommended_models: &["jev-latest", "jev-1.14.0", "jev-1.13.0"],
        requires_key: true,
        description: "Official first-party TypeSafe AI /v1/systemone endpoint for Jev models.",
    },
    ProviderSpec {
        id: "perplexity",
        name: "Perplexity Decisions (pplx-decider)",
        group: "First-Party Decision Labs",
        api_url: "https://api.perplexity.ai/v1/decisions",
        models_url: "",
        portal_url: "https://www.perplexity.ai/settings/api",
        wire_format: WIRE_FORMAT_SYSTEMONE,
        default_model: "pplx-decider-latest",
        recommended_models: &[
            "pplx-decider-latest",
            "pplx-decider-v1.3",
            "pplx-decider-flash-latest",
        ],
        requires_key: true,
        description: "Perplexity native /v1/decisions endpoint powered by pplx-decider models.",
    },
    ProviderSpec {
        id: "openai-decisions",
        name: "OpenAI Decisions (gpt-6-luna)",
        group: "First-Party Decision Labs",
        api_url: "https://api.openai.com/v1/decisions",
        models_url: "",
        portal_url: "https://platform.openai.com/api-keys",
        wire_format: WIRE_FORMAT_OPENAI_DECISIONS,
        default_model: "gpt-6-luna",
        recommended_models: &["gpt-6-luna"],
        requires_key: true,
        description: "Official OpenAI /v1/decisions endpoint (predicate & 0-indexed rubric score format) powered by gpt-6-luna.",
    },
    // --- Local, Self-Hosted & Custom Gateways ---
    ProviderSpec {
        id: "local-decider",
        name: "Local Decider / Self-Hosted Clef (127.0.0.1:8080)",
        group: "Local and Custom Gateways",
        api_url: "http://127.0.0.1:8080/v1/decisions",
        models_url: "http://127.0.0.1:8080/v1/models",
        portal_url: "",
        wire_format: WIRE_FORMAT_SYSTEMONE,
        default_model: "pplx-decider-flash",
        recommended_models: &[
            "pplx-decider-flash",
            "cloudflare/clef",
            "cloudflare/clef-flash",
            "surogate/rune-26b-a4b",
        ],
        requires_key: false,
        description: "Local offline decision server (e.g. Decider for Mac, local Cloudflare Clef, or vLLM SystemOne runner — no API key required).",
    },
    ProviderSpec {
        id: "litellm",
        name: "LiteLLM / Bifrost / TrueFoundry Proxy",
        group: "Local and Custom Gateways",
        api_url: "http://127.0.0.1:4000/v1/systemone",
        models_url: "http://127.0.0.1:4000/v1/models",
        portal_url: "",
        wire_format: WIRE_FORMAT_SYSTEMONE,
        default_model: "typesafe/jev-latest",
        recommended_models: &[
            "typesafe/jev-latest",
            "typesafe/jev-1.14",
            "cloudflare/clef",
            "gpt-6-luna",
        ],
        requires_key: false,
        description: "Self-hosted AI gateway exposing /v1/systemone or /v1/decisions (API key optional).",
    },
    ProviderSpec {
        id: "custom",
        name: "Custom Decision Endpoint",
        group: "Local and Custom Gateways",
        api_url: OPENROUTER_DECISIONS_URL,
        models_url: "",
        portal_url: "",
        wire_format: WIRE_FORMAT_SYSTEMONE,
        default_model: DEFAULT_JEV_MODEL,
        recommended_models: &[
            "typesafe/jev-1.14",
            "jev-latest",
            "cloudflare/clef",
            "pplx-decider-latest",
            "gpt-6-luna",
        ],
        requires_key: false,
        description: "Connect any custom System One or OpenAI Decisions API URL and model name (API key optional).",
    },
];

pub fn is_valid_provider_id(id: &str) -> bool {
    let clean = id.trim().to_lowercase();
    BUILTIN_PROVIDERS.iter().any(|p| p.id == clean)
}

pub fn normalize_wire_format(raw: &str) -> &'static str {
    match raw.trim().to_lowercase().as_str() {
        "openai_decisions" | "openai-decisions" | "openai" => WIRE_FORMAT_OPENAI_DECISIONS,
        _ => WIRE_FORMAT_SYSTEMONE,
    }
}

pub fn infer_wire_format_for_url(api_url: &str, fallback: &str) -> &'static str {
    let lower = api_url.trim().to_lowercase();
    if lower.contains("api.openai.com/") && lower.ends_with("/decisions") {
        return WIRE_FORMAT_OPENAI_DECISIONS;
    }
    if lower.ends_with("/systemone") || lower.contains("/api/alpha/decisions") {
        return WIRE_FORMAT_SYSTEMONE;
    }
    normalize_wire_format(fallback)
}

pub fn list_providers_json() -> Vec<Value> {
    BUILTIN_PROVIDERS
        .iter()
        .map(|p| serde_json::to_value(p).unwrap_or_else(|_| json!({})))
        .collect()
}

pub fn get_provider_spec(provider_id: &str) -> &'static ProviderSpec {
    let clean = provider_id.trim().to_lowercase();
    if clean == "openai" {
        return BUILTIN_PROVIDERS
            .iter()
            .find(|p| p.id == "openai-decisions")
            .unwrap_or(&BUILTIN_PROVIDERS[0]);
    }
    BUILTIN_PROVIDERS
        .iter()
        .find(|p| p.id == clean)
        .unwrap_or(&BUILTIN_PROVIDERS[0])
}

pub fn resolve_model_for_provider(provider_id: &str, model: &str) -> String {
    let m = model.trim();
    if m.is_empty() {
        return get_provider_spec(provider_id).default_model.to_string();
    }
    // Only normalize legacy OpenRouter shorthand when OpenRouter is active
    if provider_id == "openrouter" && matches!(m, "typesafe/jev-latest" | "jev-latest") {
        return DEFAULT_JEV_MODEL.to_string();
    }
    m.to_string()
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
    let configured_api_url = settings
        .get("provider_api_url")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let legacy_custom_base = settings
        .get("custom_base_url")
        .map(|s| s.trim())
        .filter(|s| {
            !s.is_empty()
                && s.trim_end_matches('/') != "https://openrouter.ai/api/v1"
                && s.trim_end_matches('/') != "https://nano-gpt.com/api/v1"
        });
    let custom_portal = settings
        .get("custom_portal_url")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    let provider_name = if spec.id == "custom" {
        custom_name.unwrap_or("Custom Decision Endpoint").to_string()
    } else {
        custom_name.unwrap_or(spec.name).to_string()
    };

    let api_url = configured_api_url
        .or(legacy_custom_base)
        .map(|s| s.to_string())
        .unwrap_or_else(|| spec.api_url.to_string());

    let portal_url = custom_portal
        .map(|s| s.to_string())
        .unwrap_or_else(|| spec.portal_url.to_string());

    let explicit_wf = settings
        .get("provider_wire_format")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let wire_format = match explicit_wf {
        Some(wf) => normalize_wire_format(wf).to_string(),
        None => infer_wire_format_for_url(&api_url, spec.wire_format).to_string(),
    };

    let raw_model = settings
        .get("jev_model")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or(spec.default_model);

    let model = resolve_model_for_provider(spec.id, raw_model);

    ActiveProviderConfig {
        provider_id: spec.id.to_string(),
        provider_name,
        api_url,
        portal_url,
        wire_format,
        model,
        requires_key: spec.requires_key,
    }
}

fn round4(val: f64) -> f64 {
    (val * 10000.0).round() / 10000.0
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

pub fn format_state_as_input_text(state: &Value) -> String {
    if let Some(s) = state.as_str() {
        return s.to_string();
    }
    if let Some(obj) = state.as_object() {
        let mut lines = Vec::new();
        for (label, key) in [
            ("Journal", "journal"),
            ("Title", "title"),
            ("Authors", "authors"),
            ("Published", "published_at"),
            ("Abstract", "abstract"),
            ("Link", "link"),
        ] {
            if let Some(val) = obj.get(key).and_then(|v| v.as_str()) {
                if !val.trim().is_empty() {
                    lines.push(format!("{}: {}", label, val.trim()));
                }
            }
        }
        if !lines.is_empty() {
            return lines.join("\n");
        }
    }
    serde_json::to_string_pretty(state).unwrap_or_default()
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
        let arr = criteria.and_then(|c| c.as_array()).ok_or_else(|| {
            format!(
                "Score query '{}' must have at least 2 criteria levels.",
                query.get("name").and_then(|v| v.as_str()).unwrap_or("")
            )
        })?;
        let clean_levels: Vec<String> = arr
            .iter()
            .filter_map(|v| {
                let s = v.as_str().unwrap_or("").trim().to_string();
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
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

/// Converts our canonical System One `{ state, questions }` map into OpenAI `/v1/decisions`
/// request format (`{ model, input, questions: [ { type: "predicate" | "score", name, ... } ] }`).
pub fn build_openai_decisions_payload(
    model: &str,
    state: &Value,
    questions: &Value,
) -> Result<Value, String> {
    let q_obj = questions
        .as_object()
        .ok_or_else(|| "Expected questions map.".to_string())?;
    let mut questions_arr = Vec::with_capacity(q_obj.len());

    for (key, q_spec) in q_obj {
        let qtype = q_spec.get("type").and_then(|v| v.as_str()).unwrap_or("noul");
        let base_instr = q_spec
            .get("instructions")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();

        if qtype == "noul" {
            let mut full_instr = base_instr.to_string();
            if let Some(crit) = q_spec.get("criteria").and_then(|c| c.as_object()) {
                let t = crit.get("true").and_then(|v| v.as_str()).unwrap_or("").trim();
                let f = crit.get("false").and_then(|v| v.as_str()).unwrap_or("").trim();
                if !t.is_empty() || !f.is_empty() {
                    full_instr = format!(
                        "{} (True when: {} | False when: {})",
                        base_instr, t, f
                    );
                }
            }
            questions_arr.push(json!({
                "type": "predicate",
                "name": key,
                "instructions": full_instr,
            }));
        } else if qtype == "score" {
            let levels_raw = q_spec
                .get("criteria")
                .and_then(|c| c.as_array())
                .cloned()
                .unwrap_or_default();
            let levels_objs: Vec<Value> = levels_raw
                .iter()
                .enumerate()
                .map(|(idx, lvl)| {
                    let desc = lvl.as_str().unwrap_or("").trim();
                    json!({
                        "label": format!("level_{}", idx + 1),
                        "description": desc,
                    })
                })
                .collect();
            questions_arr.push(json!({
                "type": "score",
                "name": key,
                "instructions": base_instr,
                "levels": levels_objs,
            }));
        }
    }

    Ok(json!({
        "model": model,
        "input": format_state_as_input_text(state),
        "questions": questions_arr,
    }))
}

/// Normalizes either `{ "answers": { "k": ... } }` (System One) or
/// `{ "answers": [ { "name": "k", ... } ] }` (OpenAI `/v1/decisions`) into a keyed JSON object.
pub fn extract_answers_map_from_response(data: &Value, is_zero_indexed_hint: bool) -> Result<Value, String> {
    if let Some(answers) = data.get("answers") {
        if answers.is_object() {
            if !is_zero_indexed_hint {
                return Ok(answers.clone());
            }
            let mut tagged = answers.as_object().unwrap().clone();
            for (_k, v) in tagged.iter_mut() {
                if let Some(obj) = v.as_object_mut() {
                    obj.insert("__zero_indexed_score".to_string(), Value::Bool(true));
                }
            }
            return Ok(Value::Object(tagged));
        }
        if let Some(arr) = answers.as_array() {
            let mut map = serde_json::Map::new();
            for item in arr {
                if let Some(obj) = item.as_object() {
                    let name_opt = obj
                        .get("name")
                        .or_else(|| obj.get("id"))
                        .or_else(|| obj.get("key"))
                        .and_then(|v| v.as_str());
                    if let Some(name) = name_opt {
                        let mut cloned = obj.clone();
                        cloned.insert("__zero_indexed_score".to_string(), Value::Bool(true));
                        map.insert(name.to_string(), Value::Object(cloned));
                    }
                }
            }
            return Ok(Value::Object(map));
        }
    }
    if data.is_object() {
        return Ok(data.clone());
    }
    Err("Unexpected response format from Decisions API.".to_string())
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
    let qweight = query.get("weight").and_then(|v| v.as_f64()).unwrap_or(1.0);
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
        let raw_score = answer
            .get("score")
            .or_else(|| answer.get("expected_score"));
        let zero_indexed_hint = answer
            .get("__zero_indexed_score")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let mut clean_probs: Vec<f64> = Vec::new();
        if let Some(arr) = probs.and_then(|p| p.as_array()) {
            if arr.len() == k {
                clean_probs = arr
                    .iter()
                    .map(|p| {
                        if let Some(f) = value_as_f64(p) {
                            f.max(0.0)
                        } else if let Some(p_obj) = p.as_object() {
                            p_obj
                                .get("probability")
                                .or_else(|| p_obj.get("prob"))
                                .or_else(|| p_obj.get("p"))
                                .and_then(value_as_f64)
                                .unwrap_or(0.0)
                                .max(0.0)
                        } else {
                            0.0
                        }
                    })
                    .collect();
            }
        } else if let Some(obj) = probs.and_then(|p| p.as_object()) {
            clean_probs = levels
                .iter()
                .enumerate()
                .map(|(i, lvl)| {
                    obj.get(lvl)
                        .or_else(|| obj.get(&i.to_string()))
                        .or_else(|| {
                            let lower_lvl = lvl.to_lowercase();
                            obj.iter()
                                .find(|(ok, _)| ok.to_lowercase() == lower_lvl)
                                .map(|(_, ov)| ov)
                        })
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
                    let val = rs_str.parse::<f64>().unwrap_or(if zero_indexed_hint { 0.0 } else { 1.0 });
                    if zero_indexed_hint {
                        let n = val / ((k - 1) as f64);
                        let b = (val.round() as isize).clamp(0, (k - 1) as isize) as usize;
                        (n, val + 1.0, b)
                    } else if (1.0..=(k as f64)).contains(&val) {
                        let n = (val - 1.0) / ((k - 1) as f64);
                        let b = ((val - 1.0).round() as isize).clamp(0, (k - 1) as isize) as usize;
                        (n, val, b)
                    } else {
                        let n = val / ((k - 1) as f64);
                        let b = (val.round() as isize).clamp(0, (k - 1) as isize) as usize;
                        (n, val + 1.0, b)
                    }
                }
            } else {
                let val = value_as_f64(rs).unwrap_or(if zero_indexed_hint { 0.0 } else { 1.0 });
                if zero_indexed_hint {
                    let n = val / ((k - 1) as f64);
                    let b = (val.round() as isize).clamp(0, (k - 1) as isize) as usize;
                    (n, val + 1.0, b)
                } else if (1.0..=(k as f64)).contains(&val) {
                    let n = (val - 1.0) / ((k - 1) as f64);
                    let b = ((val - 1.0).round() as isize).clamp(0, (k - 1) as isize) as usize;
                    (n, val, b)
                } else {
                    let n = val / ((k - 1) as f64);
                    let b = (val.round() as isize).clamp(0, (k - 1) as isize) as usize;
                    (n, val + 1.0, b)
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

        // Always normalize legend to a clean ordered string array so the UI probability bars render across all providers
        let legend_out = json!(levels);

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

fn derive_models_urls(provider_cfg: &ActiveProviderConfig) -> Vec<String> {
    let api_url = provider_cfg.api_url.trim().trim_end_matches('/');
    let mut urls = Vec::new();
    if api_url.contains("openrouter.ai") {
        urls.push(OPENROUTER_MODELS_URL.to_string());
        return urls;
    }
    if api_url.contains("nano-gpt.com") {
        urls.push(NANOGPT_MODELS_URL.to_string());
        urls.push("https://api.nano-gpt.com/api/v1/decision-models".to_string());
        return urls;
    }
    let spec = get_provider_spec(&provider_cfg.provider_id);
    if !spec.models_url.is_empty() {
        urls.push(spec.models_url.to_string());
    }
    if let Some(prefix) = api_url
        .strip_suffix("/decisions")
        .or_else(|| api_url.strip_suffix("/systemone"))
    {
        let derived = format!("{}/models", prefix);
        if !urls.contains(&derived) {
            urls.push(derived);
        }
    }
    urls
}

async fn discover_decision_models_at(
    client: &reqwest::Client,
    models_url: &str,
    api_key: Option<&str>,
) -> Vec<String> {
    let mut req = client
        .get(models_url)
        .header("Accept", "application/json")
        .timeout(Duration::from_secs(10));
    if let Some(k) = api_key {
        if !k.trim().is_empty() {
            req = req.header("Authorization", format!("Bearer {}", k.trim()));
        }
    }
    if let Ok(resp) = req.send().await {
        if resp.status().is_success() {
            if let Ok(data) = resp.json::<Value>().await {
                let arr_opt = data
                    .get("data")
                    .and_then(|d| d.as_array())
                    .or_else(|| data.get("models").and_then(|d| d.as_array()))
                    .or_else(|| data.as_array());
                if let Some(arr) = arr_opt {
                    let ids: Vec<String> = arr
                        .iter()
                        .take(120)
                        .filter_map(|item| {
                            item.get("id")
                                .and_then(|v| v.as_str())
                                .or_else(|| item.get("model").and_then(|v| v.as_str()))
                                .or_else(|| item.get("name").and_then(|v| v.as_str()))
                                .or_else(|| item.as_str())
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                        })
                        .collect();
                    let preferred: Vec<String> = ids
                        .iter()
                        .filter(|m| {
                            let l = m.to_lowercase();
                            (l.contains("jev")
                                || l.contains("clef")
                                || l.contains("decider")
                                || l.contains("rune")
                                || l.contains("luna")
                                || l.contains("kev"))
                                && !l.contains("router")
                        })
                        .cloned()
                        .collect();
                    let other_ids: Vec<String> = ids
                        .into_iter()
                        .filter(|m| !preferred.contains(m))
                        .collect();
                    let mut combined = preferred;
                    combined.extend(other_ids);
                    return combined;
                }
            }
        }
    }
    Vec::new()
}

fn is_loopback_endpoint(url: &str) -> bool {
    let lower = url.trim().to_lowercase();
    lower.starts_with("http://127.0.0.1")
        || lower.starts_with("https://127.0.0.1")
        || lower.starts_with("http://localhost")
        || lower.starts_with("https://localhost")
        || lower.starts_with("http://[::1]")
        || lower.starts_with("https://[::1]")
}

fn build_http_client_for_url(url: &str, timeout: Duration) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder().timeout(timeout);
    if is_loopback_endpoint(url) {
        builder = builder.no_proxy();
    }
    builder
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))
}

pub async fn fetch_provider_models(
    provider_cfg: &ActiveProviderConfig,
    api_key: Option<&str>,
) -> Result<(Vec<String>, &'static str), String> {
    let spec = get_provider_spec(&provider_cfg.provider_id);
    let mut models: Vec<String> = spec
        .recommended_models
        .iter()
        .map(|s| s.to_string())
        .collect();

    let client = build_http_client_for_url(&provider_cfg.api_url, Duration::from_secs(10))?;

    let mut source = "curated_catalog";
    for models_url in derive_models_urls(provider_cfg) {
        let live = discover_decision_models_at(&client, &models_url, api_key).await;
        if !live.is_empty() {
            source = "live_endpoint";
            let mut merged = live;
            for rec in models {
                if !merged.contains(&rec) {
                    merged.push(rec);
                }
            }
            models = merged;
            break;
        }
    }

    Ok((models, source))
}

pub async fn call_provider_decisions(
    state: &Value,
    questions: &Value,
    api_key: &str,
    provider_cfg: &ActiveProviderConfig,
) -> Result<Value, String> {
    call_jev_decisions(state, questions, api_key, provider_cfg).await
}

pub async fn call_jev_decisions(
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

    let endpoint = provider_cfg.api_url.trim();
    if endpoint.is_empty()
        || (!endpoint.starts_with("http://") && !endpoint.starts_with("https://"))
    {
        return Err(format!(
            "Invalid Provider Decisions API URL '{}'. Please configure a valid http:// or https:// URL in Settings.",
            endpoint
        ));
    }

    let model = resolve_model_for_provider(&provider_cfg.provider_id, &provider_cfg.model);
    if model.trim().is_empty() {
        return Err("Decision model name cannot be empty. Please set a model name in Settings.".to_string());
    }

    let wire_format = normalize_wire_format(&provider_cfg.wire_format);
    let is_openai_decisions = wire_format == WIRE_FORMAT_OPENAI_DECISIONS;

    let payload = if is_openai_decisions {
        build_openai_decisions_payload(&model, state, questions)?
    } else {
        json!({
            "model": model,
            "state": state,
            "questions": questions,
        })
    };

    let client = build_http_client_for_url(endpoint, Duration::from_secs(30))?;

    let mut req = client
        .post(endpoint)
        .header("Content-Type", "application/json")
        .json(&payload);

    // Attach OpenRouter identification headers only for OpenRouter so strict gateways never reject unknown headers
    if endpoint.contains("openrouter.ai") {
        req = req
            .header("HTTP-Referer", "http://127.0.0.1:8765")
            .header("X-Title", "SmartRSSReader");
    }

    if !api_key.trim().is_empty() {
        req = req.header("Authorization", format!("Bearer {}", api_key.trim()));
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("{} request error ({}): {}", provider_cfg.provider_name, endpoint, e))?;

    let status = resp.status();
    let body_text = resp.text().await.unwrap_or_default();

    if status.is_success() {
        let data: Value = serde_json::from_str(&body_text).map_err(|_| {
            format!(
                "Unexpected non-JSON response from {} ({}).",
                provider_cfg.provider_name, endpoint
            )
        })?;
        return extract_answers_map_from_response(&data, is_openai_decisions);
    }

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

    let lower_detail = detail.to_lowercase();
    let is_model_err = [
        "not a valid model",
        "does not exist",
        "no endpoints found",
        "model not found",
        "invalid model",
        "unknown model",
    ]
    .iter()
    .any(|tok| lower_detail.contains(tok));

    let mut err_msg = format!(
        "{} API error ({}) at '{}' for model '{}': {}",
        provider_cfg.provider_name,
        status.as_u16(),
        endpoint,
        model,
        detail
    );

    if is_model_err {
        for models_url in derive_models_urls(provider_cfg) {
            let live_models = discover_decision_models_at(
                &client,
                &models_url,
                if api_key.trim().is_empty() {
                    None
                } else {
                    Some(api_key)
                },
            )
            .await;
            if !live_models.is_empty() {
                let preview: Vec<String> = live_models.into_iter().take(6).collect();
                err_msg.push_str(&format!(
                    " (Available decision models reported by endpoint: {})",
                    preview.join(", ")
                ));
                break;
            }
        }
    }

    Err(err_msg)
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
    settings.insert(
        "provider_api_url".to_string(),
        OPENROUTER_DECISIONS_URL.to_string(),
    );
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

    let wire_format = normalize_wire_format(&provider_cfg.wire_format);
    let request_preview = if wire_format == WIRE_FORMAT_OPENAI_DECISIONS {
        let mut p = build_openai_decisions_payload(&provider_cfg.model, &state, &questions)?;
        if let Some(obj) = p.as_object_mut() {
            obj.insert("provider".to_string(), json!(provider_cfg.provider_id));
            obj.insert("api_url".to_string(), json!(provider_cfg.api_url));
            obj.insert("wire_format".to_string(), json!(wire_format));
        }
        p
    } else {
        json!({
            "provider": provider_cfg.provider_id,
            "api_url": provider_cfg.api_url,
            "wire_format": wire_format,
            "model": provider_cfg.model,
            "state": state,
            "questions": questions,
        })
    };

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
        "api_url": provider_cfg.api_url,
        "wire_format": wire_format,
        "model": provider_cfg.model,
        "query_name": query.get("name").and_then(|v| v.as_str()).unwrap_or("Test Query"),
        "query_type": qtype,
        "weight": weight,
        "normalized_score": norm_score,
        "weighted_contribution": weighted_contrib,
        "display_value": norm_info.get("display_value").cloned().unwrap_or(Value::Null),
        "raw_response": norm_info.get("raw_response").cloned().unwrap_or(Value::Null),
        "request_preview": request_preview,
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
    settings.insert(
        "provider_api_url".to_string(),
        OPENROUTER_DECISIONS_URL.to_string(),
    );
    settings.insert("jev_model".to_string(), model.to_string());
    let cfg = resolve_active_provider_config(&settings);
    test_single_query_with_provider(entry, query, api_key, &cfg).await
}
