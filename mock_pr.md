# ⚡ Bolt: Reduce string heap allocations during chunking

## 💡 What
Optimized `split_by_chars` and `group_sub_chunks` string processing routines in `src/vault/ingestion.rs` by replacing `String::new()` and `.clone()` followed by `.clear()` with pre-allocation (`String::with_capacity()`) and `std::mem::replace`.

## 🎯 Why
Inside tight chunking loops, `.clone()` followed by `.clear()` results in unnecessary heap copies of intermediate strings. Furthermore, resetting an active accumulation buffer without explicit pre-allocation causes frequent reallocations as characters are appended. This change reduces heap activity when ingesting large documents into the vault.

## 📊 Impact
Expected to noticeably decrease the number of heap allocations and time spent in memory management during ingestion of large Markdown files, making the chunking phase computationally lighter.

## 🔬 Measurement
This optimization can be verified by running the vault ingestion test suite, confirming all chunks remain identical to the previous implementation while leveraging `std::mem::replace` semantics. Tests pass locally.

Fixes #performance-heap
