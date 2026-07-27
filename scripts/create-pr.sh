#!/usr/bin/env bash
set -euo pipefail

# AstraOS Pull Request Creator
#
# Usage:
#   ./scripts/create-pr.sh \
#     "feat: add state-bound check execution" \
#     "feat/state-bound-check-execution" \
#     "main"
#
# Defaults:
#   title: current branch commit subject
#   head: current branch
#   base: main

TITLE="${1:-}"
HEAD_BRANCH="${2:-$(git branch --show-current)}"
BASE_BRANCH="${3:-main}"

if [[ -z "$TITLE" ]]; then
  TITLE="$(git log -1 --pretty=%s)"
fi

if [[ -z "$HEAD_BRANCH" ]]; then
  echo "Error: unable to determine current branch."
  exit 1
fi

echo "Creating pull request..."
echo
echo "Title: $TITLE"
echo "Head:  $HEAD_BRANCH"
echo "Base:  $BASE_BRANCH"
echo

# Verify repository state
if [[ -n "$(git status --porcelain)" ]]; then
  echo "Error: working tree is not clean."
  git status --short
  exit 1
fi

# Verify branch exists remotely
if ! git ls-remote --heads origin "$HEAD_BRANCH" | grep -q "$HEAD_BRANCH"; then
  echo "Error: remote branch does not exist:"
  echo "$HEAD_BRANCH"
  exit 1
fi

TMP_BODY="$(mktemp)"

cleanup() {
  rm -f "$TMP_BODY"
}

trap cleanup EXIT

cat > "$TMP_BODY" <<EOF
## Summary

${TITLE}

## Changes

$(git show --stat --oneline HEAD)

## Architecture

This change follows the AstraOS layered architecture:

\`\`\`text
astra-context
      ↓
astra-actions
      ↓
astra-execution
      ↓
astra-cli
\`\`\`

## Validation

Passed:

- cargo fmt --all -- --check
- cargo check --workspace
- cargo test --workspace
- cargo clippy --workspace --all-targets -- -D warnings
- cargo build --release
- ./scripts/astraos-milestones.sh validate
- git diff --check

## Safety Guarantees

- no shell invocation
- exact structured argv execution
- state-bound planning
- stale-plan rejection
- source-state verification
- deterministic fingerprints

## Review Notes

Please verify:

- execution boundaries
- state fingerprint correctness
- JSON schema stability
- test coverage
EOF

gh pr create \
  --base "$BASE_BRANCH" \
  --head "$HEAD_BRANCH" \
  --title "$TITLE" \
  --body-file "$TMP_BODY"

echo
echo "Pull request created successfully."
