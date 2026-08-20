# Red Team Architecture Brief

## Finding
Architectural Coupling: Model Router and Streaming Compaction

## Current Assumption
The Compactor Sweep Service and Model Router (managing local 0.5B models and external 35B models) must be tightly integrated to perform daily dreaming compaction and RAPTOR synthesis.

## Attack Scenario
To upgrade the clustering algorithm or migrate the RAPTOR synthesis to a dedicated fleet of specialized models (e.g., dedicated summarizing agents), developers must modify the core Mythrax daemon. An adversary exploiting a vulnerability in the `completions_proxy_handler` can crash the shared tokio runtime, bringing down both inference routing and background memory compaction.

## Blast Radius
High operational friction for scaling. A fault in model routing crashes the background compaction pipelines.

## Recommended Structural Change
Extract the Model Router and Compaction Pipeline into separate microservices or independent processes communicating via message queues (e.g., gRPC or Kafka). This allows independent scaling, testing, and deployment of inference and memory synthesis.
