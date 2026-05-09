# L10 — Distribution, Ecosystem, Marketplace

Status: `PARTIAL` (all 3 sub-specs `CONTRACT`; layer headline remains PARTIAL until E2+ implementation evidence)

## Responsibility

Publishing, repositories, marketplace, third-party integrations, package signing, trust roots, ecosystem governance.

## Layer invariants (from Rev.1 §6)

- Marketplace governance must support per-publisher trust.
- Package fetches must verify signatures before unpacking.
- Marketplace listings must declare requested capabilities upfront for policy review.

## Dependencies

May depend on: L0, L1, L2, L3, L4, L5, L6, L7, L8, L9.

## Planned sub-specs

| File                          | Topic                                                         | Status     | Phase |
| ----------------------------- | ------------------------------------------------------------- | ---------- | ----- |
| `01_repository_model.md`      | Repository structure; signing; trust roots; mirror semantics  | `CONTRACT` | S11.1 |
| `02_marketplace.md`           | Publisher onboarding; capability declaration; review workflow | `CONTRACT` | S11.2 |
| `03_external_integrations.md` | Bridges to Flathub, OCI registries, distro repos              | `CONTRACT` | S11.3 |

## See also

- [Rev.2 Master Index](../00_MASTER_INDEX.md)
