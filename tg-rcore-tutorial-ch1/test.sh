#!/bin/bash
set -euo pipefail

cargo build >/dev/null

LOG_FILE=/tmp/ch1-tangram-test.log
rm -f "$LOG_FILE"

timeout 3s qemu-system-riscv64 \
    -machine virt \
    -bios none \
    -display none \
    -device virtio-gpu-device \
    -serial "file:$LOG_FILE" \
    -monitor none \
    -kernel target/riscv64gc-unknown-none-elf/debug/tg-rcore-tutorial-ch1 \
    >/dev/null 2>&1 || true

OUTPUT=$(cat "$LOG_FILE" 2>/dev/null || true)

if echo "$OUTPUT" | grep -q "Tangram OS rendered."; then
    echo "Test PASSED: framebuffer rendering path completed"
    exit 0
fi

echo "Test FAILED: tangram render log not found"
echo "Actual output:"
echo "$OUTPUT"
exit 1
