#!/bin/sh
#
# AI-OS.NET Initramfs Builder
# Builds a compressed cpio initramfs from the initramfs sources directory.
#
# Usage:
#   ./build-initramfs.sh [output_path]
#
# Default output: ../aios-initramfs.img
#
# Requirements:
#   - busybox (statically linked)
#   - Required tools copied from host or build sysroot:
#       tpm2_unseal (from tpm2-tools), cryptsetup, veritysetup,
#       load_policy (from selinux-policy), switch_root (from util-linux)
#   - cpio, xz

set -eu

SCRIPT_DIR="$(dirname "$0")"
cd "${SCRIPT_DIR}"

OUTPUT="${1:-$(dirname "${SCRIPT_DIR}")/aios-initramfs.img}"
BUILD_DIR="$(mktemp -d -t aios-initramfs-XXXXXX)"

cleanup() {
    rm -rf "${BUILD_DIR}"
}
trap cleanup EXIT INT TERM

echo "[AIOS-BUILD] Building initramfs..."
echo "[AIOS-BUILD] Output: ${OUTPUT}"
echo "[AIOS-BUILD] Build dir: ${BUILD_DIR}"

# ── Step 1: Create directory structure ───────────────────────────────────────

echo "[AIOS-BUILD] Creating directory structure..."

mkdir -p "${BUILD_DIR}"/{bin,sbin,usr/bin,usr/sbin,lib,lib64,usr/lib,usr/lib64}
mkdir -p "${BUILD_DIR}"/{etc,dev,proc,sys,run,tmp,newroot}
mkdir -p "${BUILD_DIR}/etc/aios/"{verity,selinux}
mkdir -p "${BUILD_DIR}/etc/selinux/aios/policy"
mkdir -p "${BUILD_DIR}/dev/mapper"
mkdir -p "${BUILD_DIR}/dev/disk/by-partlabel"
mkdir -p "${BUILD_DIR}"/sys/fs/selinux

chmod 755 "${BUILD_DIR}"/{bin,sbin,usr/bin,usr/sbin,etc,dev,proc,sys,run,tmp,newroot}
chmod 1777 "${BUILD_DIR}/tmp"

# ── Step 2: Copy busybox and create symlinks ─────────────────────────────────

echo "[AIOS-BUILD] Installing busybox..."

BUSYBOX=""
for _bb in /usr/bin/busybox /bin/busybox /sbin/busybox; do
    if [ -x "${_bb}" ]; then
        BUSYBOX="${_bb}"
        break
    fi
done

if [ -z "${BUSYBOX}" ]; then
    echo "[AIOS-BUILD] ERROR: busybox not found. Install busybox-static first."
    echo "  Debian/Ubuntu: apt install busybox-static"
    echo "  Fedora/RHEL:   dnf install busybox"
    echo "  Arch:          pacman -S busybox"
    exit 1
fi

echo "[AIOS-BUILD] Using busybox: ${BUSYBOX}"

cp "${BUSYBOX}" "${BUILD_DIR}/bin/busybox"
chmod 755 "${BUILD_DIR}/bin/busybox"

# Create essential busybox symlinks
# These are the applets needed by the init script
BUSYBOX_APPLETS="
    sh ash cat cp mv rm mkdir ls ln mount umount mountpoint
    sleep printf echo head tr chmod chown dd sync
    blkid switch_root grep sed find dirname basename
    test [ [[ true false wc wget tftp
    kill ps pidof poweroff reboot halt
    modprobe insmod rmmod depmod losetup
    mknod mdev
"

for _app in ${BUSYBOX_APPLETS}; do
    ln -sf /bin/busybox "${BUILD_DIR}/bin/${_app}" 2>/dev/null || true
done

# Essential shell as /bin/sh
ln -sf busybox "${BUILD_DIR}/bin/sh"

echo "[AIOS-BUILD] Busybox applets created ($(echo "${BUSYBOX_APPLETS}" | wc -w) applets)."

# ── Step 3: Copy required external tools ─────────────────────────────────────

echo "[AIOS-BUILD] Installing external tools..."

copy_tool() {
    local _tool_name="$1"
    local _tool_src=""
    for _path in "/usr/sbin/${_tool_name}" "/sbin/${_tool_name}" \
                 "/usr/bin/${_tool_name}" "/bin/${_tool_name}"; do
        if [ -x "${_path}" ]; then
            _tool_src="${_path}"
            break
        fi
    done
    if [ -n "${_tool_src}" ]; then
        cp "${_tool_src}" "${BUILD_DIR}/sbin/${_tool_name}"
        chmod 755 "${BUILD_DIR}/sbin/${_tool_name}"
        echo "  [OK] ${_tool_name} -> ${_tool_src}"
    else
        echo "  [SKIP] ${_tool_name} not found on host (optional)"
    fi
}

copy_tool_with_libs() {
    local _tool_name="$1"
    local _tool_src=""
    for _path in "/usr/sbin/${_tool_name}" "/sbin/${_tool_name}" \
                 "/usr/bin/${_tool_name}" "/bin/${_tool_name}"; do
        if [ -x "${_path}" ]; then
            _tool_src="${_path}"
            break
        fi
    done
    if [ -n "${_tool_src}" ]; then
        cp "${_tool_src}" "${BUILD_DIR}/sbin/${_tool_name}"
        chmod 755 "${BUILD_DIR}/sbin/${_tool_name}"
        # Copy shared libraries the tool depends on
        if command -v ldd >/dev/null 2>&1; then
            ldd "${_tool_src}" 2>/dev/null | while read -r _lib; do
                case "${_lib}" in
                    *'=> '*)
                        _libpath="${_lib##*=> }"
                        _libpath="${_libpath%% *}"
                        if [ -f "${_libpath}" ] && [ ! -f "${BUILD_DIR}${_libpath}" ]; then
                            _libdir="$(dirname "${_libpath}")"
                            mkdir -p "${BUILD_DIR}${_libdir}"
                            cp "${_libpath}" "${BUILD_DIR}${_libpath}"
                        fi
                        ;;
                esac
            done
        else
            echo "    (ldd not available, library deps for ${_tool_name} not copied)"
        fi
        echo "  [OK] ${_tool_name} -> ${_tool_src}"
    else
        echo "  [SKIP] ${_tool_name} not found on host (optional)"
    fi
}

# Core tools (required)
copy_tool_with_libs cryptsetup
copy_tool_with_libs veritysetup
copy_tool_with_libs load_policy

# TPM2 tools
copy_tool_with_libs tpm2_unseal
copy_tool tpm2_pcrread
copy_tool tpm2_getrandom

# Additional diagnostic tools for rescue shell
copy_tool lsblk
copy_tool dmesg
copy_tool fsck
copy_tool keyctl

# ── Step 4: Install init scripts and configuration ───────────────────────────

echo "[AIOS-BUILD] Installing init scripts..."

cp init "${BUILD_DIR}/init"
chmod 755 "${BUILD_DIR}/init"

cp rescue.sh "${BUILD_DIR}/bin/rescue.sh"
chmod 755 "${BUILD_DIR}/bin/rescue.sh"

cp aios-preinit "${BUILD_DIR}/etc/aios/preinit"
chmod 644 "${BUILD_DIR}/etc/aios/preinit"

# Optional: copy kernel modules if present
if [ -d /lib/modules ]; then
    KMOD_VER=$(ls /lib/modules/ 2>/dev/null | head -1)
    if [ -n "${KMOD_VER}" ]; then
        echo "[AIOS-BUILD] Installing kernel modules for ${KMOD_VER}..."
        mkdir -p "${BUILD_DIR}/lib/modules"
        cp -a "/lib/modules/${KMOD_VER}" "${BUILD_DIR}/lib/modules/" 2>/dev/null || \
            echo "  [SKIP] Could not copy kernel modules"
    fi
fi

# ── Step 5: Basic /etc files ─────────────────────────────────────────────────

echo "[AIOS-BUILD] Creating /etc files..."

# Minimal fstab
cat > "${BUILD_DIR}/etc/fstab" << 'FSTAB_EOF'
proc    /proc    proc    defaults,nosuid,nodev,noexec    0 0
sysfs   /sys     sysfs   defaults,nosuid,nodev,noexec    0 0
devtmpfs /dev    devtmpfs defaults,nosuid,noexec          0 0
tmpfs   /run     tmpfs   defaults,nosuid,nodev,mode=755   0 0
tmpfs   /tmp     tmpfs   defaults,nosuid,nodev            0 0
FSTAB_EOF

# inittab (busybox init fallback)
cat > "${BUILD_DIR}/etc/inittab" << 'INITTAB_EOF'
::sysinit:/init
::respawn:-/bin/sh
::ctrlaltdel:/sbin/reboot
::shutdown:/sbin/poweroff
INITTAB_EOF

# Minimal passwd/group
cat > "${BUILD_DIR}/etc/passwd" << 'PASSWD_EOF'
root:x:0:0:root:/root:/bin/sh
nobody:x:65534:65534:nobody:/nonexistent:/bin/false
PASSWD_EOF

cat > "${BUILD_DIR}/etc/group" << 'GROUP_EOF'
root:x:0:
tty:x:5:
nobody:x:65534:
GROUP_EOF

# Basic nsswitch.conf
cat > "${BUILD_DIR}/etc/nsswitch.conf" << 'NSS_EOF'
passwd:  files
group:   files
shadow:  files
hosts:   files dns
NSS_EOF

# Basic hostname
echo "aios-initramfs" > "${BUILD_DIR}/etc/hostname"

# ── Step 6: Pack into cpio archive ──────────────────────────────────────────

echo "[AIOS-BUILD] Packing initramfs (cpio | xz)..."

(
    cd "${BUILD_DIR}"
    find . -print0 | sort -z | cpio --quiet -0 -o -H newc 2>/dev/null | \
        xz -9 -C crc32 --threads=0 > "${OUTPUT}"
)

IMGSIZE=$(stat -c '%s' "${OUTPUT}" 2>/dev/null || stat -f '%z' "${OUTPUT}" 2>/dev/null)
echo "[AIOS-BUILD] Initramfs built successfully!"
echo "[AIOS-BUILD] Output: ${OUTPUT}"
echo "[AIOS-BUILD] Size:   ${IMGSIZE} bytes ($(( IMGSIZE / 1024 )) KB)"

# ── Step 7: Optional — verify contents ──────────────────────────────────────

if command -v lsinitcpio >/dev/null 2>&1; then
    echo ""
    echo "[AIOS-BUILD] Contents preview:"
    lsinitcpio "${OUTPUT}" 2>/dev/null | head -60 || true
elif command -v cpio >/dev/null 2>&1; then
    echo ""
    echo "[AIOS-BUILD] Contents preview (first 60 lines):"
    xzcat "${OUTPUT}" 2>/dev/null | cpio -t 2>/dev/null | head -60 || true
fi

echo ""
echo "[AIOS-BUILD] Done."
echo ""
echo "To boot with this initramfs, add to your bootloader config:"
echo "  initrd /aios-initramfs.img"
echo ""
echo "QEMU test command:"
echo "  qemu-system-x86_64 -kernel /boot/vmlinuz-linux \\"
echo "    -initrd ${OUTPUT} \\"
echo "    -append 'root=/dev/mapper/aios-root ro quiet'"
echo ""
