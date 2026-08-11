---
title: "Red Team Architecture Brief: Transcript Tool Sequence Sliding Window Insufficient for Recursion Prevention"
labels: [architecture-review, adversarial]
---

### Finding
Tool mining uses a 1,000-element `VecDeque` sliding window cap, discarding older events.

### Current Assumption
Discarding older events prevents memory growth and implicitly stops unbounded recursion.

### Attack Scenario
An agent gets stuck in a rapid failure loop (e.g., retrying an API call that always fails). It generates 1,000 events within seconds. The window cap prevents OOM, but the agent never stops executing, consuming tokens and API quotas infinitely.

### Blast Radius
Resource exhaustion (API token burn, compute cycle monopolization) and failure to enforce scope boundaries if the loop triggers side-effecting tools.

### Recommended Structural Change
Implement a stateful circuit breaker. Track the frequency of identical tool invocations or error states within a given time window. Suspend the agent or require human intervention if a threshold is breached.
