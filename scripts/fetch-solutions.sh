#!/bin/zsh
# Downloads solved spots from a GitHub Actions "solve" run into data/solutions/,
# one artifact at a time with retries; safe to rerun (already-fetched spots are skipped).
# Usage: scripts/fetch-solutions.sh [run-id]
cd "${0:A:h}/.." || exit 1
REPO=$(gh repo view --json nameWithOwner --jq .nameWithOwner)
RUN=${1:-$(gh run list --workflow solve.yml --limit 1 --json databaseId --jq '.[0].databaseId')}
[ -n "$RUN" ] || { echo "no solve runs found"; exit 1; }
mkdir -p data/solutions data/.fetched
names=("${(@f)$(gh api "repos/$REPO/actions/runs/$RUN/artifacts" --paginate --jq '.artifacts[].name')}")
total=${#names}; i=0; added=0; failed=()
for name in $names; do
  i=$((i+1))
  if [ -f "data/.fetched/$RUN-$name" ]; then continue; fi
  ok=0
  for try in 1 2 3; do
    TMP=$(mktemp -d)
    if gh run download "$RUN" -R "$REPO" -n "$name" -D "$TMP" >/dev/null 2>&1; then
      for f in "$TMP"/*.json(N); do
        id=${f:t:r}
        if [ ! -f "data/solutions/$id.json" ] && [ -f "${f:r}.bin" ]; then
          mv "${f:r}.bin" "data/solutions/$id.bin" && mv "$f" "data/solutions/$id.json" && added=$((added+1))
        fi
      done
      rm -rf "$TMP"; touch "data/.fetched/$RUN-$name"; ok=1; break
    fi
    rm -rf "$TMP"; sleep $((try * 10))
  done
  [ $ok = 1 ] || failed+=("$name")
  echo "[$i/$total] $name $([ $ok = 1 ] && echo ok || echo FAILED) (spots added so far: $added)"
done
echo "added $added solved spots from run $RUN"
[ ${#failed} -gt 0 ] && echo "still missing: ${failed[*]} (rerun this script to retry)"
./target/release/solve status | tail -1
