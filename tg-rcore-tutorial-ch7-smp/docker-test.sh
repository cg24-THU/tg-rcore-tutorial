#!/bin/bash

set -euo pipefail

CONTAINER="${RCORE_CONTAINER:-rcore-container}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PROJECT_NAME="$(basename "$SCRIPT_DIR")"
CONTAINER_ROOT="/tmp/tg-rcore-tutorial"
CONTAINER_PROJECT="$CONTAINER_ROOT/$PROJECT_NAME"
CONTAINER_USER="$CONTAINER_ROOT/tg-rcore-tutorial-user"

sync_sources() {
    docker exec -it "$CONTAINER" bash -lc "mkdir -p '$CONTAINER_ROOT'"
    docker cp "$ROOT_DIR/tg-rcore-tutorial-sbi-smp" "$CONTAINER:$CONTAINER_ROOT/"
    docker cp "$SCRIPT_DIR" "$CONTAINER:$CONTAINER_ROOT/"
}

observe() {
    sync_sources
    docker exec -it "$CONTAINER" bash -lc \
        "cd '$CONTAINER_PROJECT' && timeout 12s env TG_USER_DIR='$CONTAINER_USER' cargo run"
}

test_mode() {
    sync_sources
    docker exec -it "$CONTAINER" bash -lc \
        "cd '$CONTAINER_PROJECT' && export TG_USER_DIR='$CONTAINER_USER' && bash test.sh base"
}

case "${1:-all}" in
    observe)
        observe
        ;;
    base|all)
        observe
        test_mode
        ;;
    *)
        echo "usage: bash docker-test.sh [observe|base|all]"
        exit 1
        ;;
esac
