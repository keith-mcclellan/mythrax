---
labels: bug, agent-found
---

# Logic Bug & Inefficient Eviction Loop in `evict_if_needed`

## File and Line
`mythrax-core/src/cognitive/memory_os.rs:43`

## Minimal Reproducible Scenario
The `PagingManager::evict_if_needed` function loops through a `VecDeque` using an index:
```rust
for i in 0..self.lru_queue.len() {
    let id = &self.lru_queue[i];
    if let Some(info) = self.active_nodes.get(id) {
        if !info.pinned {
            let id_to_evict = self.lru_queue.remove(i).unwrap();
            self.active_nodes.remove(&id_to_evict);
            // ...
        }
    }
}
```
Calling `remove(i)` on a `VecDeque` requires shifting elements, leading to $O(N)$ removal time. Compounded by resetting the loop for each eviction, this becomes an inefficient $O(N^2 \cdot k)$ loop. Additionally, using `unwrap()` after `remove(i)` represents a structural panic risk if state invariant assumptions ever change (e.g., unexpected concurrent mutations).

## Severity
Medium (Performance degradation and panic risk)

## Suggested Fix
Refactor the loop to conditionally retain elements and collect those evicted:
```rust
self.lru_queue.retain(|id| {
    if self.active_nodes.len() <= self.capacity {
        return true;
    }
    if let Some(info) = self.active_nodes.get(id) {
        if !info.pinned {
            self.active_nodes.remove(id);
            evicted.push(id.clone());
            return false;
        }
    }
    true
});
```
