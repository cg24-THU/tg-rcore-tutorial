#!/bin/bash
set -euo pipefail

LOG_FILE=/tmp/ch2-moving-tangram.log
rm -f "$LOG_FILE"

cargo build >/dev/null

timeout 15s qemu-system-riscv64 \
    -machine virt \
    -bios none \
    -display none \
    -device virtio-gpu-device \
    -serial "file:$LOG_FILE" \
    -monitor none \
    -kernel target/riscv64gc-unknown-none-elf/debug/tg-rcore-tutorial-ch2 \
    >/dev/null 2>&1

required_patterns=(
    "VirtIO-GPU framebuffer ready:"
    "request draw piece: 0"
    "request draw piece: 6"
    "app0 exit with code 0"
    "app6 exit with code 6"
)

for pattern in "${required_patterns[@]}"; do
    if ! grep -q "$pattern" "$LOG_FILE"; then
        echo "Test FAILED: missing <$pattern>"
        echo "Actual output:"
        cat "$LOG_FILE"
        exit 1
    fi
done

if grep -q "was killed because of" "$LOG_FILE"; then
    echo "Test FAILED: unexpected trap in user apps"
    cat "$LOG_FILE"
    exit 1
fi

echo "Test PASSED: moving tangram batch rendering completed"
