#!/usr/bin/env bash
# =============================================================================
# AI-OS.NET mkrootfs.sh — Root Filesystem Builder
# Revision 4 — Bootable Distribution
# =============================================================================
# Creates the complete AI-OS.NET rootfs directory tree from scratch.
#
# Usage:
#   sudo ./mkrootfs.sh [--target /path/to/rootfs] [--force]
#
# Options:
#   --target DIR    Rootfs staging directory (default: /tmp/aios-rootfs)
#   --force         Remove target if it already exists
#   --dry-run       Print what would be created without making changes
#   --help          Show this message
#
# The script:
#   1. Creates the full FHS + AIOS directory tree
#   2. Copies default configuration files into place
#   3. Sets correct permissions and ownership
#   4. Creates symlinks (bin -> usr/bin, etc.)
#   5. Applies SELinux contexts (if SELinux tools are available)
#   6. Creates placeholder files where needed
# =============================================================================

set -euo pipefail

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET_ROOT=""
DRY_RUN=false
FORCE=false

# SELinux context map: directory path => SELinux type
declare -A SELINUX_CONTEXTS

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
die() {
    echo "ERROR: $*" >&2
    exit 1
}

log() {
    local level="$1"; shift
    echo "[${level}] $*"
}

run_cmd() {
    if $DRY_RUN; then
        echo "DRY-RUN: $*"
        return 0
    fi
    "$@"
}

makedir() {
    local path="$1"
    local mode="${2:-0755}"
    local selinux_type="${3:-}"

    echo "  mkdir -p -m $mode $TARGET_ROOT/$path"

    if $DRY_RUN; then
        return 0
    fi

    mkdir -p -m "$mode" "${TARGET_ROOT}/${path}"

    if [[ -n "$selinux_type" ]] && command -v chcon &>/dev/null; then
        chcon -t "$selinux_type" "${TARGET_ROOT}/${path}" 2>/dev/null || true
    fi
}

heredoc_file() {
    local dst="$1"
    local mode="${2:-0644}"
    local heredoc
    heredoc="$(cat)"

    echo "  write $TARGET_ROOT/$dst"

    if $DRY_RUN; then
        return 0
    fi

    mkdir -p "$(dirname "${TARGET_ROOT}/${dst}")"
    cat > "${TARGET_ROOT}/${dst}" <<< "$heredoc"
    chmod "$mode" "${TARGET_ROOT}/${dst}"
}

copyfile() {
    local src="$1"
    local dst="$2"
    local mode="${3:-0644}"

    echo "  cp $src -> $TARGET_ROOT/$dst"

    if $DRY_RUN; then
        return 0
    fi

    if [[ -f "$src" ]]; then
        mkdir -p "$(dirname "${TARGET_ROOT}/${dst}")"
        cp "$src" "${TARGET_ROOT}/${dst}"
        chmod "$mode" "${TARGET_ROOT}/${dst}"
    else
        log WARN "Source file not found, skipping: $src"
    fi
}

makesymlink() {
    local target="$1"
    local linkpath="$2"

    echo "  ln -sfn $target $TARGET_ROOT/$linkpath"

    if $DRY_RUN; then
        return 0
    fi

    ln -sfn "$target" "${TARGET_ROOT}/${linkpath}"
}

touchplaceholder() {
    local path="$1"
    local mode="${2:-0644}"

    echo "  touch $TARGET_ROOT/$path"

    if $DRY_RUN; then
        return 0
    fi

    touch "${TARGET_ROOT}/${path}"
    chmod "$mode" "${TARGET_ROOT}/${path}"
}

# Attempt to apply SELinux file context from policy
apply_selinux() {
    local path="$1"
    local ftype="$2"

    if $DRY_RUN; then
        return 0
    fi

    if command -v restorecon &>/dev/null && command -v semanage &>/dev/null; then
        restorecon -RF "${TARGET_ROOT}/${path}" 2>/dev/null || true
    fi
}

# ---------------------------------------------------------------------------
# Parse arguments
# ---------------------------------------------------------------------------
parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --target)
                TARGET_ROOT="$2"; shift 2 ;;
            --force)
                FORCE=true; shift ;;
            --dry-run)
                DRY_RUN=true; shift ;;
            --help)
                head -30 "$0" | grep -E '^(#|Usage)' | sed 's/^# //; s/^#//'
                exit 0 ;;
            *)
                die "Unknown argument: $1 (use --help)" ;;
        esac
    done

    TARGET_ROOT="${TARGET_ROOT:-/tmp/aios-rootfs}"
    TARGET_ROOT="$(realpath -m "$TARGET_ROOT")"
}

# ---------------------------------------------------------------------------
# Pre-flight checks
# ---------------------------------------------------------------------------
preflight() {
    log INFO "AI-OS.NET mkrootfs.sh — Revision 4"
    log INFO "Target rootfs: $TARGET_ROOT"
    log INFO "Source directory: $SCRIPT_DIR"

    if $DRY_RUN; then
        log INFO "DRY-RUN mode — no filesystem changes will be made"
        return 0
    fi

    if [[ "$(id -u)" -ne 0 ]]; then
        if [[ -z "${AIOS_SKIP_ROOT_CHECK:-}" ]]; then
            die "This script must be run as root (sudo) to set permissions and SELinux contexts."
        fi
        log WARN "Running as non-root (AIOS_SKIP_ROOT_CHECK set) — SELinux contexts will NOT be applied"
    fi

    if [[ "$TARGET_ROOT" == "/" ]] || [[ "$TARGET_ROOT" == "/usr" ]] || [[ "$TARGET_ROOT" == "/etc" ]]; then
        die "Refusing to target a live system directory: $TARGET_ROOT"
    fi

    if [[ -d "$TARGET_ROOT" ]] && [[ -n "$(ls -A "$TARGET_ROOT" 2>/dev/null)" ]]; then
        if $FORCE; then
            log WARN "Removing existing rootfs at $TARGET_ROOT"
            rm -rf "$TARGET_ROOT"
        else
            die "Target $TARGET_ROOT already exists and is not empty. Use --force to overwrite."
        fi
    fi

    # Check SELinux availability
    if command -v chcon &>/dev/null; then
        log INFO "SELinux tools found — contexts will be applied"
    else
        log WARN "SELinux tools not found — contexts will NOT be applied (non-SELinux host)"
    fi
}

# ---------------------------------------------------------------------------
# Directory creation — top-level
# ---------------------------------------------------------------------------
create_top_level() {
    log INFO "Creating top-level directory structure..."

    makedir ""                             0755 "root_t"

    # Standard FHS symlinks (merged-usr layout)
    makesymlink "usr/bin"  "bin"
    makesymlink "usr/sbin" "sbin"
    makesymlink "usr/lib"  "lib"

    # Standard FHS top-level directories
    makedir "boot"         0755 "boot_t"
    makedir "dev"          0755 "device_t"
    makedir "etc"          0755 "etc_t"
    makedir "home"         0755 "home_root_t"
    makedir "media"        0755 "mnt_t"
    makedir "mnt"          0755 "mnt_t"
    makedir "opt"          0755 "usr_t"
    makedir "proc"         0555 "proc_t"
    makedir "root"         0700 "admin_home_t"
    makedir "run"          0755 "var_run_t"
    makedir "sys"          0555 "sysfs_t"
    makedir "tmp"          1777 "tmp_t"
    makedir "usr"          0755 "usr_t"
    makedir "var"          0755 "var_t"
}

# ---------------------------------------------------------------------------
# /etc — system configuration
# ---------------------------------------------------------------------------
create_etc() {
    log INFO "Creating /etc structure..."

    # AIOS core configuration
    makedir "etc/aios"                         0755 "aios_etc_t"
    makedir "etc/aios/verity"                  0700 "aios_etc_t"
    makedir "etc/aios/capsules"                0755 "aios_etc_t"

    copyfile "$SCRIPT_DIR/aios-config.toml"    "etc/aios/config.toml"        0640
    touchplaceholder                            "etc/aios/sealed-key.blob"    0400
    touchplaceholder                            "etc/aios/verity/roothash.sig" 0400
    touchplaceholder                            "etc/aios/capsules/.gitkeep"  0644

    # SELinux
    makedir "etc/selinux"                      0755 "selinux_config_t"
    makedir "etc/selinux/aios"                 0755 "selinux_config_t"
    makedir "etc/selinux/aios/policy"          0755 "selinux_config_t"

    # SELinux config file
    heredoc_file "etc/selinux/config" 0644 <<'SELINUXCONF'
# AI-OS.NET SELinux Configuration
SELINUX=enforcing
SELINUXTYPE=aios
SETLOCALDEFS=0
SELINUXCONF

    touchplaceholder "etc/selinux/aios/policy/policy.33" 0644

    # Systemd
    makedir "etc/systemd"                      0755 "systemd_unit_t"
    makedir "etc/systemd/system"               0755 "systemd_unit_t"
    makedir "etc/tmpfiles.d"                   0755 "etc_t"

    # tmpfiles.d entry for /run/aios
    heredoc_file "etc/tmpfiles.d/aios.conf" 0644 <<'TMPFILES'
# AI-OS.NET runtime directories
d /run/aios             0755 root root -
d /run/aios/lock        0755 root root -
TMPFILES

    # Core configuration files
    copyfile "$SCRIPT_DIR/fstab"               "etc/fstab"                   0644
    copyfile "$SCRIPT_DIR/crypttab"            "etc/crypttab"                0600
    copyfile "$SCRIPT_DIR/hostname"            "etc/hostname"                0644

    # Locale, console, os-release
    heredoc_file "etc/locale.conf" 0644 <<'EOF'
LANG=en_US.UTF-8
EOF
    heredoc_file "etc/vconsole.conf" 0644 <<'EOF'
KEYMAP=us
FONT=ter-v32n
EOF

    # os-release
    heredoc_file "etc/os-release" 0644 <<'OSRELEASE'
NAME="AI-OS.NET"
VERSION="4 (Bootable Distribution)"
ID=aios
ID_LIKE="linux"
VERSION_ID=4
PRETTY_NAME="AI-OS.NET REV4"
ANSI_COLOR="1;34"
HOME_URL="https://aios.net"
SUPPORT_URL="https://aios.net/support"
BUG_REPORT_URL="https://aios.net/issues"
OSRELEASE

    # Default machine-id (will be regenerated on first boot by systemd-firstboot)
    heredoc_file "etc/machine-id" 0444 <<'EOF'
uninitialized
EOF

    # SELinux autorelabel touch file
    touchplaceholder ".autorelabel" 0644

    # Generate systemd service unit files
    create_systemd_units
}

# ---------------------------------------------------------------------------
# Systemd service units
# ---------------------------------------------------------------------------
create_systemd_units() {
    log INFO "Creating systemd service unit files..."

    # aios-capability-runtime.service
    heredoc_file "etc/systemd/system/aios-capability-runtime.service" 0644 <<'UNIT'
[Unit]
Description=AIOS Capability Runtime
Documentation=https://aios.net/docs/capability-runtime
After=network.target aios-policy-kernel.service
Requires=aios-policy-kernel.service
Wants=network.target

[Service]
Type=notify
ExecStart=/usr/bin/aios-capability-runtime
Restart=always
RestartSec=5
User=aios
Group=aios
ProtectSystem=strict
ProtectHome=yes
NoNewPrivileges=yes
PrivateTmp=yes
ReadOnlyPaths=/
ReadWritePaths=/run/aios /var/lib/aios /var/log/aios
SELinuxContext=system_u:system_r:aios_capability_runtime_t:s0

[Install]
WantedBy=multi-user.target
UNIT

    # aios-policy-kernel.service
    heredoc_file "etc/systemd/system/aios-policy-kernel.service" 0644 <<'UNIT'
[Unit]
Description=AIOS Policy Kernel
Documentation=https://aios.net/docs/policy-kernel
Before=aios-capability-runtime.service

[Service]
Type=notify
ExecStart=/usr/lib/systemd/system/aios-policy-kernel
Restart=always
RestartSec=5
User=aios
Group=aios
ProtectSystem=strict
NoNewPrivileges=yes
PrivateTmp=yes
ReadWritePaths=/run/aios /var/lib/aios/policy
SELinuxContext=system_u:system_r:aios_policy_kernel_t:s0

[Install]
WantedBy=multi-user.target
UNIT

    # aios-evidence-log.service
    heredoc_file "etc/systemd/system/aios-evidence-log.service" 0644 <<'UNIT'
[Unit]
Description=AIOS Evidence Log Service
Documentation=https://aios.net/docs/evidence-log
After=aios-capability-runtime.service
Requires=aios-capability-runtime.service

[Service]
Type=simple
ExecStart=/usr/lib/systemd/system/aios-evidence-log
Restart=always
RestartSec=10
User=aios
Group=aios
NoNewPrivileges=yes
ReadWritePaths=/var/lib/aios/evidence /run/aios
SELinuxContext=system_u:system_r:aios_evidence_t:s0

[Install]
WantedBy=multi-user.target
UNIT

    # aios-sandbox.service
    heredoc_file "etc/systemd/system/aios-sandbox.service" 0644 <<'UNIT'
[Unit]
Description=AIOS Sandbox Service
Documentation=https://aios.net/docs/sandbox
After=aios-capability-runtime.service
Requires=aios-capability-runtime.service

[Service]
Type=notify
ExecStart=/usr/lib/systemd/system/aios-sandbox
Restart=always
RestartSec=5
User=aios
Group=aios
NoNewPrivileges=yes
SELinuxContext=system_u:system_r:aios_sandbox_t:s0

[Install]
WantedBy=multi-user.target
UNIT

    # aios-fs-mount.service — runs early to set up overlay mounts
    heredoc_file "etc/systemd/system/aios-fs-mount.service" 0644 <<'UNIT'
[Unit]
Description=AIOS Filesystem Mount Service
Documentation=https://aios.net/docs/filesystem
DefaultDependencies=no
After=local-fs-pre.target systemd-tmpfiles-setup.service
Before=local-fs.target

[Service]
Type=oneshot
ExecStart=/usr/lib/systemd/system/aios-fs-mount
RemainAfterExit=yes
SELinuxContext=system_u:system_r:aios_fs_t:s0

[Install]
WantedBy=local-fs.target
UNIT

    # aios-first-boot.service
    heredoc_file "etc/systemd/system/aios-first-boot.service" 0644 <<'UNIT'
[Unit]
Description=AIOS First Boot Setup
Documentation=https://aios.net/docs/first-boot
ConditionFirstBoot=yes
After=aios-fs-mount.service
Before=aios-capability-runtime.service

[Service]
Type=oneshot
ExecStart=/usr/bin/aios first-boot
RemainAfterExit=no
SELinuxContext=system_u:system_r:aios_first_boot_t:s0

[Install]
WantedBy=multi-user.target
UNIT

    # aios-capsule-engram.service
    heredoc_file "etc/systemd/system/aios-capsule-engram.service" 0644 <<'UNIT'
[Unit]
Description=AIOS Capsule Engram Service
Documentation=https://aios.net/docs/capsule-engram
After=aios-sandbox.service
Requires=aios-sandbox.service

[Service]
Type=simple
ExecStart=/usr/lib/systemd/system/aios-capsule-engram
Restart=always
RestartSec=10
User=aios
Group=aios
NoNewPrivileges=yes
SELinuxContext=system_u:system_r:aios_capsule_t:s0

[Install]
WantedBy=multi-user.target
UNIT
}

# ---------------------------------------------------------------------------
# /usr — shared read-only data
# ---------------------------------------------------------------------------
create_usr() {
    log INFO "Creating /usr structure..."

    makedir "usr/bin"    0755 "bin_t"
    makedir "usr/sbin"   0755 "bin_t"
    makedir "usr/lib"    0755 "lib_t"

    # AIOS shared libraries (all 25 crates)
    makedir "usr/lib/aios" 0755 "lib_t"

    local -a CRATE_NAMES=(
        "action" "apps" "backup" "capability_runtime" "cognitive"
        "container" "distribution" "eval" "evidence" "fleet"
        "fs" "hardware" "integration" "mobile" "network"
        "policy" "recovery" "renderer_cli" "renderer_kde" "renderer_web"
        "sandbox" "sgr" "time" "vault" "verification"
    )

    for crate in "${CRATE_NAMES[@]}"; do
        touchplaceholder "usr/lib/aios/libaios_${crate}.so" 0755
    done

    # Systemd helper binaries
    makedir "usr/lib/systemd/system" 0755 "lib_t"
    touchplaceholder "usr/lib/systemd/system/aios-capability-runtime" 0755
    touchplaceholder "usr/lib/systemd/system/aios-policy-kernel"      0755
    touchplaceholder "usr/lib/systemd/system/aios-evidence-log"       0755
    touchplaceholder "usr/lib/systemd/system/aios-sandbox"            0755
    touchplaceholder "usr/lib/systemd/system/aios-fs-mount"           0755
    touchplaceholder "usr/lib/systemd/system/aios-capsule-engram"     0755

    # CLI binaries
    touchplaceholder "usr/bin/aios"                         0755
    touchplaceholder "usr/bin/aios-capability-runtime"      0755
    touchplaceholder "usr/sbin/aios-init"                   0755

    # /usr/share
    makedir "usr/share/aios"                    0755 "usr_t"
    makedir "usr/share/aios/selinux"            0755 "usr_t"
    makedir "usr/share/aios/caps"               0755 "usr_t"
    makedir "usr/share/aios/evidence"           0755 "usr_t"
    makedir "usr/share/aios/policy"             0755 "usr_t"
    makedir "usr/share/aios/docs"               0755 "usr_t"
    makedir "usr/share/licenses/aios"           0755 "usr_t"

    # Default capability grants
    heredoc_file "usr/share/aios/caps/default.json" 0644 <<'DEFAULTCAPS'
{
  "version": 1,
  "grants": {
    "aios-capability-runtime": {
      "allow": ["system.info", "policy.evaluate", "evidence.write"],
      "deny": ["network.outbound", "filesystem.write_root"]
    },
    "aios-policy-kernel": {
      "allow": ["policy.evaluate", "policy.cache_read", "policy.cache_write"],
      "deny": ["network.outbound"]
    },
    "aios-evidence-log": {
      "allow": ["evidence.write", "evidence.read", "evidence.compact"],
      "deny": ["network.outbound", "filesystem.write_root"]
    }
  },
  "default_deny_all": true
}
DEFAULTCAPS

    # Constitutional policy definition
    heredoc_file "usr/share/aios/policy/constitutional.json" 0644 <<'CONSTITUTIONAL'
{
  "version": 4,
  "name": "AI-OS.NET Constitutional Policy",
  "description": "Immutable core policy defining the AI-OS.NET operating constraints.",
  "rules": [
    {
      "id": "POL-001",
      "name": "No Unexplained Privilege Escalation",
      "condition": "action.requires_privilege_escalation && !action.has_explicit_grant",
      "effect": "deny",
      "evidence_log": true
    },
    {
      "id": "POL-002",
      "name": "Evidence Before Action",
      "condition": "action.is_mutating && !evidence.chain_includes(action.hash)",
      "effect": "deny",
      "evidence_log": true
    },
    {
      "id": "POL-003",
      "name": "Time Attestation Required",
      "condition": "!time.is_attested || time.grade < \"ATTESTED_SINGLE\"",
      "effect": "deny",
      "evidence_log": true
    },
    {
      "id": "POL-004",
      "name": "Capsule Isolation",
      "condition": "capsule.id != action.capsule_id",
      "effect": "deny",
      "evidence_log": true
    },
    {
      "id": "POL-005",
      "name": "Network Outbound Gate",
      "condition": "action.type == \"network.outbound\" && !network.is_granted(action.destination)",
      "effect": "deny",
      "evidence_log": true
    }
  ]
}
CONSTITUTIONAL

    # Placeholder for license
    touchplaceholder "usr/share/licenses/aios/LICENSE" 0644
}

# ---------------------------------------------------------------------------
# /var — variable data
# ---------------------------------------------------------------------------
create_var() {
    log INFO "Creating /var structure..."

    makedir "var/lib"                     0755 "var_lib_t"
    makedir "var/lib/aios"                0755 "var_lib_t"
    makedir "var/lib/aios/evidence"       0755 "var_lib_t"
    makedir "var/lib/aios/policy"         0755 "var_lib_t"
    makedir "var/lib/aios/capsules"       0755 "var_lib_t"
    makedir "var/lib/aios/backup"         0755 "var_lib_t"
    makedir "var/lib/aios/vault"          0700 "var_lib_t"

    makedir "var/log"                     0755 "var_log_t"
    makedir "var/log/aios"                0755 "var_log_t"

    makedir "var/cache"                   0755 "var_t"
    makedir "var/cache/aios"              0755 "var_t"

    touchplaceholder "var/lib/aios/evidence/.gitkeep" 0644
    touchplaceholder "var/lib/aios/policy/.gitkeep"   0644
    touchplaceholder "var/lib/aios/capsules/.gitkeep" 0644
    touchplaceholder "var/lib/aios/backup/.gitkeep"   0644
    touchplaceholder "var/log/aios/.gitkeep"          0644
}

# ---------------------------------------------------------------------------
# /boot — kernel + bootloader
# ---------------------------------------------------------------------------
create_boot() {
    log INFO "Creating /boot structure..."

    makedir "boot"                         0755 "boot_t"
    makedir "boot/loader"                  0755 "boot_t"
    makedir "boot/loader/entries"          0755 "boot_t"
    makedir "boot/EFI"                     0755 "boot_t"
    makedir "boot/EFI/BOOT"               0755 "boot_t"

    copyfile "$SCRIPT_DIR/loader-entry.conf" "boot/loader/entries/aios.conf" 0644

    touchplaceholder "boot/vmlinuz-aios"       0644
    touchplaceholder "boot/initramfs-aios.img" 0644
    touchplaceholder "boot/EFI/BOOT/BOOTX64.EFI" 0644
}

# ---------------------------------------------------------------------------
# /opt — optional packages
# ---------------------------------------------------------------------------
create_opt() {
    makedir "opt/aios"                     0755 "usr_t"
    makedir "opt/aios/cognitive"           0755 "usr_t"
}

# ---------------------------------------------------------------------------
# /run/aios runtime directories (created at boot by tmpfiles.d, but staged here)
# ---------------------------------------------------------------------------
create_run_stub() {
    makedir "run/aios"                     0755 "var_run_t"
    makedir "run/aios/lock"                0755 "var_run_t"
    touchplaceholder "run/aios/.gitkeep"   0644
}

# ---------------------------------------------------------------------------
# Create AIOS system user/group entries (stage in /etc for reference)
# ---------------------------------------------------------------------------
create_passwd_group() {
    log INFO "Creating /etc/passwd, /etc/group, /etc/shadow stubs..."

    heredoc_file "etc/group" 0644 <<'GROUP'
root:x:0:
aios:x:980:
GROUP

    heredoc_file "etc/passwd" 0644 <<'PASSWD'
root:x:0:0:root:/root:/bin/bash
aios:x:980:980:AIOS Runtime:/var/lib/aios:/sbin/nologin
PASSWD

    heredoc_file "etc/shadow" 0000 <<'SHADOW'
root:!!:20000:0:99999:7:::
aios:!!:20000:0:99999:7:::
SHADOW
}

# ---------------------------------------------------------------------------
# Post-creation: apply SELinux and produce summary
# ---------------------------------------------------------------------------
post_create() {
    log INFO "Post-creation tasks..."

    if $DRY_RUN; then
        return 0
    fi

    # Attempt to relabel the entire tree if SELinux is available
    if command -v setfiles &>/dev/null && [[ -f /etc/selinux/aios/contexts/files/file_contexts ]]; then
        log INFO "Applying SELinux file contexts..."
        setfiles -r "$TARGET_ROOT" /etc/selinux/aios/contexts/files/file_contexts "$TARGET_ROOT" 2>/dev/null || \
            log WARN "setfiles failed — contexts may not be fully applied"
    fi

    # Summary
    local dir_count
    local file_count
    dir_count=$(find "$TARGET_ROOT" -type d 2>/dev/null | wc -l)
    file_count=$(find "$TARGET_ROOT" -type f 2>/dev/null | wc -l)

    log INFO "============================================"
    log INFO "Root filesystem created at: $TARGET_ROOT"
    log INFO "Directories: $dir_count"
    log INFO "Files:       $file_count"
    log INFO "============================================"

    # Disk usage
    du -sh "$TARGET_ROOT" 2>/dev/null || true
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
main() {
    parse_args "$@"
    preflight

    create_top_level
    create_etc
    create_usr
    create_var
    create_boot
    create_opt
    create_run_stub
    create_passwd_group
    post_create

    log INFO "mkrootfs.sh complete."
}

main "$@"
