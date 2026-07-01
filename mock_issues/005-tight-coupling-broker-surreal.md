---
title: "Tight Coupling Between Model Broker and Database Layer"
tags: [architecture-review, adversarial]
---

# Finding: Tight Coupling Between Model Broker and Database Layer

## Current Assumption
It is acceptable for the database backend (`db/backend.rs`) to directly invoke the embedding model to generate vectors, falling back to zero-vectors if the model fails.

## Attack Scenario
Under high load, the embedding model queue fills up or the model crashes (OOM). The database insertion path blocks entirely or silently injects zero-vectors (which corrupt cosine similarity search), bringing down the memory ingestion pipeline.

## Blast Radius
Data Corruption and Component Deadlock. The database layer cannot function independently of the inference engine. A failure in the GPU/MLX layer cascades instantly into a database failure, violating separation of concerns.

## Recommended Structural Change
Decouple the vector generation from the database insertion via an asynchronous event bus or queue. The database should only store raw text; a separate background worker should poll for un-embedded records, request vectors from the model broker, and update the records, ensuring DB availability even if the GPU is offline.

*This issue requires an ADR response to close.*
