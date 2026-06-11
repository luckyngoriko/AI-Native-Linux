# AI-OS.NET Installer — Revision 4

Bare-metal installer for the AI-OS.NET Linux distribution. Installs from a live
ISO/USB environment to a physical disk with full-disk encryption (LUKS2), TPM2
sealing, dm-verity integrity, SELinux enforcing, and systemd-boot.

## Quick Start

Boot the AI-OS.NET live ISO/USB, then:

```bash
sudo aios-installer
```

The installer guides you through disk selection, encryption setup, and
configuration. At the end it displays a **recovery key** — record this
offline.

## Hardware Requirements

| Component          | Minimum                                       |
|--------------------|-----------------------------------------------|
| CPU                | x86_64-v3 (AVX2) or later                     |
| RAM                | 4 GB (8 GB recommended for cognitive models)   |
| Disk               | 40 GB (NVMe or SATA SSD recommended)          |
| Firmware           | UEFI 2.8+ (Secure Boot recommended)           |
| TPM                | TPM 2.0 (required for automatic disk unlock)   |
| Network            | Ethernet or Wi-Fi (for NTP during install)     |

## Installation Phases

The installer (`aios-installer.sh`) runs through 14 phases:

| Phase | Description                                      |
|-------|--------------------------------------------------|
| 0     | Environment check — tools, UEFI, TPM, squashfs    |
| 1     | Disk selection — enumerate, validate, confirm     |
| 2     | Confirmation — explicit user consent to destroy   |
| 3     | Partitioning — GPT: ESP + Boot + LUKS root        |
| 4     | LUKS2 encryption — argon2id PBKDF, random key     |
| 5     | Filesystem creation — FAT32 ESP, ext4 boot/root   |
| 6     | Rootfs deployment — unsquashfs from live media    |
| 7     | System configuration — fstab, crypttab, hostname  |
| 8     | dm-verity setup — root hash tree                  |
| 9     | Bootloader — systemd-boot installed to ESP        |
| 10    | TPM2 sealing — PCRs 0+1+7 enrolled in LUKS header |
| 11    | Recovery key — displayed, must be saved offline   |
| 12    | SELinux policy — enforcing, .autorelabel          |
| 13    | First-boot flag — triggers wizard on next boot    |
| 14    | Unmount + completion                              |

## Partition Layout

```
/dev/<disk>
  ├── p1  (512 MiB)  EFI System Partition   FAT32   /boot/efi
  ├── p2  (1 GiB)    Linux filesystem        ext4    /boot
  └── p3  (rest)     Linux LUKS              LUKS2   /
                         └── aios-cryptroot   ext4
```

## Usage

### Interactive Installer

```bash
sudo aios-installer
sudo aios-installer --disk /dev/nvme0n1 --hostname my-aios
sudo aios-installer --hostname aios-dev --profile SECURE_DEFAULT
```

### Non-Interactive (CI/Automation)

```bash
AIOS_TARGET_DISK=/dev/vda AIOS_HOSTNAME=aios-ci AIOS_CONFIRM_SKIP=1 \
    sudo -E bash aios-quick-install.sh
```

### Quick Installer Options

| Variable              | Default                       | Description                    |
|-----------------------|-------------------------------|--------------------------------|
| `AIOS_TARGET_DISK`    | *required*                    | Target block device            |
| `AIOS_HOSTNAME`       | *required*                    | System hostname                |
| `AIOS_CONFIRM_SKIP`   | *required* (`1`)              | Must be `1` for CI mode        |
| `AIOS_PROFILE`        | `CI_BARE`                     | Security profile               |
| `AIOS_ESP_SIZE_MB`    | `512`                         | ESP size in MiB                |
| `AIOS_BOOT_SIZE_MB`   | `1024`                        | Boot partition size in MiB     |
| `AIOS_MIN_DISK_GB`    | `40`                          | Minimum disk size in GB        |
| `AIOS_SQUASHFS`       | auto-detected                 | Path to squashed rootfs        |
| `AIOS_SKIP_VERITY`    | `0`                           | Set to `1` to skip dm-verity   |
| `AIOS_SKIP_TPM`       | `0`                           | Set to `1` to skip TPM2 seal   |
| `AIOS_SKIP_SELINUX`   | `0`                           | Set to `1` to skip SELinux     |

## Recovery

### Disk Unlock (manual)

If TPM unseal fails (firmware update, hardware change):

1. Boot from the AIOS live ISO/USB
2. Open LUKS manually:
   ```bash
   cryptsetup open /dev/<disk>p3 aios-cryptroot
   ```
3. Mount and repair:
   ```bash
   mount /dev/mapper/aios-cryptroot /mnt
   mount /dev/<disk>p2 /mnt/boot
   # repair as needed
   ```

### Recovery Key

The 24-word recovery key displayed during installation is the **only way**
to unlock the disk if TPM unseal fails. **Store it offline.** Without it,
your data is **irrecoverable**.

The key is also saved at `/etc/aios/recovery-key.txt` on the installed
system — **move this file offline and delete it from the system**.

## TPM PCR Policy

The installer seals the LUKS key against PCRs **0, 1, and 7** (SHA-256 bank):

| PCR  | Measures                                                    |
|------|-------------------------------------------------------------|
| 0    | UEFI firmware code (BIOS/UEFI)                              |
| 1    | UEFI firmware configuration (boot order, setup variables)   |
| 7    | Secure Boot state (PK, KEK, db, dbx certificates)           |

Any change to the measured components will prevent automatic disk unlock
and require the recovery key.

## Security Profiles

| Profile            | LUKS | TPM | SELinux | dm-verity | FIPS |
|--------------------|------|-----|---------|-----------|------|
| SECURE_DEFAULT     | Yes  | Yes | enforcing | Yes    | No   |
| CI_BARE            | Yes  | Yes | enforcing | No     | No   |
| HARDENED           | Yes  | Yes | enforcing | Yes    | Yes  |
| DISCONNECTED       | Yes  | No  | enforcing | No     | No   |

## Troubleshooting

### "Missing required tools"

The installer requires: `lsblk`, `sgdisk`, `mkfs.vfat`, `mkfs.ext4`,
`cryptsetup`, `unsquashfs`, `bootctl`, `blkid`, `systemd-cryptenroll`.
These are pre-installed in the AIOS live environment.

### "Disk too small"

Minimum 40 GB. The rootfs alone requires ~8 GB; the rest is for logs,
capsule state, cognitive model cache, and user data.

### "No TPM device found"

Installation continues without TPM sealing. You will be prompted for a
LUKS passphrase on every boot. The recovery key is still generated and
must be saved.

### "systemd-cryptenroll failed"

Verify the TPM is enabled in UEFI firmware settings. Some firmware
requires a "Physical Presence" action (keyboard confirmation) during
first enrollment.

### Boot failure after install

1. Check Secure Boot is enabled in UEFI firmware
2. Verify the boot order: UEFI OS (AI-OS.NET) should be first
3. If "No bootable device", re-enter UEFI setup and manually add
   `\EFI\systemd\systemd-bootx64.efi` as a boot option

## Files

| File                        | Purpose                                       |
|-----------------------------|-----------------------------------------------|
| `aios-installer.sh`         | Interactive installer for bare-metal machines |
| `aios-quick-install.sh`     | Non-interactive installer for CI/automation   |
| `README.md`                 | This documentation                            |

## Exit Codes (Quick Installer)

| Code | Meaning                     |
|------|-----------------------------|
| 0    | Success                     |
| 1    | Generic error               |
| 2    | Invalid args / missing env  |
| 3    | Disk validation failed      |
| 4    | Partitioning failed         |
| 5    | Encryption failed           |
| 6    | Filesystem error            |
| 7    | Rootfs extraction failed    |
| 8    | Bootloader install failed   |
