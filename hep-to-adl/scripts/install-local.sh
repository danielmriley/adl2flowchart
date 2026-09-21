#!/usr/bin/env bash
# Copy this plugin to a real local Cursor plugin directory.
# Default destination: ~/.cursor/plugins/local/hep-to-adl
# Never installs as a symlink to the checkout.
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: install-local.sh [--dry-run] [--dest DIR]

Copy hep-to-adl into a real directory (cp -R, not ln -s).

  --dry-run   Print the copy plan and exit 0 without writing.
  --dest DIR  Override the destination.
              Default: $HEPTADL_INSTALL_DIR or ~/.cursor/plugins/local/hep-to-adl
EOF
}

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
SRC=$(cd "$SCRIPT_DIR/.." && pwd)
DEST=${HEPTADL_INSTALL_DIR:-"$HOME/.cursor/plugins/local/hep-to-adl"}
DRY_RUN=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --dest)
      DEST=${2:?--dest requires a directory}
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ ! -f "$SRC/.cursor-plugin/plugin.json" ]]; then
  echo "missing $SRC/.cursor-plugin/plugin.json" >&2
  exit 1
fi

if [[ -L "$DEST" ]]; then
  echo "refusing to install over symlink $DEST (copy target must be a real directory)" >&2
  exit 1
fi

echo "source: $SRC"
echo "dest:   $DEST"
echo "mode:   $([[ $DRY_RUN -eq 1 ]] && echo dry-run || echo copy)"

AFTER_HINT=$(cat <<'EOF'
Then in Cursor: Reload Window, enable the local plugin if needed, and invoke
  /hep-to-adl convert this CMSSW analyzer…
the same way /poteto-mode is invoked.
EOF
)

if [[ $DRY_RUN -eq 1 ]]; then
  echo "would mkdir -p $(dirname "$DEST")"
  echo "would rm -rf $DEST"
  echo "would cp -R $SRC $DEST"
  echo "would chmod 755 $DEST/scripts/*.sh $DEST/scripts/*.py"
  echo "$AFTER_HINT"
  exit 0
fi

mkdir -p "$(dirname "$DEST")"
rm -rf "$DEST"
cp -R "$SRC" "$DEST"
chmod 755 "$DEST/scripts/"*.sh "$DEST/scripts/"*.py
echo "installed $DEST"
echo "$AFTER_HINT"
