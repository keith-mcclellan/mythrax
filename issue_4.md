# High: Vulnerable Dependencies in Cargo.lock

## Description
A `cargo audit` scan revealed multiple dependencies in the `mythrax-core` workspace with known high-severity vulnerabilities. These can potentially be exploited to cause denial of service or other security issues.

## Findings
- `lopdf v0.38.0`: Stack overflow via deeply nested PDF objects (RUSTSEC-2026-0187, Severity: 7.5 High).
- `quinn-proto v0.11.14`: Remote memory exhaustion from unbounded out-of-order stream reassembly (RUSTSEC-2026-0185, Severity: 7.5 High).

## Remediation
Update the versions of the vulnerable dependencies in `Cargo.toml` and run `cargo update` to regenerate `Cargo.lock`.
- Upgrade `lopdf` to `>=0.42.0`
- Upgrade `quinn-proto` to `>=0.11.15`
