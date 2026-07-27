## 2024-05-24 - Hardcoded Fallback Auth Token
**Vulnerability:** The API Gateway fallback authentication token was hardcoded as "fallback-err-token", introducing a single point of failure and systemic compromise risk if token creation fails.
**Learning:** Using a static string fallback for auth tokens bypasses dynamic generation and exposes the system to unauthorized access when edge cases fail.
**Prevention:** Always propagate errors for critical security operations like token generation using `Result` rather than silently swallowing errors and falling back to weak static strings.