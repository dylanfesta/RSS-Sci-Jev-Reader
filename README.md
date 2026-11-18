# SmartRSSReader (Multi-Platform Tauri v2 Desktop App)

A self-contained, multi-platform (**Linux, macOS, Windows**) scientific journal RSS reader and relevance ranker built with **Tauri v2 (Rust + Webview)**, powered by **OpenRouter's TypeSafe Jev** decision model (`typesafe/jev-1.14` via `POST https://openrouter.ai/api/alpha/decisions`).

Fully equivalent in every aspect to the reference `SmartRSSReader` implementation—including dark/light mode, SQLite WAL persistence, 30-day archive memory, historical query snapshots, and AES-256-GCM API key encryption—while providing a **self-contained Data & AI Skills Folder** (`~/SmartRSSReader` by default) so AI agents can interact with your feeds and Jev queries without installing system-wide skills.

## Key Features

- **Self-Contained Data & AI Skills Folder (`~/SmartRSSReader`)**:
  - Automatically provisions `smart_rss.db` (SQLite WAL), `secrets.enc` (encrypted API key), `AGENTS.md`, `GEMINI.md`, `CLAUDE.md`, `.agents/skills/manage-rss-feeds/SKILL.md`, `.agents/skills/manage-jev-queries/SKILL.md`, and local `./smart-rss` (`smart-rss.cmd` on Windows) CLI wrappers inside the same folder that stores your data.
  - Open any AI coding assistant (Antigravity, Claude Code, Cursor, Codex, Gemini CLI) inside your Data Folder and ask it in natural language to add feeds, test journals, or tune Jev queries—zero system-wide skill installation required.
  - Includes a live SQLite WAL file watcher that automatically refreshes the open Tauri app window whenever an AI agent updates feeds or query weights.
- **Expandable "🤖 AI Skills & Info" Frame**:
  - Appears automatically on first launch (and can be expanded/collapsed anytime from the header bar) to show your Data & AI Skills Folder path, one-click folder actions (**Copy Path**, **Open Folder**, **Change Folder**, **Re-sync Skills**), copyable AI prompts, and an overview of the Jev ranking engine.
- **29+ AI Providers (OpenCode Catalog + NanoGPT + Local + Custom Provider Link)**:
  - Native **OpenRouter TypeSafe Jev** (`/api/alpha/decisions`) plus structured JSON probability evaluation across **NanoGPT** (`https://nano-gpt.com/api/v1`), **OpenCode Zen** (`https://opencode.ai/zen/v1`), **OpenAI**, **Anthropic**, **Google Gemini**, **DeepSeek**, **xAI Grok**, **Mistral**, **Groq**, **Cerebras**, **Together AI**, **Fireworks AI**, **Venice AI**, **Chutes AI**, **SiliconFlow**, **Z.ai / GLM**, **Perplexity**, **DeepInfra**, **Hyperbolic**, **Featherless AI**, **SambaNova**, **Nebius**, **NVIDIA NIM**, **GitHub Models**, **Novita AI**, **Ollama** (local), **LM Studio** (local), and **Custom / Own Provider** (with configurable **API Base URL** and **General Link for Your Provider** to open your dashboard, API keys, or docs in one click).
- **Password-Encrypted Multi-Provider API Keys (`AES-256-GCM` + `PBKDF2-HMAC-SHA256`)**:
  - Encrypted at rest in `secrets.enc` (600,000 PBKDF2 iterations, binary-compatible with the Python version) and unlocked into RAM with your password when using the app, storing separate keys per provider under your master password, plus cross-platform OS Keyring support (Linux GNOME Keyring / `libsecret`, macOS Keychain, Windows Credential Manager).
- **Incremental Memory (New Entries Only) & 1-Month Archive Retention**:
  - Remembers all seen RSS entries in `smart_rss.db` across both the **Ranked Feed** and the **Archive**, ensuring feed updates only run Jev queries on genuinely new papers. Automatically prunes archived entries older than 30 days on startup.
- **Expandable RSS Feeds List + Live Test/Preview**:
  - Inspect, edit, enable/disable, and live-preview entries from any scientific journal RSS/Atom/RDF feed.
- **Expandable Jev Queries (`noul` Yes/No & `score` Rubric) + Positive/Negative Weights**:
  - Configure binary (`noul`) and multi-level rubric (`score`) queries with positive boosts or negative penalties. Outputs are normalized to `[0.0, 1.0]`, and changing weights immediately recalculates paper rankings offline.
- **Interactive Individual Jev Query Tester**:
  - Pick an RSS feed, pick an entry from that feed, choose a Jev query, and inspect the live Jev probability distribution and weighted score with one click.

## Quick Start (Development & Build)

```bash
# 1. Run the Rust unit & integration test suite
cd src-tauri && cargo test

# 2. Launch the Tauri desktop app in development mode
cargo run --manifest-path src-tauri/Cargo.toml

# 3. Or use the headless Agent CLI directly
./src-tauri/target/debug/smart-rss-reader feeds list
./src-tauri/target/debug/smart-rss-reader queries list
```
