#!/bin/zsh
# Downloads solved spots from the latest (or a given) GitHub Actions "solve" run into data/solutions/.
# Usage: scripts/fetch-solutions.sh [run-id]
cd "${0:A:h}/.." || exit 1
RUN=${1:-$(gh run list --workflow solve.yml --limit 1 --json databaseId --jq '.[0].databaseId')}
[ -n "$RUN" ] || { echo "no solve runs found"; exit 1; }
TMP=$(mktemp -d)
gh run download "$RUN" -D "$TMP" || exit 1
mkdir -p data/solutions
n=0
for f in "$TMP"/*/*.json; do
  id=${f:t:r}
  if [ ! -f "data/solutions/$id.json" ] && [ -f "${f:r}.bin" ]; then
    mv "${f:r}.bin" "data/solutions/$id.bin" && mv "$f" "data/solutions/$id.json" && n=$((n+1))
  fi
done
rm -rf "$TMP"
echo "added $n solved spots from run $RUN"
./target/release/solve status | tail -1
