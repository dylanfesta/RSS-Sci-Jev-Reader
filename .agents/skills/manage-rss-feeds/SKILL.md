---
name: manage-rss-feeds
description: >-
  Add, test, preview, update, or remove scientific journal RSS and Atom feeds in
  SmartRSSReader. Use this skill whenever the user asks to add a journal RSS feed,
  subscribe to a scientific journal or preprint category (e.g., Nature, Science,
  Cell, Neuron, arXiv, bioRxiv, APS), check configured feeds, or test/preview an
  RSS URL.
---

# Manage Scientific RSS Feeds (`manage-rss-feeds`)

This skill allows you to list, validate, add/update, or remove scientific journal RSS/Atom feeds in **SmartRSSReader** without using the GUI. Changes are persisted in `smart_rss.db` (SQLite WAL mode) and are immediately visible in the open Tauri desktop application.

## 1. Inspect Existing Feeds

Run the local `./smart-rss` CLI helper in this folder to view all currently configured feeds:

```bash
./smart-rss feeds list
```

## 2. Test / Preview a Feed URL Before Adding

Whenever the user provides a journal name or RSS URL, test it first to verify that it returns valid RSS/Atom/RDF XML and to inspect the detected journal title and sample entries:

```bash
./smart-rss feeds test --url "<RSS_URL>" --max-entries 5
```

### Common Scientific Journal RSS URL Patterns
- **Nature Journals**: `https://www.nature.com/<journal_code>.rss` (e.g., `nature.rss`, `neuro.rss`, `nmeth.rss`, `nbt.rss`, `srep.rss`)
- **arXiv Categories**: `https://rss.arxiv.org/rss/<category>` (e.g., `q-bio`, `q-bio.NC`, `cs.AI`, `cs.LG`, `physics.bio-ph`)
- **Cell Press Journals**: `https://www.cell.com/<journal_slug>/current.rss` (e.g., `cell`, `neuron`, `current-biology`)
- **APS Physics**: `https://feeds.aps.org/rss/recent/<journal>.xml` (e.g., `prl.xml`, `prx.xml`, `pre.xml`)
- **PNAS**: `https://www.pnas.org/action/showFeed?type=etoc&feed=rss&jc=pnas`

## 3. Add or Update (Upsert) a Feed (with Category & Tags)

Use `feeds add` to idempotently add or update a feed by its URL. Pass `--test` to validate the feed before saving (and omit `--title` if you want the title auto-detected from the RSS XML). You can also attach searchable `--tags` (space- or comma-separated):

```bash
./smart-rss feeds add \
  --url "https://www.nature.com/neuro.rss" \
  --title "Nature Neuroscience" \
  --category "Neuroscience" \
  --tags "neuroscience" "peer-reviewed" \
  --test
```

To disable a feed without deleting it, pass `--disabled`.

> **Tip:** To bulk-export or bulk-import your entire feed list (including names, categories, and tags) alongside filter rules as a human-readable TOML file, use the `import-export-config` skill (`./smart-rss config export` / `./smart-rss config import`).

## 4. Remove a Feed

Remove a feed by its `--url` or numeric `--id`:

```bash
./smart-rss feeds remove --url "https://www.nature.com/neuro.rss"
```

## 5. Validation Step

After adding or modifying feeds, run `./smart-rss feeds list` and confirm the feed appears in the JSON output with `"enabled": 1`.
