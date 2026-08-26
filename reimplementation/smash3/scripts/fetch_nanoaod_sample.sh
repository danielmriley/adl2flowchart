#!/usr/bin/env bash
# Fetch the NanoAOD validation sample into the local cache. A copy of this
# same file is already committed at
#   crates/adl-ingest/fixtures/nanoaod_ttbar.root
# (used by the always-on golden test); this script is for refreshing it or
# for the env-gated uproot oracle:
#
#   SMASH2_RUN_UPROOT_ORACLE=1 cargo test -p adl-cli --test ingest nanoaod
#
# The sample is the real CMS Open Data 2015 ttbar NanoAOD slice distributed
# by scikit-hep-testdata (CC0): tree `Events`, 200 events, 372 KB. It is
# verified by sha256 before being moved into place.
set -euo pipefail

URL="https://raw.githubusercontent.com/scikit-hep/scikit-hep-testdata/main/src/skhep_testdata/data/nanoAOD_2015_CMS_Open_Data_ttbar.root"
SHA256="c14a29b25b15b837226f396e920b5d9fb134f3558bef5b0a9db5d6d9606c5f3a"
CACHE_DIR="${SMASH2_CACHE_DIR:-$HOME/.cache/smash2}"
DEST="$CACHE_DIR/nanoaod_ttbar.root"

if [[ -f "$DEST" ]] && echo "$SHA256  $DEST" | sha256sum --check --quiet -; then
    echo "already cached: $DEST"
    exit 0
fi

mkdir -p "$CACHE_DIR"
TMP="$(mktemp "$CACHE_DIR/.fetch.XXXXXX")"
trap 'rm -f "$TMP"' EXIT

echo "fetching nanoAOD_2015_CMS_Open_Data_ttbar.root (~372 KB) ..."
if command -v wget >/dev/null; then
    wget -q -O "$TMP" "$URL"
else
    curl -sSL -o "$TMP" "$URL"
fi

echo "$SHA256  $TMP" | sha256sum --check --quiet - || {
    echo "ERROR: sha256 mismatch — refusing the download" >&2
    exit 1
}
mv "$TMP" "$DEST"
trap - EXIT
echo "cached: $DEST"
