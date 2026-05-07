# L10 — Distribution, Ecosystem, Marketplace

Status: `SHELL`

## Responsibility

Publishing, repositories, marketplace, third-party integrations, package signing, trust roots, ecosystem governance.

## Layer invariants (from Rev.1 §6)

- Marketplace governance must support per-publisher trust.
- Package fetches must verify signatures before unpacking.
- Marketplace listings must declare requested capabilities upfront for policy review.

## Dependencies

May depend on: L0, L1, L2, L3, L4, L5, L6, L7, L8, L9.

## Planned sub-specs

| File                          | Topic                                                         | Status  |
| ----------------------------- | ------------------------------------------------------------- | ------- |
| `01_repository_model.md`      | Repository structure; signing; trust roots; mirror semantics  | `SHELL` |
| `02_marketplace.md`           | Publisher onboarding; capability declaration; review workflow | `SHELL` |
| `03_external_integrations.md` | Bridges to Flathub, OCI registries, distro repos              | `SHELL` |

## Status

This layer is `DEFERRED` for early phases — none of the Phase 0–3 sub-specs touch L10. The folder exists for completeness and future work.

## See also

- [Rev.2 Master Index](../00_MASTER_INDEX.md)
