---
title: "VRAM Tracking Desynchronization Risk"
labels: ["architecture-review", "adversarial"]
---

## Finding
VRAM tracking state (`active_tier` and `last_weak_ref`) must be manually updated on cache hits.

## Current Assumption
Manual state updates will consistently synchronize VRAM usage tracking with actual hardware utilization.

## Attack Scenario
Adversarial inputs or concurrent requests can exploit race conditions in model loading or cache hits where the tracking state is updated incorrectly or missed. This leads to desynchronization between tracked and actual VRAM, causing out-of-memory crashes or aggressive, unnecessary model evictions.

## Blast Radius
Model broker failure, OOM crashes, and denial of service for inference requests.

## Recommended Structural Change
Implement an automated, hardware-aware VRAM monitoring abstraction that directly queries the GPU for memory usage, rather than relying on manual, application-level tracking state updates.

> Note: This issue requires a documented architectural decision record (ADR) response before it can be closed.
