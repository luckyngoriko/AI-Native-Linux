# Security Policy

AI-Native Linux is currently specification-first. There is no production runtime yet, but the repository defines security-sensitive operating system behavior.

## In Scope

Please report issues involving:

- unsafe authority flow in the specification
- missing policy checks for privileged actions
- direct AI-to-shell execution paths
- weak sandbox, vault, identity, or recovery boundaries
- filesystem authority escalation
- service, package, DNS, firewall, gateway, or device control gaps
- evidence log bypasses or unverifiable execution paths

## Out of Scope

The current repository does not ship a runtime, daemon, package manager, kernel module, installer, or network service. Vulnerability reports against non-existent implementation code are out of scope until implementation begins.

## Reporting

Use a private GitHub security advisory when available. If GitHub advisories are not enabled, open a minimal public issue that says a security-sensitive design issue exists without publishing exploit details.

Do not include real credentials, private keys, production hostnames, or third-party secrets in reports.

## Response Model

Security reports should be mapped to the affected Rev.2 layer and should describe:

- affected contract or file
- required authority boundary
- current weakness
- proposed safer behavior
- verification or evidence requirement

The project may reject reports that rely on granting AI direct shell/root authority, because that execution model is explicitly outside the architecture.
