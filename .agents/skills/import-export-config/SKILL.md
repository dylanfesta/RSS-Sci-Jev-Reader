---
name: import-export-config
description: >-
  Export or import SmartRSSReader RSS feeds (including name, URL, category, and tags)
  and Jev filter/ranking rules as a clean, human-readable TOML file. Use this skill
  whenever the user asks to back up, export, share, migrate, or import their RSS feed
  list and filter rules to/from a TOML file.
---

# Import & Export Configuration as TOML (`import-export-config`)

This skill allows you to export all configured scientific RSS feeds (including `name`, `url`, `category`, `tags`, and `enabled`) and Jev filter rules (`noul` Yes/No queries and `score` ordered rubrics) to a clean, human-readable `.toml` file—and import `.toml` configuration files back into `smart_rss.db` with safe conflict resolution.

## 1. Export Feeds & Filter Rules to a TOML File

Run `config export` to write a formatted, commented `.toml` file:

```bash
# Export to a specific file path
./smart-rss config export --file ./my_feeds_and_rules.toml

# Or export to the default path (<data_dir>/smart_rss_config.toml)
./smart-rss config export

# Or print the raw TOML directly to stdout
./smart-rss config export --stdout
```

The exported TOML contains only portable feeds and filter rules—**never** API keys or secrets.

### TOML Schema Reference

```toml
version = 1

[[feeds]]
name = "Nature Neuroscience"
url = "https://www.nature.com/neuro.rss"
category = "Neuroscience"
tags = ["neuroscience", "peer-reviewed"]
enabled = true

[[rules]]
name = "Neuroscience Paper"
type = "noul"
weight = 2.0
enabled = true
instructions = "Is the topic of this paper neuroscience or computational neuroscience?"
true_description = "The paper investigates neuroscience or neural network dynamics."
false_description = "The paper is outside neuroscience."

[[rules]]
name = "Computational Depth"
type = "score"
weight = 1.5
enabled = true
instructions = "Rate the depth of computational modeling or machine learning."
levels = [
  "No computational or modeling focus",
  "Uses standard statistical tools as a minor component",
  "Develops or heavily applies advanced computational models or AI",
]
```

## 2. Import from a TOML File (Safe Conflict-Resolution Workflow)

Always preview an import first (`--dry-run`) so you know whether any incoming feed URL or filter rule name already exists in `smart_rss.db`:

```bash
# Step 1: Preview & detect conflicts without changing the database
./smart-rss config import --file ./my_feeds_and_rules.toml --dry-run
```

Inspect the JSON response:
- If `preview.has_conflicts` is `false`, import immediately:
  ```bash
  ./smart-rss config import --file ./my_feeds_and_rules.toml
  ```
- If `preview.has_conflicts` is `true`, **do NOT silently overwrite existing items**. Show the user the conflicting feeds (`preview.conflicting_feeds`) and conflicting rules (`preview.conflicting_rules`) and ask whether they want to:
  1. **Override specific items (Yes/No)** — pass `--on-conflict skip-existing` plus `--override-feeds <url>...` and/or `--override-rules <name>...`,
  2. **Override All** — pass `--on-conflict override-all`,
  3. **Skip All Existing (import only new items)** — pass `--on-conflict skip-existing`, or
  4. **Cancel** — do not run the import (or pass `--on-conflict abort`).

```bash
# Override all conflicting feeds and filter rules
./smart-rss config import --file ./my_feeds_and_rules.toml --on-conflict override-all

# Keep all existing items and import only brand-new feeds and rules
./smart-rss config import --file ./my_feeds_and_rules.toml --on-conflict skip-existing

# Selectively override specific items while keeping other existing ones unchanged
./smart-rss config import --file ./my_feeds_and_rules.toml \
  --on-conflict skip-existing \
  --override-feeds "https://www.nature.com/neuro.rss" \
  --override-rules "Neuroscience Paper"
```

## 3. HTTP REST API Equivalent (When Desktop App is Open on `127.0.0.1:8765`)

- `GET /api/config/export` — Returns `{ toml, feeds_count, rules_count, default_export_path }`.
- `POST /api/config/export-file` — Writes the exported TOML to `{ "path": "/path/to/config.toml" }`.
- `POST /api/config/import/preview` — Previews `{ "toml": "..." }` or `{ "path": "..." }` and lists new vs. conflicting feeds and rules.
- `POST /api/config/import/apply` — Applies `{ "toml": "...", "override_all": false, "override_feed_urls": [...], "override_rule_names": [...] }`.
