---
title: "Mutex Poisoning in Size-Rolling Logger"
labels: ["architecture-review", "adversarial"]
---

Finding -> Current Assumption -> Attack Scenario -> Blast Radius -> Recommended Structural Change

**Finding**: log_writer.rs uses self.inner.lock().unwrap().
**Current Assumption**: Thread panics will not occur while holding the logger lock.
**Attack Scenario**: A malformed payload causes a thread to panic during a log write. The mutex is poisoned.
**Blast Radius**: Complete daemon crash (DoS) on the next log write attempt, with no graceful degradation path.
**Recommended Structural Change**: Use a lock-free asynchronous logging channel (e.g., tracing-appender) to decouple logging from request threads.

**Note:** Never close this issue without a documented architectural decision record (ADR) response.
