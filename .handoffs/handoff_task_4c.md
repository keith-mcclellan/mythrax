# Handoff Contract: Task 4C (Hebbian Synaptic Pruning & Garbage Collection)

## Objective
Update `compactor.rs` to implement garbage collection for low-confidence nodes and Hebbian decay/pruning for semantic edges.

## Files to Modify
- `mythrax-core/src/cognitive/compactor.rs`
- `mythrax-core/tests/test_compactor.rs` (to add tests)

## Requirements
1. **Red (Test)**:
   Write tests verifying:
   - `test_garbage_collect_low_confidence_nodes`: In `tests/test_compactor.rs`, create a `wiki_node` with `metacognitive_confidence = Some(2)` and `updated_at = Utc::now() - 31 days` (mocked or explicitly updated in DB). Run `compact_scope`. Verify the file is moved from its original path to `{vault_root}/archive/{filename}` and the record is deleted from the DB.
   - `test_hebbian_synaptic_pruning`: In `tests/test_compactor.rs`, create a `relates_to` edge between two wiki nodes with a starting weight of `0.105`. Run `compact_scope` (which decays weights). Assert the edge's weight decays (e.g. multiplied by `0.9` to become `0.0945`) and because it is `< 0.1`, the edge is deleted from the database. Also verify that edges with higher weights (e.g., `0.5` becoming `0.45`) are decayed but NOT deleted.

2. **Green (Code)**:
   - In `compactor.rs`'s `compact_scope()`:
     - Perform garbage collection of decayed wiki nodes: select all `wiki_node` where `metacognitive_confidence < 3` and `updated_at < time::now() - 30d`. For each, delete the record from SurrealDB and move the associated vault markdown file to the `{vault_root}/archive/` directory.
     - Perform Hebbian Synaptic Pruning: decay the weight of all `relates_to` edges by multiplying them by `0.9` (e.g., `UPDATE relates_to SET weight = weight * 0.9`). Afterwards, delete all `relates_to` edges where `weight < 0.1`.
