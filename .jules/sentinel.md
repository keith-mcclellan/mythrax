## 2024-05-09 - [Panic vulnerability on malformed relations]
**Vulnerability:** Calling `.unwrap()` on `rel.get("from_str")` and `.as_str()` in `mythrax-core/src/db/crud_operations.rs` crashes the application if the input is malformed, leading to a potential DoS vulnerability.
**Learning:** External or unchecked data (like JSON values representing relations) must always be safely extracted. Silently unwrapping creates panic boundaries.
**Prevention:** Always use safe extraction methods (e.g., `if let`, `match`, or returning errors) for external inputs rather than `.unwrap()`.
