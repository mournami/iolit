#!/bin/sh
# Usage: push_mirror.sh <branch> <file>...
#
# Replaces <branch> of this repository with a single commit that holds exactly
# the given files, for clients that cannot reach github.com: they read the same
# signed files from raw.githubusercontent.com and cdn.jsdelivr.net, which serve
# that branch from other hosts. The files are verified by their signature on
# the client, so the mirrors need no trust. Needs GH_TOKEN and
# GITHUB_REPOSITORY, as set in GitHub Actions.
set -eu
branch=$1
shift
work=$(mktemp -d)
cp "$@" "$work/"
cd "$work"
git init -q -b "$branch"
git add -A
git -c user.name="github-actions[bot]" \
    -c user.email="41898282+github-actions[bot]@users.noreply.github.com" \
    commit -q -m "Mirror of the $branch files"
git push -q --force "https://x-access-token:${GH_TOKEN}@github.com/${GITHUB_REPOSITORY}.git" \
    "HEAD:refs/heads/$branch"
# jsDelivr keeps branch files for up to 12 hours; a purge makes it fetch them
# again. A failed purge only delays that mirror.
for file in "$@"; do
    name=$(basename "$file")
    curl -fsS -o /dev/null "https://purge.jsdelivr.net/gh/${GITHUB_REPOSITORY}@${branch}/${name}" \
        || echo "jsDelivr purge failed for $name"
done
echo "Mirrored $# file(s) to $branch"
