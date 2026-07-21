#!/usr/bin/env bash
# mapit installer — run: curl -sfSL https://raw.githubusercontent.com/patchyevolve/mapit/main/install.sh | sh

set -euo pipefail

APP="mapit"
REPO="patchyevolve/mapit"

die() {
  echo "error: $*" >&2
  exit 1
}

get_target() {
  local arch
  arch="$(uname -m)"
  local os
  os="$(uname -s)"
  case "$os" in
    Linux) os="unknown-linux-gnu" ;;
    Darwin) os="apple-darwin" ;;
    *) die "unsupported OS: $os" ;;
  esac
  case "$arch" in
    x86_64 | amd64) arch="x86_64" ;;
    aarch64 | arm64) arch="aarch64" ;;
    *) die "unsupported arch: $arch" ;;
  esac
  echo "${arch}-${os}"
}

get_latest_version() {
  curl -sfSL "https://api.github.com/repos/${REPO}/releases/latest" |
    grep '"tag_name"' |
    sed -E 's/.*"v?([^"]+)".*/\1/'
}

main() {
  local version="${1:-}"
  if [[ -z "$version" ]]; then
    echo "Fetching latest version..." >&2
    version="$(get_latest_version)"
  fi
  local target
  target="$(get_target)"
  local url="https://github.com/${REPO}/releases/download/v${version}/mapit-${target}.tar.gz"
  local dest="${DEST:-${HOME}/.local/bin}"

  echo "Downloading mapit v${version} for ${target}..." >&2
  mkdir -p "$dest"
  curl -sfSL "$url" | tar xzf - -C "$dest"

  if [[ ":$PATH:" != *":${dest}:"* ]]; then
    echo "Warning: ${dest} is not in your PATH. Add it or move the binary." >&2
  fi

  echo "Installed mapit v${version} to ${dest}/mapit" >&2
  mapit --version 2>/dev/null || true
}

main "$@"
