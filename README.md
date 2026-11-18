# SmartRSSReader

[![Download Latest Release](https://img.shields.io/github/v/release/dylanfesta/RSS-Sci-Jev-Reader?include_prereleases&label=Download%20Desktop%20App&style=for-the-badge&logo=github&color=2ea44f)](https://github.com/dylanfesta/RSS-Sci-Jev-Reader/releases/latest)

[![License: CC0-1.0](https://img.shields.io/badge/License-CC0_1.0-lightgrey.svg)](http://creativecommons.org/publicdomain/zero/1.0/)
[![Status: Experimental Alpha](https://img.shields.io/badge/status-alpha%20%2F%20untested-orange.svg)](https://github.com/dylanfesta/RSS-Sci-Jev-Reader/releases/latest)
[![Platforms](https://img.shields.io/badge/platforms-Windows%20%7C%20macOS%20%7C%20Linux-blue.svg)](#download-pre-built-desktop-app)

**SmartRSSReader** is a local desktop app that turns noisy scientific feeds into a personalized, ranked reading list in two simple steps:

1. **Select the RSS feeds** of all the scientific journals and preprint servers you follow (e.g., *Nature*, *Science*, *Neuron*, *arXiv*, *bioRxiv*).
2. **Define an arbitrary number of selection criteria**—both positive boosts (topics or methods you care about) and negative penalties (topics or methods you want to filter out)—to give every new paper a combined relevance score.

### Why is it so fast and cheap? (What is TypeSafe Jev?)

Instead of asking a slow, chatty Large Language Model to write essays or JSON about every paper, SmartRSSReader uses **[TypeSafe Jev](https://openrouter.ai/typesafe)** (via OpenRouter)—a specialized decision model built strictly for structured evaluation rather than text generation.

Given a paper's title and abstract alongside your custom criteria, Jev skips prose generation entirely and directly outputs calibrated probabilities (for Yes/No questions or multi-level rubrics) in a single pass. Because it doesn't waste tokens "talking," scoring hundreds of newly published papers against multiple criteria is **extremely fast and costs fractions of a cent**—plus SmartRSSReader remembers every paper it has already seen, so you never pay to score the same paper twice, and changing criterion weights re-ranks your entire feed instantly offline.

> [!WARNING]
> **Experimental Alpha Software & API Spending Limits**
>
> - **API Key Safety:** In my testing, running all queries combined across my feeds did not cost more than **~5 cents**. However, because this software is **not thoroughly tested** (a bug or misconfigured feed could theoretically trigger unexpected requests), I strongly recommend **always using API keys that have a maximum spending limit** (for example, setting a small USD credit cap on your OpenRouter key).
> - **Early Alpha Status:** Expect rough edges, breaking changes, or incomplete behavior across diverse environments and operating systems. Provided strictly as-is—use at your own risk!

## Download Pre-Built Desktop App

No Rust or Node.js setup required—grab the pre-built installer for your operating system directly below, or browse [**All Releases & Changelogs →**](https://github.com/dylanfesta/RSS-Sci-Jev-Reader/releases/latest).

| Operating System | Primary Installer (1-Click Download) | Alternative Formats |
| :--- | :--- | :--- |
| **Windows** (x64) | [**⬇ Download `.exe` Setup**](https://github.com/dylanfesta/RSS-Sci-Jev-Reader/releases/latest/download/SmartRSSReader_Windows_x64-setup.exe) | [`.msi` Installer](https://github.com/dylanfesta/RSS-Sci-Jev-Reader/releases/latest/download/SmartRSSReader_Windows_x64.msi) |
| **macOS** (Apple Silicon M1–M4) | [**⬇ Download `.dmg` (Apple Silicon)**](https://github.com/dylanfesta/RSS-Sci-Jev-Reader/releases/latest/download/SmartRSSReader_macOS_AppleSilicon.dmg) | [`.app.tar.gz`](https://github.com/dylanfesta/RSS-Sci-Jev-Reader/releases/latest) |
| **macOS** (Intel) | [**⬇ Download `.dmg` (Intel x64)**](https://github.com/dylanfesta/RSS-Sci-Jev-Reader/releases/latest/download/SmartRSSReader_macOS_Intel.dmg) | [`.app.tar.gz`](https://github.com/dylanfesta/RSS-Sci-Jev-Reader/releases/latest) |
| **Linux** (x86_64) | [**⬇ Download `.deb` (Ubuntu / Debian)**](https://github.com/dylanfesta/RSS-Sci-Jev-Reader/releases/latest/download/SmartRSSReader_Linux_amd64.deb) | [`.AppImage` (Universal)](https://github.com/dylanfesta/RSS-Sci-Jev-Reader/releases/latest/download/SmartRSSReader_Linux_amd64.AppImage) · [`.rpm` (Fedora / RHEL)](https://github.com/dylanfesta/RSS-Sci-Jev-Reader/releases/latest/download/SmartRSSReader_Linux_x86_64.rpm) |

<details>
<summary><strong>First-launch notes (macOS Gatekeeper / Windows SmartScreen / Linux AppImage)</strong></summary>

Because these alpha binaries are not code-signed with paid Apple/Microsoft developer certificates yet:
- **macOS**: After dragging `SmartRSSReader.app` to `Applications`, right-click (Control-click) the app and choose **Open** (or run `xattr -cr /Applications/SmartRSSReader.app` if macOS reports it is "damaged" or from an unidentified developer).
- **Windows**: If Windows Defender SmartScreen appears, click **More info** → **Run anyway**.
- **Linux `.AppImage`**: Make the file executable before launching (`chmod +x SmartRSSReader_Linux_amd64.AppImage && ./SmartRSSReader_Linux_amd64.AppImage`).

</details>

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


