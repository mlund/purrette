#!/usr/bin/env bash
# Publishes a trunk build into one folder of the gh-pages branch and refreshes the
# root index (redirect to newest release) and versions.html (all folders).
#   scripts/publish-pages.sh dist v1.2.0
# Releases (v*) are immutable: re-publishing one fails, so a URL cited in a paper
# keeps serving the exact bytes it did. Other folders (e.g. dev) are overwritten.
set -euo pipefail

dist=$(cd "$1" && pwd)
folder=$2
[[ $folder =~ ^[A-Za-z0-9._-]+$ ]] || { echo "bad folder name: $folder" >&2; exit 1; }

site=$(mktemp -d)
trap 'git worktree remove --force "$site" 2>/dev/null || true' EXIT

if git fetch -q origin gh-pages 2>/dev/null; then
    git worktree add -q --detach "$site" FETCH_HEAD
else
    git worktree add -q --orphan -b gh-pages "$site"
fi

if [[ $folder == v* && -e $site/$folder ]]; then
    echo "$folder is already published; releases are immutable" >&2
    exit 1
fi
rm -rf "${site:?}/$folder"
cp -R "$dist" "$site/$folder"

cd "$site"
folders() { find . -mindepth 1 -maxdepth 1 -type d ! -name '.*' | sed 's|^\./||'; }
releases=$(folders | grep '^v' | sort -rV || true)
others=$(folders | grep -v '^v' | sort || true)
# Pre-releases (v1.0.0-rc1) are listed but never the redirect target.
newest=$(printf '%s\n%s\n' "$(grep -v -- - <<<"$releases")" "$releases$others" | grep -m1 . || true)

cat > index.html <<HTML
<!doctype html>
<meta charset="utf-8">
<meta http-equiv="refresh" content="0; url=$newest/">
<title>Redirecting…</title>
<p>Redirecting to <a href="$newest/">$newest</a>. <a href="versions.html">All versions</a>.</p>
HTML

{
    cat <<'HTML'
<!doctype html>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Versions</title>
<style>body{font-family:system-ui,sans-serif;max-width:40rem;margin:2rem auto;padding:0 1rem}</style>
<h1>Versions</h1>
<p>Every release stays online unchanged, so results can be reproduced with the version that made them.</p>
<ul>
HTML
    printf '%s\n%s\n' "$releases" "$others" | grep . | while read -r f; do
        echo "<li><a href=\"$f/\">$f</a></li>"
    done
    echo "</ul>"
} > versions.html

touch .nojekyll
git add -A
if git diff --cached --quiet; then
    echo "$folder unchanged; nothing to publish"
    exit 0
fi
git -c user.name="github-actions[bot]" \
    -c user.email="41898282+github-actions[bot]@users.noreply.github.com" \
    commit -q -m "Publish $folder"
git push -q origin HEAD:gh-pages
echo "Published $folder; root redirects to $newest"
