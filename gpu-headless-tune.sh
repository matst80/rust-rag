#!/usr/bin/env bash
#
# Headless GPU Tuning Script for NVIDIA GTX 1080 Ti
# Sets power limit, memory clock offset (overclock), and core clock offset (underclock).
#
set -euo pipefail

POWER_LIMIT_W=160
CORE_OFFSET_MHZ=-200
MEM_OFFSET_MHZ=800
GPU_INDEX=0
# GTX 1080 Ti PCI address on this host is 26:00.0 (hex 0x26 = decimal 38)
BUS_ID="PCI:38:0:0"
DISPLAY_NUM=":99"
CONFIG_FILE="/tmp/xorg-headless-oc.conf"

echo "=== [1/5] Checking dependencies ==="
if ! command -v nvidia-smi &>/dev/null; then
    echo "Error: nvidia-smi not found." >&2
    exit 1
fi

if ! command -v nvidia-settings &>/dev/null; then
    echo "Error: nvidia-settings is required to adjust clock offsets on Pascal architecture." >&2
    echo "Please install it with: sudo apt install -y nvidia-settings" >&2
    exit 1
fi

if ! command -v Xorg &>/dev/null; then
    echo "Error: Xorg not found. Please install xserver-xorg." >&2
    exit 1
fi

echo "=== [2/5] Setting GPU Power Limit ==="
sudo nvidia-smi -pm 1 -i "${GPU_INDEX}"
sudo nvidia-smi -pl "${POWER_LIMIT_W}" -i "${GPU_INDEX}"

echo "=== [3/5] Generating temporary headless Xorg configuration ==="
cat <<CONFIG_EOF > "${CONFIG_FILE}"
Section "ServerLayout"
    Identifier     "Layout0"
    Screen      0  "Screen0"
    Option         "AutoAddDevices" "false"
    Option         "AutoEnableDevices" "false"
EndSection

Section "Device"
    Identifier     "Device0"
    Driver         "nvidia"
    VendorName     "NVIDIA Corporation"
    BusID          "${BUS_ID}"
    Option         "Coolbits" "28"
    Option         "UseDisplayDevice" "none"
EndSection

Section "Screen"
    Identifier     "Screen0"
    Device         "Device0"
    Monitor        "Monitor0"
    DefaultDepth    24
    Option         "UseDisplayDevice" "none"
    SubSection     "Display"
        Depth       24
        Modes      "1024x768"
    EndSubSection
EndSection

Section "Monitor"
    Identifier     "Monitor0"
EndSection
CONFIG_EOF

echo "=== [4/5] Spawning headless dummy Xorg server on ${DISPLAY_NUM} ==="
sudo Xorg "${DISPLAY_NUM}" -config "${CONFIG_FILE}" -noreset +extension GLX +extension RANDR > /tmp/xorg-headless.log 2>&1 &
XORG_PID=$!

cleanup() {
    echo "Cleaning up dummy Xorg server (PID: ${XORG_PID})..."
    sudo kill -15 "${XORG_PID}" 2>/dev/null || true
    rm -f "${CONFIG_FILE}"
}
trap cleanup EXIT

sleep 2

if ! kill -0 "${XORG_PID}" 2>/dev/null; then
    echo "Error: Headless Xorg server failed to start. Log output:" >&2
    cat /tmp/xorg-headless.log >&2
    exit 1
fi

echo "=== [5/5] Applying clock offsets via nvidia-settings ==="
export DISPLAY="${DISPLAY_NUM}"

nvidia-settings -a "[gpu:${GPU_INDEX}]/GPUGraphicsClockOffset[3]=${CORE_OFFSET_MHZ}"
nvidia-settings -a "[gpu:${GPU_INDEX}]/GPUMemoryTransferRateOffset[3]=${MEM_OFFSET_MHZ}"

echo "GPU tuning applied successfully! Core: ${CORE_OFFSET_MHZ}MHz, Mem: +${MEM_OFFSET_MHZ}MHz, Power: ${POWER_LIMIT_W}W."
