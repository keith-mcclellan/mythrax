# ⚡ Bolt: Eliminate heap allocations in vault ingestion text chunking

## 💡 What
Replaced `.clone()` + `.clear()` and string overwrites with `std::mem::replace` in the `split_by_chars` and `group_sub_chunks` functions within `mythrax-core/src/vault/ingestion.rs`.

## 🎯 Why
During document ingestion, text is heavily processed and split into chunks. The previous implementation utilized `.clone()` to push string buffers into a vector, subsequently clearing or overwriting the original buffer. This incurred an unnecessary O(N) heap allocation, memory copy, and subsequent deallocation per chunk. By leveraging `std::mem::replace`, ownership of the buffer is directly moved to the vector (an O(1) operation), completely avoiding the deep copy.

## 📊 Impact
* Eliminates one unnecessary string clone/heap allocation per generated chunk.
* Decreases memory fragmentation during heavy document parsing.
* Improves parsing speed linearly with document size (O(N) copy reduced to O(1) pointer move).

## 🔬 Measurement
Run `MYTHRAX_TEST_MOCK=1 cargo test vault::ingestion::tests` to verify logical equivalency. Measure memory profile differences on large markdown corpus ingestion to verify decreased heap allocation rate.
