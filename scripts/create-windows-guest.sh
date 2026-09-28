#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/create-windows-guest.sh [/path/to/windows.iso] [disk.qcow2]

Creates a QEMU VM that is suitable for validating the Windows release workflow:
- Windows 11/Server installer media
- MSVC + Windows SDK + LLVM 19
- backend build + test workflow reproducibility

Examples:
  WINDOWS_ISO=/Volumes/C4TB/iso/Win11.iso scripts/create-windows-guest.sh
  scripts/create-windows-guest.sh ~/Downloads/Win11_23H2_English_x64.iso
  scripts/create-windows-guest.sh ~/Downloads/Win11_23H2_English_x64.iso ~/tmp/skill-win.qcow2

Environment overrides:
  WINDOWS_ISO=/Volumes/C4TB/iso/Win11.iso RAM_GB=8 CPU_COUNT=4 ./scripts/create-windows-guest.sh

This script boots the guest from the ISO and leaves the VM running in the foreground.
To stop it, press Ctrl+C.
EOF
}

if [[ $# -gt 2 ]]; then
  usage
  exit 1
fi

if [[ $# -ge 1 ]]; then
  ISO_PATH=${1}
else
  ISO_PATH=${WINDOWS_ISO:-/Volumes/C4TB/iso}
fi

if [[ -d "$ISO_PATH" ]]; then
  # Prefer a Windows installer ISO in the directory if present.
  candidates=(
    "$ISO_PATH"/*.iso
    "$ISO_PATH"/*.ISO
    "$ISO_PATH"/*.img
    "$ISO_PATH"/*.IMG
  )
  ISO_PATH=""
  for candidate in "${candidates[@]}"; do
    if [[ -f "$candidate" ]]; then
      ISO_PATH="$candidate"
      break
    fi
  done
fi

if [[ -z "$ISO_PATH" || ! -f "$ISO_PATH" ]]; then
  if [[ $# -eq 0 && -n "${WINDOWS_ISO:-}" ]]; then
    echo "ERROR: WINDOWS_ISO is set but the file does not exist: ${WINDOWS_ISO}" >&2
  else
    echo "ERROR: Windows ISO not found. Pass it as an argument or set WINDOWS_ISO." >&2
  fi
  usage >&2
  exit 1
fi

DISK_PATH=${2:-"$PWD/windows-ci-validation.qcow2"}
RAM_GB=${RAM_GB:-8}
CPU_COUNT=${CPU_COUNT:-4}
CPU_MODEL=${CPU_MODEL:-max}
VIDEO_MODEL=${VIDEO_MODEL:-std}
MACHINE=${MACHINE:-q35}
NIC_MODEL=${NIC_MODEL:-e1000e}
NETWORK_ENABLED=${NETWORK_ENABLED:-1}
NO_REBOOT=${NO_REBOOT:-0}
SSH_HOST_PORT=${SSH_HOST_PORT:-2222}
DISK_BUS=${DISK_BUS:-ide}
ISO_BUS=${ISO_BUS:-ide}
BOOT_ORDER=${BOOT_ORDER:-d}
TPM_STATE_DIR=${TPM_STATE_DIR:-"${DISK_PATH}.tpm"}
UEFI_VARS_PATH=${UEFI_VARS_PATH:-"${DISK_PATH}.uefi-vars.fd"}
QEMU_SHARE_DIR=${QEMU_SHARE_DIR:-"$(brew --prefix qemu 2>/dev/null || echo /opt/homebrew/opt/qemu)/share/qemu"}
UEFI_CODE_PATH=${UEFI_CODE_PATH:-"$QEMU_SHARE_DIR/edk2-x86_64-secure-code.fd"}
UEFI_VARS_TEMPLATE=${UEFI_VARS_TEMPLATE:-"$QEMU_SHARE_DIR/edk2-i386-vars.fd"}

if [[ ! -f "$ISO_PATH" ]]; then
  echo "ERROR: Windows ISO not found: $ISO_PATH" >&2
  exit 1
fi

if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then
  echo "ERROR: qemu-system-x86_64 is not installed." >&2
  exit 1
fi

mkdir -p "$(dirname "$DISK_PATH")"
if [[ ! -f "$DISK_PATH" ]]; then
  qemu-img create -f qcow2 "$DISK_PATH" 120G
fi

if ! command -v swtpm >/dev/null 2>&1; then
  echo "ERROR: swtpm is required for TPM 2.0 support." >&2
  exit 1
fi
if [[ ! -f "$UEFI_CODE_PATH" || ! -f "$UEFI_VARS_TEMPLATE" ]]; then
  echo "ERROR: Secure Boot firmware was not found under $QEMU_SHARE_DIR." >&2
  exit 1
fi

mkdir -p "$TPM_STATE_DIR"
if [[ ! -f "$UEFI_VARS_PATH" ]]; then
  cp "$UEFI_VARS_TEMPLATE" "$UEFI_VARS_PATH"
fi

TPM_SOCKET="$TPM_STATE_DIR/swtpm.sock"
rm -f "$TPM_SOCKET"
swtpm socket \
  --tpm2 \
  --tpmstate "dir=$TPM_STATE_DIR" \
  --ctrl "type=unixio,path=$TPM_SOCKET" &
SWTPM_PID=$!
for attempt in {1..50}; do
  [[ -S "$TPM_SOCKET" ]] && break
  if ! kill -0 "$SWTPM_PID" 2>/dev/null; then
    echo "ERROR: swtpm exited before creating its control socket." >&2
    exit 1
  fi
  sleep 0.1
done
if [[ ! -S "$TPM_SOCKET" ]]; then
  echo "ERROR: timed out waiting for swtpm control socket." >&2
  exit 1
fi
cleanup() {
  kill "$SWTPM_PID" 2>/dev/null || true
  rm -f "$TPM_SOCKET"
}
trap cleanup EXIT INT TERM

ACCEL=${QEMU_ACCEL:-tcg}
if [[ -z "${QEMU_ACCEL:-}" && "$(uname -s)" == "Darwin" && \
  "$(qemu-system-x86_64 -accel help 2>/dev/null)" == *"hvf"* ]]; then
  ACCEL=hvf
fi

QEMU_ARGS=(
  qemu-system-x86_64 \
  -machine "$MACHINE",accel="$ACCEL",smm=on \
  -cpu "$CPU_MODEL" \
  -m "${RAM_GB}G" \
  -smp "$CPU_COUNT" \
  -drive if=pflash,format=raw,readonly=on,file="$UEFI_CODE_PATH" \
  -drive if=pflash,format=raw,file="$UEFI_VARS_PATH" \
  -drive file="$DISK_PATH",format=qcow2,if="$DISK_BUS" \
  -cdrom "$ISO_PATH" \
  -chardev socket,id=chrtpm,path="$TPM_SOCKET" \
  -tpmdev emulator,id=tpm0,chardev=chrtpm \
  -device tpm-crb,tpmdev=tpm0 \
  -boot order="$BOOT_ORDER",menu=on,strict=on \
  -display default \
  -vga "$VIDEO_MODEL" \
  -rtc base=localtime
)
if [[ "$NETWORK_ENABLED" == "1" ]]; then
  QEMU_ARGS+=(
    -netdev user,id=net0,hostfwd=tcp:127.0.0.1:3389-:3389,hostfwd=tcp:127.0.0.1:${SSH_HOST_PORT}-:22
    -device "$NIC_MODEL",netdev=net0
  )
fi
QEMU_ARGS+=(
  -usb \
  -device usb-tablet \
  -serial stdio
)
if [[ "$NO_REBOOT" == "1" ]]; then
  QEMU_ARGS+=( -no-reboot )
fi

"${QEMU_ARGS[@]}"
