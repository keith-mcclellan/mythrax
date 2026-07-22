# 2. In-Process GPU Inference Tightly Coupled with Core Daemon

**Tags**: `architecture-review`, `adversarial`

**Finding**: 2. In-Process GPU Inference Tightly Coupled with Core Daemon

**Current Assumption**: Loading MLX or ONNX models natively into the Rust process memory on Apple Silicon is the most efficient way to serve local inferences, and splitting GPU semaphores for metal/embedding processes prevents deadlocks.

**Attack Scenario**: If a model execution panics due to out-of-memory (OOM) exceptions (e.g. from an adversarial context-window overflow attack) or encounters a driver-level fault, it crashes the entire Mythrax daemon. The single-process architecture guarantees that any GPU fault terminates API routing, memory persistence, and orchestration simultaneously.

**Blast Radius**: Total node outage. A failure in the inference tier brings down the API Gateway and Storage tier, halting all agent tasks until manual recovery or supervisor restarts the process.

**Recommended Structural Change**: Decouple model execution into isolated sidecar processes or remote GPU worker nodes communicating via gRPC or HTTP. The core daemon should only manage routing and orchestration, gracefully falling back or returning standard errors on worker node failure without crashing.
