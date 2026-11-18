# SmartRSSReader (Multi-Platform Tauri v2 Desktop App)

[![License: CC0-1.0](https://img.shields.io/badge/License-CC0_1.0-lightgrey.svg)](http://creativecommons.org/publicdomain/zero/1.0/)
[![Status: Experimental Alpha](https://img.shields.io/badge/status-alpha%20%2F%20untested-orange.svg)](#)

> [!WARNING]
> **Experimental Alpha Software — Untested & Provided As-Is**
>
> This software is in an **early alpha** stage of development. It is largely **untested** across diverse environments, operating systems, and edge cases. Expect rough edges, breaking changes, potential bugs, or incomplete behavior. Use at your own risk!

A self-contained, multi-platform scientific journal RSS reader and relevance-ranker built with **Tauri v2 (Rust + Webview)**, powered by **OpenRouter's TypeSafe Jev** decision model.

## Key Features

- **Expandable RSS Feeds List + Live Test/Preview**:
  - Inspect, edit, enable/disable, and live-preview entries from any scientific journal RSS/Atom/RDF feed.
- **Expandable Jev Queries (Yes/No & `score`) + Positive/Negative Weights**:
  - Configure binary (`noul`) and multi-level rubric (`score`) queries with positive boosts or negative penalties. Outputs are normalized to `[0.0, 1.0]`, and changing weights immediately recalculates paper rankings offline.
- **Interactive Individual Jev Query Tester**:
  - Pick an RSS feed, pick an entry from that feed, choose a Jev query, and inspect the live Jev probability distribution and weighted score with one click.
- **29+ AI Providers (OpenCode Catalog + NanoGPT + Local + Custom Provider Link)**:
  - Tested exclusively on **OpenRouter, TypeSafe Jev** (`/api/alpha/decisions`) plus structured JSON probability evaluation.
  - Further providers included but untested.
- **Password-Encrypted Multi-Provider API Keys (`AES-256-GCM` + `PBKDF2-HMAC-SHA256`)**:
  - Encrypted at rest in `secrets.enc` (600,000 PBKDF2 iterations, binary-compatible with the Python version) and unlocked into RAM with your password when using the app, storing separate keys per provider under your master password.
- **Incremental Memory (New Entries Only) & 1-Month Archive Retention**:
  - Remembers all seen RSS entries in `smart_rss.db` across both the **Ranked Feed** and the **Archive**, ensuring feed updates only run Jev queries on genuinely new papers. Automatically prunes archived entries older than 30 days on startup.

## Where Your Database Lives & Using Natural Language with AI

On first launch, SmartRSSReader automatically creates a single **Data & AI Skills Folder** named `SmartRSSReader` inside your home directory (unless you choose a custom location via **🤖 AI Skills & Info → Change Folder...**).

Assuming your username is **Jane Doe** (`janedoe`), your SQLite database (`smart_rss.db`) and encrypted API keys (`secrets.enc`) are created by default at:

| Platform | Default Folder | Database File |
| :--- | :--- | :--- |
| **Linux** | `/home/janedoe/SmartRSSReader` | `/home/janedoe/SmartRSSReader/smart_rss.db` |
| **macOS** | `/Users/janedoe/SmartRSSReader` | `/Users/janedoe/SmartRSSReader/smart_rss.db` |
| **Windows** | `C:\Users\janedoe\SmartRSSReader` | `C:\Users\janedoe\SmartRSSReader\smart_rss.db` |

### Manage Feeds & Queries in Natural Language

The exact folder path currently in use is always shown at the top of the app inside the **🤖 AI Skills & Info** panel (with **Copy Path** and **Open Folder** buttons).

Alongside `smart_rss.db`, the app automatically places ready-to-use AI instructions (`AGENTS.md`, `GEMINI.md`, `CLAUDE.md`, `.agents/skills/`) and a local CLI wrapper (`./smart-rss` or `smart-rss.cmd`) directly into that same folder.

To manage your reader with natural language—with zero extra setup:
1. Check the folder path shown in **🤖 AI Skills & Info** (or click **Copy Path**).
2. Open any AI coding assistant (**Antigravity**, **Claude Code**, **Gemini CLI**, **Cursor**, **Codex**, etc.) in that folder.
3. Ask in plain English—for example:
   - *"Add the Nature Neuroscience and Neuron RSS feeds, and test them first."*
   - *"Penalize papers that primarily rely on fMRI or EEG."*
   - *"Boost papers with strong computational modeling."*

Any changes your AI assistant makes to `smart_rss.db` show up **live** in the open SmartRSSReader window.

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

## Acknowledgments

This work was supported in part by complimentary access to Google Antigravity and Gemini 4 Carbon to accelerate scientific workflows.

## License

This project is dedicated to the public domain under **[CC0 1.0 Universal (CC0-1.0)](./LICENSE)**. You may copy, modify, distribute, and use the work, even for commercial purposes, all without asking permission—provided strictly **as-is**, without warranty of any kind.


