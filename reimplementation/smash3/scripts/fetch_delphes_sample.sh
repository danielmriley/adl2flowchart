#!/usr/bin/env bash
# Fetch the pinned Delphes validation sample (SPEC_EVENT_PIPELINE §7) into
# the local cache for the env-gated e2e tests:
#
#   SMASH2_RUN_DELPHES_E2E=1 cargo test -p adl-cli --test ingest
#
# The sample is CutLang's own tutorial file (binder/Tutorial.ipynb),
# 71 452 474 bytes, ROOT 6.18.04, tree `Delphes`, 20 000 events. It is
# verified by sha256 before being moved into place; a corrupt or changed
# download is refused.
set -euo pipefail

URL="https://www.dropbox.com/s/zza28peyjy8qgg6/T2tt_700_50.root?dl=1"
SHA256="04fae8b1d94809f799741af8351f9448b84370122b780ccf03df3b74531b89fc"
CACHE_DIR="${SMASH2_CACHE_DIR:-$HOME/.cache/smash2}"
DEST="$CACHE_DIR/delphes_T2tt_700_50.root"

if [[ -f "$DEST" ]] && echo "$SHA256  $DEST" | sha256sum --check --quiet -; then
    echo "already cached: $DEST"
    exit 0
fi

mkdir -p "$CACHE_DIR"
TMP="$(mktemp "$CACHE_DIR/.fetch.XXXXXX")"
trap 'rm -f "$TMP"' EXIT

echo "fetching T2tt_700_50.root (~71 MB) ..."
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
