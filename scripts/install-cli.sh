#!/usr/bin/env sh

set -eu

BIN_NAME="nitro-da-cli"
VERSION="${VERSION:-v0.1.0-rc2}"
INSTALL_DIR="$HOME/.cargo/bin"
BASE_URL="https://nitro-da-cli.termina.technology"
BINARY_URL="${BASE_URL}/${BIN_NAME}/${VERSION}"

detect_platform() {
  uname_out="$(uname -s)"
  arch_out="$(uname -m)"

  case "${uname_out}" in
  Linux*) platform="unknown-linux-gnu" ;;
  Darwin*) platform="apple-darwin" ;;
  *)
    echo "Unsupported platform: ${uname_out}"
    exit 1
    ;;
  esac

  case "${arch_out}" in
  x86_64*) arch="x86_64" ;;
  arm64* | aarch64*) arch="aarch64" ;;
  *)
    echo "Unsupported architecture: ${arch_out}"
    exit 1
    ;;
  esac

  TARGET="${arch}-${platform}"
  DOWNLOAD_URL="${BINARY_URL}/${BIN_NAME}-${VERSION}-${TARGET}.tar.gz"
}

download_binary() {
  echo "Downloading ${BIN_NAME} version ${VERSION}..."
  TMP_DIR=$(mktemp -d)
  curl -sSfL "${DOWNLOAD_URL}" -o "${TMP_DIR}/${BIN_NAME}.tar.gz"
}

install_binary() {
  echo "Installing ${BIN_NAME} to ${INSTALL_DIR}..."
  mkdir -p "${INSTALL_DIR}"
  tar -xzvf "${TMP_DIR}/${BIN_NAME}.tar.gz" -C "${TMP_DIR}"
  chmod +x "${TMP_DIR}/${BIN_NAME}"
  mv "${TMP_DIR}/${BIN_NAME}" "${INSTALL_DIR}/${BIN_NAME}"
}

cleanup() {
  rm -rf "${TMP_DIR}"
}

check_path() {
  if echo "$PATH" | grep -q "$HOME/.cargo/bin"; then
    INSTALL_DIR="$HOME/.cargo/bin"
  elif echo "$PATH" | grep -q "$HOME/.local/bin"; then
    INSTALL_DIR="$HOME/.local/bin"
  else
    INSTALL_DIR="/usr/local/bin"
    echo "Neither ~/.cargo/bin nor ~/.local/bin found in PATH. Falling back to $INSTALL_DIR"
    echo "You might need sudo privileges to install here."
  fi
}

check_path
detect_platform
download_binary
install_binary
cleanup

echo "${BIN_NAME} version ${VERSION} installed successfully!"
echo "You should add ${INSTALL_DIR} to your PATH if it's not already there by running the following command:"
echo "export PATH=\"\${PATH}:${INSTALL_DIR}\""
echo "or set PATH \"\${PATH}\" \"${INSTALL_DIR}\" if using fish"
