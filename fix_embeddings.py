import os
import re

def fix_embeddings():
    with open('/Users/keith/Documents/mythrax/mythrax-core/src/embeddings.rs', 'r') as f:
        content = f.read()

    # Re-patch the mock embed method with normalization.
    # Since we already ran the script and replaced the method, we'll look for the existing mock block and replace it.

    mock_embed_search = r"""        if self\.is_mock \{\s*use std::collections::hash_map::DefaultHasher;\s*use std::hash::\{Hash, Hasher\};\s*let mut hasher = DefaultHasher::new\(\);\s*text\.hash\(&mut hasher\);\s*let seed = hasher\.finish\(\);\s*let mut vec = vec!\[0\.0; 768\];\s*for i in 0\.\.768 \{\s*let val = \(\(\(seed \^ \(i as u64\)\) % 1000\) as f32 / 5000\.0\) - 0\.1;\s*vec\[i\] = val;\s*\}\s*return Ok\(vec\);\s*\}"""

    mock_embed_replace = r"""        if self.is_mock {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            text.hash(&mut hasher);
            let seed = hasher.finish();
            let mut vec = vec![0.0; 768];
            let mut sum_sq = 0.0;
            for i in 0..768 {
                let val = (((seed ^ (i as u64)) % 1000) as f32 / 5000.0) - 0.1;
                vec[i] = val;
                sum_sq += val * val;
            }
            let norm = sum_sq.sqrt();
            if norm > 0.0 {
                for v in &mut vec {
                    *v /= norm;
                }
            }
            return Ok(vec);
        }"""

    content = re.sub(mock_embed_search, mock_embed_replace, content)

    with open('/Users/keith/Documents/mythrax/mythrax-core/src/embeddings.rs', 'w') as f:
        f.write(content)

if __name__ == '__main__':
    fix_embeddings()
