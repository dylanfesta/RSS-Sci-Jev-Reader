---
name: manage-jev-queries
description: >-
  Create, edit, re-weight (including positive boosts and negative penalties), or
  delete OpenRouter TypeSafe Jev relevance queries in SmartRSSReader. Use this
  skill whenever the user asks to add a relevance filter, change how papers are
  ranked, penalize unwanted topics or methods (such as fMRI, EEG, errata, or
  reviews), or adjust query weights.
---

# Manage Jev Relevance Queries & Weights (`manage-jev-queries`)

This skill allows you to manage **TypeSafe Jev** decision queries (`noul` Yes/No probabilities and `score` ordered rubrics) and their weights ($w_q \in \mathbb{R}$) in **SmartRSSReader** without using the GUI.

## How Jev Queries & Weights Work

1. **Query Types**:
   - **`noul` (Yes/No Probability)**: Evaluates a binary condition on each paper and returns a calibrated probability $s \in [0.0, 1.0]$. Requires `--instructions` and optional `--true-desc` and `--false-desc`.
   - **`score` (Ordered Rubric Scale)**: Places each paper on an ordered scale of $K \ge 2$ concrete rubric levels (`--levels "Level 1 (lowest)" ... "Level K (highest)"`) and normalizes the expected score onto $s \in [0.0, 1.0]$.
2. **Weights ($w_q \in \mathbb{R}$)**:
   - **Positive Weight ($w_q > 0$)**: Boosts the ranking of papers that match the query (e.g., `w = 2.0` for desired methodologies).
   - **Negative Weight ($w_q < 0$)**: Subtracts $w_q \cdot s$ from the paper's total score to penalize unwanted paper types or methodologies (e.g., `w = -1.0` for papers using fMRI or EEG).
   - Changing a query's weight immediately recalculates the ranking of all scored papers in `smart_rss.db` without making new OpenRouter API calls.

## 1. List Existing Jev Queries

Always inspect the current queries and weights first:

```bash
./smart-rss queries list
```

## 2. Create or Update a Yes/No (`noul`) Query

Use `queries set` to idempotently create or update a query by `--name`:

```bash
./smart-rss queries set \
  --name "Uses fMRI or EEG (Penalty)" \
  --type noul \
  --weight -1.0 \
  --instructions "Does this paper primarily rely on or report functional MRI (fMRI) or electroencephalography (EEG) experiments/data?" \
  --true-desc "The study uses, analyzes, or focuses on fMRI or EEG data." \
  --false-desc "The study does not involve fMRI or EEG."
```

## 3. Create or Update an Ordered Rubric (`score`) Query

Provide at least 2 `--levels` ordered from **lowest relevance (`[0]`)** to **highest relevance (`[K-1]`)**:

```bash
./smart-rss queries set \
  --name "Single-Cell & Spatial Omics Focus" \
  --type score \
  --weight 2.5 \
  --instructions "Rate how central single-cell sequencing or spatial transcriptomics is to this study." \
  --levels \
    "No mention of single-cell or spatial omics" \
    "Uses existing single-cell datasets only for secondary validation" \
    "Develops novel single-cell/spatial omics methods or primary biological discovery"
```

## 4. Adjust a Query's Weight Only

To quickly tune a positive or negative weight on an existing query by `--name` (or `--id`):

```bash
./smart-rss queries weight --name "Uses fMRI or EEG (Penalty)" --weight -1.5
```

## 5. Remove a Query

```bash
./smart-rss queries remove --name "Uses fMRI or EEG (Penalty)"
```

## 6. Validation Step

After creating, updating, or re-weighting a query:
1. Check the JSON response returned by `queries set` or `queries weight`—it includes `top_active_entries_preview` showing the updated top 5 ranked papers.
2. Or run `./smart-rss entries list --limit 5` to inspect the full ranked list.

> **Tip:** To export or import all Jev filter rules together with RSS feeds as a readable TOML file, see the `import-export-config` skill (`./smart-rss config export` / `./smart-rss config import`).
