#!/usr/bin/env bash
set -o errexit
set -o pipefail

node .changeset/pre-changelog.mjs

if [[ "$1" != "--action" ]]; then
  export GITHUB_TOKEN=$(gh auth token)
fi


# Sets js package version
pnpm exec changeset version
# Note: this will not immediately affect crates/pluot/pkg/package.json, as this version matches the version in crates/pluot/Cargo.toml

node .changeset/post-changelog.mjs

# The above post-changelog script modifies the version of the root package.json file,
# so we obtain the version following that script.
NEXT_VERSION=$(cat ./package.json | jq -r .version)

# Set rust crate versions using cargo-edit
cargo set-version --workspace $NEXT_VERSION
cargo set-version --manifest-path examples/pluot_cli/Cargo.toml $NEXT_VERSION

# Set R package version by modifying DESCRIPTION
perl -pi -e "s/^Version: .*/Version: ${NEXT_VERSION}/" bindings-r/DESCRIPTION

# Note: python package version is already dynamic.
