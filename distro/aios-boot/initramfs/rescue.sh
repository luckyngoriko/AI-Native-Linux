#!/bin/busybox ash
#
# AI-OS.NET Initramfs Rescue Shell
# Dropped when the init script encounters an unrecoverable error.
# Provides minimal diagnostic tools to inspect the system state.

FAILURE_REASON="${1:-unknown}"

# ── Banner ───────────────────────────────────────────────────────────────────

printf '\033[1;31m'
printf '╔══════════════════════════════════════════════════════════╗\n'
printf '║          AI-OS.NET INITRAMFS — RESCUE SHELL             ║\n'
printf '╚══════════════════════════════════════════════════════════╝\n'
printf '\033[0m\n'

echo "Failure reason: ${FAILURE_REASON}"
echo ""
echo "Available diagnostic commands:"
echo "  lsblk        — list block devices"
echo "  blkid        — print block device attributes"
echo "  mount        — show mounted filesystems"
echo "  dmesg | tail — recent kernel messages"
echo "  cat /proc/cmdline  — kernel command line"
echo "  cat /proc/partitions — partition table"
echo "  ls /dev/mapper/ — device mapper nodes"
echo "  tpm2_pcrread — read TPM2 PCR banks"
echo "  veritysetup status aios-root — dm-verity status"
echo "  cryptsetup status aios-cryptroot — LUKS status"
echo "  exit         — reboot (kernel panic will follow)"
echo ""
echo "Type 'exit' or press Ctrl-Alt-Del to reboot."
echo "────────────────────────────────────────────────────────────"
echo ""

# ── Block device quick overview ─────────────────────────────────────────────

if [ -x /sbin/lsblk ]; then
    echo "=== Block devices ==="
    lsblk -o NAME,SIZE,TYPE,MOUNTPOINT,LABEL,PARTLABEL 2>/dev/null || true
    echo ""
fi

if [ -x /sbin/blkid ]; then
    echo "=== Block device attributes ==="
    blkid 2>/dev/null || true
    echo ""
fi

if [ -e /proc/partitions ]; then
    echo "=== Kernel partitions ==="
    cat /proc/partitions
    echo ""
fi

if [ -d /dev/mapper ]; then
    echo "=== Device mapper nodes ==="
    ls -la /dev/mapper/ 2>/dev/null || echo "  (empty)"
    echo ""
fi

# ── TPM2 status ──────────────────────────────────────────────────────────────

if [ -x /usr/bin/tpm2_pcrread ] || [ -x /bin/tpm2_pcrread ]; then
    echo "=== TPM2 PCR values ==="
    TPM2PCR=$(command -v tpm2_pcrread 2>/dev/null || echo "")
    if [ -n "${TPM2PCR}" ]; then
        "${TPM2PCR}" sha256:0,1,2,3,4,5,6,7 2>/dev/null || echo "  TPM2 not available or accessible"
    fi
    echo ""
fi

# ── dm-verity / LUKS status ──────────────────────────────────────────────────

if [ -x /sbin/veritysetup ]; then
    echo "=== dm-verity status ==="
    veritysetup status aios-root 2>/dev/null || echo "  aios-root not active"
    echo ""
fi

if [ -x /sbin/cryptsetup ]; then
    echo "=== LUKS status ==="
    cryptsetup status aios-cryptroot 2>/dev/null || echo "  aios-cryptroot not active"
    echo ""
fi

# ── Kernel log tail ──────────────────────────────────────────────────────────

if [ -e /proc/kmsg ] || [ -x /bin/dmesg ]; then
    echo "=== Recent kernel messages ==="
    dmesg | tail -30 2>/dev/null || true
    echo ""
fi

# ── Mounted filesystems ──────────────────────────────────────────────────────

echo "=== Mounted filesystems ==="
mount 2>/dev/null || cat /proc/mounts 2>/dev/null || echo "  mount table unavailable"
echo ""

# ── Filesystem repair hint ───────────────────────────────────────────────────

echo "────────────────────────────────────────────────────────────"
echo "To attempt repair:"
echo "  fsck -y <root_device>"
echo "  mount -o remount,rw /newroot"
echo ""
echo "To continue boot manually after fix:"
echo "  exec switch_root /newroot /sbin/init"
echo "────────────────────────────────────────────────────────────"
echo ""

# ── Interactive shell ────────────────────────────────────────────────────────

export PS1='\033[1;31m[AIOS-RESCUE]\033[0m \w # '
export HOME=/
export PATH=/sbin:/bin:/usr/sbin:/usr/bin
export TERM=linux

exec /bin/sh
