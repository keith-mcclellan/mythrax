# Critical: Hardcoded Fallback Auth Token

## Description
The application uses a hardcoded fallback authentication token (`"secret-token"`) in test setups. A commit analysis of git history revealed a prior finding (HARD-001) that showed the token was also previously hardcoded in production paths like `daemon.rs`, `main.rs`, and `db/backend.rs`. This provides a known bypass for authentication if the token file is missing or improperly initialized.

## Locations
- `src/api.rs`: Hardcoded `auth_token: "secret-token".to_string()` in `ApiState` initialization for tests.
- `src/api.rs`: Hardcoded `.header("X-Mythrax-Token", "secret-token")` in multiple test requests.
- git history: Previous commits (e.g. HARD-001) used `"secret-token"` in production codepaths as well.

## Remediation
Remove all instances of the hardcoded fallback token. The application must generate a cryptographically random token (e.g., UUID v4) and write it to `~/.mythrax/token` with `0600` permissions if one does not exist. Tests should mock the token generation or read a dynamically generated token, rather than relying on a static string.
