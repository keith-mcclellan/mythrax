# ⚡ Bolt: Replace .clone().clear() with std::mem::replace in string splits

## 💡 What
Replaced `.clone()` followed by `.clear()` with `std::mem::replace` when pushing string chunks into a vector in `src/vault/ingestion.rs`. Combined with `String::with_capacity()`, this avoids unnecessary heap allocations and reallocations when processing ingested texts.

## 🎯 Why
In Rust, `.clone()` forces an unnecessary heap allocation of the string content. When `.clear()` is called immediately after, it doesn't just empty the string; in some patterns (like variable reassignment or when `std::mem::take` replaces it with a default), it can drop the capacity. By using `std::mem::replace` combined with `String::with_capacity()`, we transfer ownership of the pre-allocated string into the array directly and ensure the next iteration's string maintains proper capacity.

## 📊 Impact
Reduces heap allocations per document chunk by 1, and avoids re-allocating new capacities when handling very large input texts. Expected to make text ingestion measurably faster and use less memory overhead.

## 🔬 Measurement
Run `cargo build --release` and `MYTHRAX_TEST_MOCK=1 cargo test vault::ingestion` to verify the logic correctly creates and groups the chunks as before. Benchmark text ingestion speeds before and after to observe fewer allocations per document.
