#!/usr/bin/env bash
# Create or update a PATH symlink for the claw binary.
#
# Usage:
#   ./link-claw.sh
#   ./link-claw.sh --release
#   ./link-claw.sh --bin-dir /custom/bin
#   ./link-claw.sh --name claw-dev
#   ./link-claw.sh --unlink

set -euo pipefail

print_usage() {
    cat <<'EOF'
Usage: ./link-claw.sh [options]

Options:
  --debug            Link rust/target/debug/claw (default)
  --release          Link rust/target/release/claw
  --bin-dir DIR      Destination directory for the symlink
  --name NAME        Symlink name (default: claw)
  --unlink           Remove the symlink instead of creating it
  -h, --help         Show this help text and exit

Examples:
  ./link-claw.sh
  ./link-claw.sh --release
  ./link-claw.sh --bin-dir ~/.local/bin --name claw-dev
EOF
}

error() {
    printf 'error: %s\n' "$1" 1>&2
}

info() {
    printf '%s\n' "$1"
}

PROFILE="debug"
BIN_DIR="${HOME:-${PWD}}/.local/bin"
LINK_NAME="claw"
UNLINK_ONLY="0"

while [ "$#" -gt 0 ]; do
    case "$1" in
        --debug)
            PROFILE="debug"
            ;;
        --release)
            PROFILE="release"
            ;;
        --bin-dir)
            shift
            if [ "$#" -eq 0 ]; then
                error "--bin-dir requires a value"
                exit 2
            fi
            BIN_DIR="$1"
            ;;
        --name)
            shift
            if [ "$#" -eq 0 ]; then
                error "--name requires a value"
                exit 2
            fi
            LINK_NAME="$1"
            ;;
        --unlink)
            UNLINK_ONLY="1"
            ;;
        -h|--help)
            print_usage
            exit 0
            ;;
        *)
            error "unknown argument: $1"
            print_usage
            exit 2
            ;;
    esac
    shift
done

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TARGET_BIN="${SCRIPT_DIR}/rust/target/${PROFILE}/claw"
LINK_PATH="${BIN_DIR}/${LINK_NAME}"

if [ "${UNLINK_ONLY}" = "1" ]; then
    if [ -L "${LINK_PATH}" ]; then
        rm -f "${LINK_PATH}"
        info "removed symlink: ${LINK_PATH}"
    elif [ -e "${LINK_PATH}" ]; then
        error "${LINK_PATH} exists but is not a symlink; refusing to remove it"
        exit 1
    else
        info "no symlink to remove: ${LINK_PATH}"
    fi
    exit 0
fi

if [ ! -x "${TARGET_BIN}" ]; then
    error "binary not found: ${TARGET_BIN}"
    error "build it first, for example: cd rust && cargo build --workspace"
    exit 1
fi

mkdir -p "${BIN_DIR}"

if [ -e "${LINK_PATH}" ] && [ ! -L "${LINK_PATH}" ]; then
    error "${LINK_PATH} already exists and is not a symlink"
    exit 1
fi

ln -sfn "${TARGET_BIN}" "${LINK_PATH}"

info "linked ${LINK_PATH} -> ${TARGET_BIN}"
case ":${PATH}:" in
    *:"${BIN_DIR}":*)
        info "${BIN_DIR} is already on PATH"
        ;;
    *)
        info "${BIN_DIR} is not on PATH yet"
        ;;
esac
