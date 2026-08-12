#!/usr/bin/env sh
set -eu

repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
version=$(tr -d '[:space:]' < "$repo_dir/LAMEDH_VERSION")
install_root=${GYMNAST_TOOLS_DIR:-"$repo_dir/.tools"}

case "$(uname -m)" in
  x86_64|amd64)
    asset=lamedh-linux-x86_64
    expected_sha256=4c2ef9375a1128608b79bb47ba0e47cf64bb761bf78292a565178374eab2cd2b
    ;;
  aarch64|arm64)
    asset=lamedh-linux-aarch64
    expected_sha256=7d567a826a273d018bcdd5a1b1bf87b8e6b05a77fc36707511eae01189c51b21
    ;;
  *)
    echo "bootstrap-lamedh: unsupported architecture: $(uname -m)" >&2
    exit 1
    ;;
esac

if ! command -v curl >/dev/null 2>&1; then
  echo "bootstrap-lamedh: curl is required" >&2
  exit 127
fi

mkdir -p "$install_root/bin"
download="$install_root/bin/.lamedh-download"
trap 'rm -f "$download"' EXIT HUP INT TERM

curl --fail --location --retry 3 \
  --output "$download" \
  "https://github.com/pnathan/lamedh/releases/download/$version/$asset"

if command -v sha256sum >/dev/null 2>&1; then
  actual_sha256=$(sha256sum "$download" | awk '{print $1}')
elif command -v shasum >/dev/null 2>&1; then
  actual_sha256=$(shasum -a 256 "$download" | awk '{print $1}')
else
  echo "bootstrap-lamedh: sha256sum or shasum is required" >&2
  exit 127
fi

if [ "$actual_sha256" != "$expected_sha256" ]; then
  echo "bootstrap-lamedh: checksum mismatch for $asset" >&2
  exit 1
fi

chmod 0755 "$download"
mv "$download" "$install_root/bin/lamedh"
"$install_root/bin/lamedh" -s '(list '\''lamedh '\''ready)'
