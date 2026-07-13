pub mod markdown;
pub mod organization;
pub mod watcher;

#[allow(unused_imports)]
pub use markdown::{parse_frontmatter, extract_plain_text};
#[allow(unused_imports)]
pub use organization::organize_file;
pub mod ingestion;
#[allow(unused_imports)]
pub use watcher::{WatchIgnoreList, start_watching, save_episode_bidirectional};
pub mod operations;
pub mod distillation;

