# SmartRSSReader — Architecture & AI Agent Guide

**SmartRSSReader** is a local scientific journal RSS reader and relevance ranker powered by **OpenRouter's TypeSafe Jev** decision model (`typesafe/jev-1.14` via `POST https://openrouter.ai/api/alpha/decisions`).

It allows researchers and AI agents to curate scientific RSS/Atom feeds, define structured relevance queries (`noul` Yes/No probabilities and `score` ordered rubrics) with **positive or negative weights**, and rank newly published papers while remembering all previously seen or archived papers.

---

## 1. Self-Contained Data & AI Skills Folder

SmartRSSReader stores its database, encrypted API key, local CLI wrapper (`./smart-rss` / `smart-rss.cmd`), and AI Agent skills (`AGENTS.md`, `GEMINI.md`, `CLAUDE.md`, `.agents/skills/*`) together in a single **Data & AI Skills Folder** (by default `~/SmartRSSReader`, or a custom folder chosen in the app's **🤖 AI Skills & Info** frame).

```text
~/SmartRSSReader/                   # Active Data & AI Skills Folder
├── smart_rss.db                    # SQLite WAL database (feeds, queries, entries, scores)
├── secrets.enc                     # PBKDF2-HMAC-SHA256 + AES-256-GCM encrypted OpenRouter API key
├── smart-rss                       # Local CLI wrapper script (Linux / macOS)
├── smart-rss.cmd                   # Local CLI wrapper script (Windows)
├── AGENTS.md                       # Architecture & AI Agent instructions
├── GEMINI.md                       # Antigravity / Gemini CLI instructions
├── CLAUDE.md                       # Claude Code instructions
└── .agents/
    └── skills/
        ├── manage-rss-feeds/       # Skill for adding, testing, and managing RSS feeds
        │   └── SKILL.md
        └── manage-jev-queries/     # Skill for creating, editing, and weighting Jev queries
            └── SKILL.md
```

### Key Architectural Principles
1. **SQLite WAL Mode Concurrency (`smart_rss.db`)**:
   - The database uses `PRAGMA journal_mode=WAL;`.
   - When an AI agent runs CLI commands (`./smart-rss feeds ...` or `./smart-rss queries ...`), changes are written directly to `smart_rss.db` and are **immediately live** in the open Tauri desktop window via an automatic SQLite change watcher.
2. **Incremental Memory (New Entries Only) & 1-Month Archive Retention**:
   - Every RSS entry is hashed into a deterministic `entry_uid` (from its DOI, GUID, or canonical link).
   - Both `status = 'active'` and `status = 'archived'` entries remain in the `entries` table. Future feed updates check `get_known_entry_uids()` and **only evaluate genuinely new papers** with Jev.
   - When the app is opened, any archived entry older than 1 month (30 days since `archived_at` / `created_at`) is automatically pruned.
3. **Normalized Weighted Sum & Negative Weights**:
   - Every Jev query $q$ produces a normalized score $s_{e,q} \in [0.0, 1.0]$ stored in `entry_scores`:
     - **`noul` (Yes/No)**: $s_{e,q} = P(\text{true}) \in [0.0, 1.0]$.
     - **`score` (Ordered Rubric with $K \ge 2$ levels)**: $s_{e,q} = \frac{\sum_{i=0}^{K-1} i \cdot p_i}{K - 1} \in [0.0, 1.0]$.
   - Each query has a weight $w_q \in \mathbb{R}$:
     - **Positive weight ($w_q > 0$)**: Boosts papers matching the query.
     - **Negative weight ($w_q < 0$)**: Penalizes/subtracts from the ranking when a paper matches an unwanted topic or methodology (e.g., `Uses fMRI or EEG (Penalty)` with $w_q = -1.0$).
   - Total score $R_{\text{raw}}(e) = \sum_{q} w_q \cdot s_{e,q}$ is computed dynamically on read, so **changing any query weight re-ranks all papers instantly without calling OpenRouter**.
4. **Historical Query Snapshots (`entry_scores`)**:
   - When an article is evaluated, `entry_scores` stores the complete snapshot of each query at the time the test was run (`query_name_snapshot`, `query_type_snapshot`, `weight_snapshot`, `weighted_score_snapshot`, `raw_outcome_text`, `confidence`, and `query_snapshot_json` without a cascading foreign key to `queries`).
   - If a query's name, instructions, or criteria are later modified (or the query is deleted), already-evaluated articles preserve and display their exact query breakdown from the time their test was run.

---

## 2. Using AI Agents to Manage Feeds & Jev Queries

AI agents operating inside the Data & AI Skills Folder (or repository root) should use the **`./smart-rss` Agent CLI** (which works whether the Tauri desktop app is running or closed and always outputs structured JSON), or the idempotent HTTP endpoints when the desktop app is open (`127.0.0.1:8765`).

### A. Managing RSS Feeds (`manage-rss-feeds` skill)

```bash
# 1. List all configured RSS feeds
./smart-rss feeds list

# 2. Test / preview a feed URL before adding
./smart-rss feeds test --url "https://www.nature.com/neuro.rss" --max-entries 5

# 3. Add or update (upsert by URL) an RSS feed (auto-detects title if --title omitted)
./smart-rss feeds add \
  --url "https://www.nature.com/neuro.rss" \
  --title "Nature Neuroscience" \
  --category "Neuroscience" \
  --test

# 4. Remove a feed by URL or ID
./smart-rss feeds remove --url "https://www.nature.com/neuro.rss"
```

### B. Managing Jev Queries & Weights (`manage-jev-queries` skill)

```bash
# 1. List all Jev queries and their current weights
./smart-rss queries list

# 2. Add or update a Yes/No (`noul`) query (positive or negative weight)
./smart-rss queries set \
  --name "Uses fMRI or EEG (Penalty)" \
  --type noul \
  --weight -1.0 \
  --instructions "Does this paper primarily rely on or report functional MRI (fMRI) or electroencephalography (EEG) experiments/data?" \
  --true-desc "The study uses, analyzes, or focuses on fMRI or EEG data." \
  --false-desc "The study does not involve fMRI or EEG."

# 3. Add or update an Ordered Rubric (`score`) query (2+ levels from lowest to highest)
./smart-rss queries set \
  --name "Computational / AI Methodology Depth" \
  --type score \
  --weight 2.0 \
  --instructions "Rate the depth of computational modeling, machine learning, or quantitative data analysis in this paper." \
  --levels \
    "No computational, statistical, or modeling focus mentioned" \
    "Uses standard statistical or bioinformatics tools as a minor component" \
    "Develops or heavily applies advanced computational models, algorithms, or AI"

# 4. Quickly change a query's weight by name (immediately re-calculates rankings!)
./smart-rss queries weight --name "Uses fMRI or EEG (Penalty)" --weight -1.5

# 5. Remove a query by name or ID
./smart-rss queries remove --name "Uses fMRI or EEG (Penalty)"
```

### C. Managing AI Providers & Custom Provider Links

SmartRSSReader supports **29+ AI providers** (OpenRouter, NanoGPT, OpenCode Zen, OpenAI, Anthropic, Google Gemini, DeepSeek, xAI Grok, Mistral, Groq, Cerebras, Together AI, Fireworks AI, Venice AI, Chutes AI, SiliconFlow, Z.ai, Perplexity, DeepInfra, Hyperbolic, Featherless AI, SambaNova, Nebius, NVIDIA NIM, GitHub Models, Novita AI, Ollama, LM Studio, and **Custom / Own Provider** with custom API Base URL and General Link).

```bash
# 1. List all 29+ built-in providers and the currently active provider configuration
./smart-rss providers list

# 2. Switch to NanoGPT (or OpenRouter, OpenCode, Groq, DeepSeek, Ollama, etc.)
./smart-rss providers set --provider nanogpt --model "deepseek/deepseek-chat"

# 3. Configure your own Custom Provider with a General Link (Dashboard / Keys / Docs URL)
./smart-rss providers set \
  --provider custom \
  --custom-name "My Lab vLLM Gateway" \
  --base-url "https://llm.mylab.org/v1" \
  --portal-url "https://llm.mylab.org/dashboard/keys" \
  --model "qwen-2.5-72b-instruct"
```

### D. Inspecting or Resetting Ranked Entries

```bash
# View top 10 ranked active entries as JSON
./smart-rss entries list --status active --limit 10

# Reset all RSS entries (active & archived) to start over blank
./smart-rss entries reset
```

---

## 3. HTTP REST API for Agents (When Desktop App is Open on `127.0.0.1:8765`)

- `GET /api/feeds` — List all RSS feeds.
- `POST /api/agent/feeds` — Idempotent upsert by `url`:
  ```json
  {
    "url": "https://www.nature.com/neuro.rss",
    "title": "Nature Neuroscience",
    "category": "Neuroscience",
    "enabled": true,
    "validate_first": true
  }
  ```
- `GET /api/queries` — List all Jev queries.
- `POST /api/agent/queries` — Idempotent upsert by `name` or `id` (supports partial updates like changing only `weight`):
  ```json
  {
    "name": "Uses fMRI or EEG (Penalty)",
    "query_type": "noul",
    "weight": -1.0,
    "instructions": "Does this paper primarily rely on or report functional MRI (fMRI) or electroencephalography (EEG) experiments/data?",
    "criteria": {
      "true": "The study uses, analyzes, or focuses on fMRI or EEG data.",
      "false": "The study does not involve fMRI or EEG."
    }
  }
  ```
- `GET /api/entries?status=active` — List ranked entries with per-query score breakdowns.
- `POST /api/entries/reset-all` — Delete all active and archived RSS entries and Jev scores so the next feed update re-reads and re-scores from scratch.
