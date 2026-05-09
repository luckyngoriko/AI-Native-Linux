# L8 — Network, Hardware, Devices

Status: `PARTIAL` (`02_network_policy.md` and `05_gpu_resource_model.md` are `CONTRACT`; the three remaining sub-specs stay `SHELL`)

## Responsibility

AIOS-HDM (Hardware and Driver Manager) owns the hardware graph: CPU, GPU, storage, network adapters, audio, Bluetooth, USB/Thunderbolt, printers, sensors, firmware update paths, removable device policy. AIOS-NP (Network Policy Manager) enforces local-first network posture with per-app outbound policy and explicit approvals for LAN/public exposure.

## Layer invariants (from Rev.1 §6, §18)

- Default: deny public exposure; services default to localhost-only.
- Listening on `0.0.0.0` requires approval.
- Opening firewall ports or DNS/VPN changes are logged.
- Per-app outbound access is declared explicitly.
- Firmware update paths must be classified by trust before enabling.

## Dependencies

May depend on: L0, L1, L2, L3, L4.

## Planned sub-specs

| File                       | Topic                                                                                                                                                      | Status            |
| -------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------- |
| `01_hardware_graph.md`     | Device detection, identification, classification, lifecycle                                                                                                | `SHELL`           |
| `02_network_policy.md`     | Default-deny posture; per-app outbound; firewall integration                                                                                               | `CONTRACT` (S8.1) |
| `03_dns_vpn_management.md` | Resolver backend, WireGuard, mDNS/Avahi gating                                                                                                             | `SHELL`           |
| `04_firmware_trust.md`     | Firmware update classification; signed update paths                                                                                                        | `SHELL`           |
| `05_gpu_resource_model.md` | Device topology, VRAM accounting, dmabuf brokering, closed `GpuCapabilityClass` enum, per-group `VkDevice` partitioning, hardware capability-lie detection | `CONTRACT` (S8.2) |

## Optional capability provider: ProxGuard

When installed as an AIOS app, ProxGuard may provide network-facing capabilities through L8 policy gates:

- `proxguard.dns.plan`
- `proxguard.dns.apply`
- `proxguard.gateway.route`
- `proxguard.gateway.status`
- `certificate.challenge.prepare`

L8 remains authoritative for network exposure. ProxGuard may propose DNS, gateway, certificate, and route changes, but public exposure, firewall changes, provider credentials, and certificate challenge updates require AIOS policy approval and evidence.

## See also

- [Rev.1 §18 — Hardware and Network](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [ProxGuard Reference Model](../XX_Cross_Cutting/02_proxguard_reference_model.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
