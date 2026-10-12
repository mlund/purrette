#!/usr/bin/env bash
# One-shot rename after creating a repo from the template:
#   scripts/rename.sh my_app "My App"
# Replaces crate/lib/bin names, the repo slug and the display title, drops the
# template-only sections from README.md and AGENTS.md, then deletes itself.
set -euo pipefail

old_crate=eframe_wasm_template
old_slug=eframe-wasm-template
old_title="eframe wasm template"

crate=${1:-}
title=${2:-}
if [[ ! $crate =~ ^[a-z][a-z0-9_]*$ || -z $title ]]; then
    echo "usage: $0 <snake_case_crate_name> \"<App Title>\"" >&2
    exit 1
fi
slug=${crate//_/-}

cd "$(git rev-parse --show-toplevel)"

# Repo slug follows the git remote when there is one, so URLs match GitHub.
if remote=$(git remote get-url origin 2>/dev/null); then
    slug=$(basename "$remote" .git)
fi

# perl rather than sed -i: same behaviour on macOS and Linux.
git ls-files -z -- ':!scripts/rename.sh' ':!LICENSE-*' |
    xargs -0 perl -pi -e "s/\Q$old_crate\E/$crate/g; s/\Q$old_slug\E/$slug/g; s/\Q$old_title\E/$title/g"

# Template-only text sits between these markers.
perl -0pi -e 's/<!-- template:start -->.*?<!-- template:end -->\n*//s' README.md AGENTS.md

git rm -q scripts/rename.sh
cargo check -q
echo "Renamed to $crate (\"$title\", repo $slug). Review with: git diff"
