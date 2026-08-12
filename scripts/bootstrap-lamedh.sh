#!/usr/bin/env sh
set -eu

repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
revision=$(tr -d '[:space:]' < "$repo_dir/LAMEDH_REVISION")
install_root=${GYMNAST_TOOLS_DIR:-"$repo_dir/.tools"}

if ! command -v cargo >/dev/null 2>&1; then
  echo "bootstrap-lamedh: cargo is required to build the pinned Lamedh runtime" >&2
  exit 127
fi

cargo install \
  --git https://github.com/pnathan/lamedh.git \
  --rev "$revision" \
  --locked \
  --root "$install_root" \
  --package lamedh-cli

"$install_root/bin/lamedh" -s '(list '\''lamedh '\''ready)'
