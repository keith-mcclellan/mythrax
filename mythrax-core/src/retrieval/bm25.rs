use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct OkapiBM25 {
    doc_lengths: HashMap<String, usize>,
    doc_term_freqs: HashMap<String, HashMap<String, usize>>,
    df: HashMap<String, usize>,
    n: usize,
    avg_dl: f32,
    k1: f32,
    b: f32,
}

impl OkapiBM25 {
    pub fn new(corpus: &[(String, String)]) -> Self {
        let mut doc_lengths = HashMap::new();
        let mut doc_term_freqs = HashMap::new();
        let mut df = HashMap::new();
        let mut total_len = 0;

        for (doc_id, content) in corpus {
            let tokens = tokenize(content);
            let doc_len = tokens.len();
            doc_lengths.insert(doc_id.clone(), doc_len);
            total_len += doc_len;

            let mut term_freqs = HashMap::new();
            for token in tokens {
                *term_freqs.entry(token.clone()).or_insert(0) += 1;
            }

            for term in term_freqs.keys() {
                *df.entry(term.clone()).or_insert(0) += 1;
            }

            doc_term_freqs.insert(doc_id.clone(), term_freqs);
        }

        let n = corpus.len();
        let avg_dl = if n > 0 {
            total_len as f32 / n as f32
        } else {
            0.0
        };

        Self {
            doc_lengths,
            doc_term_freqs,
            df,
            n,
            avg_dl,
            k1: 1.5,
            b: 0.75,
        }
    }

    pub fn with_global_stats(
        corpus: &[(String, String)],
        global_df: HashMap<String, usize>,
        global_n: usize,
        global_avg_dl: f32,
    ) -> Self {
        let mut doc_lengths = HashMap::new();
        let mut doc_term_freqs = HashMap::new();

        for (doc_id, content) in corpus {
            let tokens = tokenize(content);
            let doc_len = tokens.len();
            doc_lengths.insert(doc_id.clone(), doc_len);

            let mut term_freqs = HashMap::new();
            for token in tokens {
                *term_freqs.entry(token.clone()).or_insert(0) += 1;
            }
            doc_term_freqs.insert(doc_id.clone(), term_freqs);
        }

        Self {
            doc_lengths,
            doc_term_freqs,
            df: global_df,
            n: global_n,
            avg_dl: global_avg_dl,
            k1: 1.5,
            b: 0.75,
        }
    }

    pub fn with_k1(mut self, k1: f32) -> Self {
        self.k1 = k1;
        self
    }

    pub fn with_b(mut self, b: f32) -> Self {
        self.b = b;
        self
    }

    pub fn score(&self, query: &str) -> Vec<(String, f32)> {
        let query_tokens = tokenize(query);
        let mut results = Vec::new();

        if query_tokens.is_empty() || self.n == 0 {
            for doc_id in self.doc_lengths.keys() {
                results.push((doc_id.clone(), 0.0));
            }
            return results;
        }

        for (doc_id, doc_len) in &self.doc_lengths {
            let term_freqs = self.doc_term_freqs.get(doc_id).unwrap();
            let mut score = 0.0;

            for term in &query_tokens {
                let tf = *term_freqs.get(term).unwrap_or(&0) as f32;
                let df_t = *self.df.get(term).unwrap_or(&0);
                
                // IDF formula: log((N - df + 0.5)/(df + 0.5) + 1.0)
                let idf = (((self.n as f32 - df_t as f32 + 0.5) / (df_t as f32 + 0.5)) + 1.0).ln();
                
                // Okapi BM25 formula
                let numerator = tf * (self.k1 + 1.0);
                let dl_ratio = if self.avg_dl > 0.0 {
                    *doc_len as f32 / self.avg_dl
                } else {
                    1.0
                };
                let denominator = tf + self.k1 * (1.0 - self.b + self.b * dl_ratio);
                
                score += idf * (numerator / denominator);
            }

            results.push((doc_id.clone(), score));
        }

        results
    }

    pub fn score_normalized(&self, query: &str) -> Vec<(String, f32)> {
        let raw = self.score(query);
        if raw.is_empty() {
            return vec![];
        }

        let mut min_val = f32::MAX;
        let mut max_val = f32::MIN;
        for (_, s) in &raw {
            if *s < min_val { min_val = *s; }
            if *s > max_val { max_val = *s; }
        }

        let denom = max_val - min_val;
        raw.into_iter()
            .map(|(id, s)| {
                let norm = if denom > 1e-6 {
                    (s - min_val) / denom
                } else if max_val > 1e-6 {
                    1.0
                } else {
                    0.0
                };
                (id, norm)
            })
            .collect()
    }
}

pub fn stem(word: &str) -> String {
    if word.len() <= 3 || word.contains('-') {
        return word.to_string();
    }
    if word.ends_with("ing") {
        word[..word.len() - 3].to_string()
    } else if word.ends_with("ed") {
        word[..word.len() - 2].to_string()
    } else if word.ends_with("sses") {
        word[..word.len() - 2].to_string()
    } else if word.ends_with("ies") {
        let stem_str = &word[..word.len() - 3];
        format!("{}i", stem_str)
    } else if word.ends_with("es") {
        word[..word.len() - 2].to_string()
    } else if word.ends_with('s') && !word.ends_with("ss") {
        word[..word.len() - 1].to_string()
    } else {
        word.to_string()
    }
}

pub fn tokenize(text: &str) -> Vec<String> {
    let lowercase = text.to_lowercase();
    lowercase
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !is_stop_word(s))
        .map(|s| stem(s))
        .collect()
}

fn is_stop_word(w: &str) -> bool {
    matches!(
        w,
        "a" | "about"
            | "above"
            | "after"
            | "again"
            | "against"
            | "all"
            | "am"
            | "an"
            | "and"
            | "any"
            | "are"
            | "as"
            | "at"
            | "be"
            | "because"
            | "been"
            | "before"
            | "being"
            | "below"
            | "between"
            | "both"
            | "but"
            | "by"
            | "can"
            | "did"
            | "do"
            | "does"
            | "doing"
            | "down"
            | "during"
            | "each"
            | "few"
            | "for"
            | "from"
            | "further"
            | "had"
            | "has"
            | "have"
            | "having"
            | "he"
            | "her"
            | "here"
            | "hers"
            | "him"
            | "himself"
            | "his"
            | "how"
            | "i"
            | "if"
            | "in"
            | "into"
            | "is"
            | "it"
            | "its"
            | "itself"
            | "me"
            | "more"
            | "most"
            | "my"
            | "myself"
            | "no"
            | "nor"
            | "not"
            | "of"
            | "off"
            | "on"
            | "once"
            | "only"
            | "or"
            | "other"
            | "our"
            | "ours"
            | "ourselves"
            | "out"
            | "over"
            | "own"
            | "same"
            | "she"
            | "should"
            | "so"
            | "some"
            | "such"
            | "than"
            | "that"
            | "the"
            | "their"
            | "theirs"
            | "them"
            | "themselves"
            | "then"
            | "there"
            | "these"
            | "they"
            | "this"
            | "those"
            | "through"
            | "to"
            | "too"
            | "under"
            | "until"
            | "up"
            | "very"
            | "was"
            | "we"
            | "were"
            | "what"
            | "when"
            | "where"
            | "which"
            | "while"
            | "who"
            | "whom"
            | "why"
            | "with"
            | "you"
            | "your"
            | "yours"
            | "yourself"
            | "yourselves"
    )
}
