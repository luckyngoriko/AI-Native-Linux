# L10 — Distribution, Ecosystem, Marketplace

Status: `PARTIAL` (`01_repository_model.md` is `CONTRACT`; the marketplace UX and external-integration sub-specs stay `SHELL`)

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
| `02_marketplace.md`           | Publisher onboarding; capability declaration; review workflow | `SHELL`    | —     |
| `03_external_integrations.md` | Bridges to Flathub, OCI registries, distro repos              | `SHELL`    | —     |

## See also

- [Rev.2 Master Index](../00_MASTER_INDEX.md)
