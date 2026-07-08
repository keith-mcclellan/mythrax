use std::path::Path;
use mythrax_core::embeddings::load_embedding_cache_from_disk;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cache_path = Path::new("embedding_cache.bin");
    load_embedding_cache_from_disk(cache_path)?;
    
    // We want to access the global cache map to inspect the keys
    // In src/embeddings.rs:
    // pub static GLOBAL_EMBEDDING_CACHE: LazyLock<DashMap<String, Vec<f32>>> = ...
    // Since it's public, we can access it!
    let cache = &mythrax_core::embeddings::GLOBAL_EMBEDDING_CACHE;
    println!("Total keys in cache: {}", cache.len());
    println!("First 20 keys:");
    let mut count = 0;
    for entry in cache.iter() {
        if count >= 20 {
            break;
        }
        println!("  - {:?}", entry.key());
        count += 1;
    }
    
    // Also check if any key starts with "search_query"
    let mut search_query_count = 0;
    for entry in cache.iter() {
        if entry.key().starts_with("search_query") {
            search_query_count += 1;
            if search_query_count <= 5 {
                println!("  Found search query key: {:?}", entry.key());
            }
        }
    }
    println!("Total search query keys: {}", search_query_count);
    
    Ok(())
}
