#!/usr/bin/env bash
# SmartRSSReader — Single-command version manager
#
# Usage:
#   ./scripts/set-version.sh                  # Show & verify current version across all files
#   ./scripts/set-version.sh --check          # Verify Cargo.toml, tauri.conf.json & package.json match
#   ./scripts/set-version.sh 0.2.0-alpha.1    # Set explicit SemVer version everywhere
#   ./scripts/set-version.sh alpha            # Bump prerelease alpha (e.g. 0.1.0 -> 0.1.1-alpha.1, 0.2.0-alpha.1 -> 0.2.0-alpha.2)
#   ./scripts/set-version.sh minor-alpha      # Start next minor alpha (e.g. 0.1.0 -> 0.2.0-alpha.1)
#   ./scripts/set-version.sh patch            # Bump patch (or promote 0.2.0-alpha.N -> 0.2.0)
#   ./scripts/set-version.sh minor            # Bump minor (e.g. 0.1.0 -> 0.2.0)
#   ./scripts/set-version.sh major            # Bump major (e.g. 0.2.0 -> 1.0.0)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

exec python3 - "$REPO_ROOT" "${1:-}" <<'PY'
import json
import pathlib
import re
import sys

root = pathlib.Path(sys.argv[1])
arg = sys.argv[2].strip()

cargo_toml = root / "src-tauri" / "Cargo.toml"
cargo_lock = root / "src-tauri" / "Cargo.lock"
tauri_conf = root / "src-tauri" / "tauri.conf.json"
package_json = root / "package.json"
index_html = root / "src" / "index.html"

SEMVER_RE = re.compile(
    r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)"
    r"(?:-((?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?"
    r"(?:\+([0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*))?$"
)


def read_cargo_version(text: str) -> str:
    m = re.search(
        r"(?ms)^\[package\]\s*\n(?:[^\[]*?\n)?version\s*=\s*\"([^\"]+)\"",
        text,
    )
    if not m:
        raise SystemExit("Error: Could not locate [package] version in src-tauri/Cargo.toml")
    return m.group(1)


def to_tauri_msi_version(semver: str) -> str:
    """Convert SemVer to a WiX MSI-compatible Tauri bundle version.

    Tauri's Windows MSI (WiX) bundler requires any pre-release identifier in
    `src-tauri/tauri.conf.json` to be numeric-only and <= 65535 (e.g.
    `0.1.0-alpha.4` -> `0.1.0-4`). Stable versions (`0.2.0`) stay unchanged.
    """
    m = SEMVER_RE.match(semver)
    if not m:
        return semver
    maj, min_, pat, pre = m.group(1), m.group(2), m.group(3), m.group(4)
    if not pre:
        return f"{maj}.{min_}.{pat}"
    num_match = re.search(r"(\d+)$", pre)
    if num_match:
        num = int(num_match.group(1))
        if 0 <= num <= 65535:
            return f"{maj}.{min_}.{pat}-{num}"
    return f"{maj}.{min_}.{pat}"


cargo_text = cargo_toml.read_text(encoding="utf-8")
current_cargo_ver = read_cargo_version(cargo_text)
tauri_data = json.loads(tauri_conf.read_text(encoding="utf-8"))
pkg_data = json.loads(package_json.read_text(encoding="utf-8"))
current_tauri_ver = tauri_data.get("version", "")
current_pkg_ver = pkg_data.get("version", "")
expected_tauri_ver = to_tauri_msi_version(current_cargo_ver)

if arg in ("", "--check", "check", "--current", "current"):
    in_sync = (current_cargo_ver == current_pkg_ver and current_tauri_ver == expected_tauri_ver)
    print(f"Current SmartRSSReader version: {current_cargo_ver}")
    print(f"  src-tauri/Cargo.toml      : {current_cargo_ver}")
    print(f"  src-tauri/tauri.conf.json : {current_tauri_ver} (expected {expected_tauri_ver} for WiX MSI)")
    print(f"  package.json              : {current_pkg_ver}")
    if not in_sync:
        print("Status: OUT OF SYNC", file=sys.stderr)
        sys.exit(1)
    print("Status: IN SYNC")
    sys.exit(0)

if arg in ("-h", "--help", "help"):
    print("Usage: ./scripts/set-version.sh [<semver> | alpha | minor-alpha | patch | minor | major | --check]")
    sys.exit(0)


def compute_next_version(cur: str, spec: str) -> str:
    m = SEMVER_RE.match(cur)
    if not m:
        raise SystemExit(f"Current version '{cur}' is not valid SemVer.")
    maj, min_, pat = int(m.group(1)), int(m.group(2)), int(m.group(3))
    pre = m.group(4)

    if spec == "alpha":
        if pre:
            am = re.match(r"^(.*?)(\d+)$", pre)
            if am:
                prefix, num = am.group(1), int(am.group(2))
                return f"{maj}.{min_}.{pat}-{prefix}{num + 1}"
            return f"{maj}.{min_}.{pat}-{pre}.1"
        return f"{maj}.{min_}.{pat + 1}-alpha.1"
    if spec == "minor-alpha":
        return f"{maj}.{min_ + 1}.0-alpha.1"
    if spec == "patch":
        if pre:
            return f"{maj}.{min_}.{pat}"
        return f"{maj}.{min_}.{pat + 1}"
    if spec == "minor":
        return f"{maj}.{min_ + 1}.0"
    if spec == "major":
        return f"{maj + 1}.0.0"

    cleaned = spec[1:] if spec[:1].lower() == "v" else spec
    if not SEMVER_RE.match(cleaned):
        raise SystemExit(
            f"Error: '{spec}' is not a valid SemVer string (expected e.g. '0.2.0-alpha.1' or '0.2.0')."
        )
    return cleaned


new_ver = compute_next_version(current_cargo_ver, arg)
new_tauri_ver = to_tauri_msi_version(new_ver)

# 1. Update src-tauri/Cargo.toml ([package] block only)
new_cargo_text, count = re.subn(
    r"(?ms)(^\[package\]\s*\n(?:[^\[]*?\n)?version\s*=\s*\")([^\"]+)(\")",
    rf"\g<1>{new_ver}\g<3>",
    cargo_text,
    count=1,
)
if count != 1:
    raise SystemExit("Error: Failed to update [package] version in src-tauri/Cargo.toml")
cargo_toml.write_text(new_cargo_text, encoding="utf-8")

# 2. Update src-tauri/tauri.conf.json (preserving 2-space formatting; WiX MSI numeric prerelease)
tauri_raw = tauri_conf.read_text(encoding="utf-8")
new_tauri_raw, t_count = re.subn(
    r'("version"\s*:\s*")([^"]+)(")',
    rf'\g<1>{new_tauri_ver}\g<3>',
    tauri_raw,
    count=1,
)
if t_count == 1:
    tauri_conf.write_text(new_tauri_raw, encoding="utf-8")
else:
    tauri_data["version"] = new_tauri_ver
    tauri_conf.write_text(json.dumps(tauri_data, indent=2) + "\n", encoding="utf-8")

# 3. Update package.json
pkg_raw = package_json.read_text(encoding="utf-8")
new_pkg_raw, p_count = re.subn(
    r'("version"\s*:\s*")([^"]+)(")',
    rf'\g<1>{new_ver}\g<3>',
    pkg_raw,
    count=1,
)
if p_count == 1:
    package_json.write_text(new_pkg_raw, encoding="utf-8")
else:
    pkg_data["version"] = new_ver
    package_json.write_text(json.dumps(pkg_data, indent=2) + "\n", encoding="utf-8")

# 4. Update fallback version badges in src/index.html
if index_html.exists():
    html_raw = index_html.read_text(encoding="utf-8")
    html_raw = re.sub(
        r'(id="app-version-badge">)v[^<]+(</span>)',
        rf"\g<1>v{new_ver} Tauri\g<2>",
        html_raw,
    )
    html_raw = re.sub(
        r'(id="settings-version-badge">)v[^<]+(</span>)',
        rf"\g<1>v{new_ver}\g<2>",
        html_raw,
    )
    index_html.write_text(html_raw, encoding="utf-8")

# 5. Update root package entry in src-tauri/Cargo.lock if present
if cargo_lock.exists():
    lock_raw = cargo_lock.read_text(encoding="utf-8")
    lock_raw = re.sub(
        r'(name\s*=\s*"smart-rss-reader"\s*\nversion\s*=\s*")([^"]+)(")',
        rf"\g<1>{new_ver}\g<3>",
        lock_raw,
        count=1,
    )
    cargo_lock.write_text(lock_raw, encoding="utf-8")

print(f"Updated SmartRSSReader version: {current_cargo_ver} -> {new_ver}")
print("  ✓ src-tauri/Cargo.toml")
print("  ✓ src-tauri/Cargo.lock")
print(f"  ✓ src-tauri/tauri.conf.json ({new_tauri_ver})")
print("  ✓ package.json")
print("  ✓ src/index.html")
print(
    f"\nNote: Existing users upgrading to v{new_ver} will automatically have their previous "
    f"DB, encrypted keys, and a TOML export backed up under ~/SmartRSSReader/backups/v{current_cargo_ver}/."
)
PY
