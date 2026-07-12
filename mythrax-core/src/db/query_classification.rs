use crate::db::backend::{SurrealBackend, record_key_to_string, unescape_id_part};
use anyhow::{Result, Context};
use std::sync::OnceLock;
use regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum QueryCategory {
    Preference,
    User,
    Temporal,
    Default,
}

impl QueryCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            QueryCategory::Preference => "preference",
            QueryCategory::User => "user",
            QueryCategory::Temporal => "temporal",
            QueryCategory::Default => "default",
        }
    }
}

pub fn get_decay_factor(category: QueryCategory, delta_t_secs: f64, sigma_hours: f64, decay_floor: f32) -> f32 {
    if category == QueryCategory::Preference || category == QueryCategory::User {
        1.0f32
    } else {
        let delta_t_hours = (delta_t_secs.max(0.0) / 3600.0) as f32;
        let sigma = sigma_hours as f32;
        if sigma <= 0.0 {
            return 1.0f32;
        }
        let decay = (-0.5f32 * (delta_t_hours / sigma).powi(2)).exp();
        decay.max(decay_floor)
    }
}

pub fn split_temporal_query(query: &str) -> (String, String) {
    static CLEANING_RE: OnceLock<Regex> = OnceLock::new();
    let cleaning_re = CLEANING_RE.get_or_init(|| {
        Regex::new(r"\b(before|preceding|previously|prior|earlier|ago|last|after|following|subsequently|later|next|recent|recently|latest|newest|today|now|week|weeks|month|months|year|years|day|days|hour|hours|minute|minutes|second|seconds)\b").unwrap()
    });
    let cleaned = cleaning_re.replace_all(query, "").to_string();
    let cleaned_query = cleaned.split_whitespace().collect::<Vec<&str>>().join(" ");
    (cleaned_query, query.to_string())
}

pub fn normalize_spelling(word: &str) -> &str {
    match word {
        "favourite" | "favourites" => "favorite",
        "appt" | "appts" => "appointment",
        "mtg" | "mtgs" => "meeting",
        "grad" => "graduation",
        "lodging" | "lodgings" => "hotel",
        "staying" | "stays" => "stay",
        other => other,
    }
}

pub fn expand_synonyms(word: &str) -> &str {
    match word {
        "motel" | "hostel" | "cabin" | "lodge" | "resort" | "inn" | "accommodation" => "hotel",
        "airline" | "jet" | "plane" | "airplane" => "flight",
        "diner" | "cafe" | "bistro" | "eatery" | "pub" => "restaurant",
        "profession" | "occupation" | "vocation" | "work" => "job",
        "employer" | "company" | "corporation" | "firm" => "work",
        "school" | "college" | "university" | "academy" => "degree",
        "spouse" | "wife" | "husband" | "partner" => "spouse",
        "buddy" | "pal" | "colleague" => "friend",
        other => other,
    }
}

pub fn classify_query(query: &str) -> QueryCategory {
    let lower = query.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().map(|w| w.trim_matches(|c: char| !c.is_alphanumeric())).collect();
    
    let preference = ["prefer", "favorite", "favourite", "like", "dislike", "love", "hate", "choice", "opinion", "preferred", "choose", "chose", "select", "book", "vendor", "hotel", "restaurant", "flight", "airline", "stay"];
    let user = ["my", "me", "i", "myself", "profile", "age", "name", "career", "degree", "spouse", "husband", "wife", "work", "job", "employer", "friend"];
    let temporal = ["before", "after", "previously", "prior", "earlier", "ago", "last", "later", "next", "recent", "recently", "today", "now", "yesterday", "tomorrow", "appt", "appts", "mtg", "mtgs", "meeting", "meetings", "appointment", "appointments"];

    let has_temporal = words.iter().any(|w| temporal.contains(w));
    let has_preference = words.iter().any(|w| preference.contains(w));
    let has_user = words.iter().any(|w| user.contains(w)) 
        || lower.contains("who am i") 
        || lower.contains("about me");

    if has_temporal {
        QueryCategory::Temporal
    } else if has_preference {
        QueryCategory::Preference
    } else if has_user {
        QueryCategory::User
    } else {
        QueryCategory::Default
    }
}

impl SurrealBackend {
    pub async fn classify_query_db(&self, query: &str) -> QueryCategory {
        let lower = query.to_lowercase();
        if lower.contains("who am i") || lower.contains("about me") {
            return QueryCategory::User;
        }

        let sql = "
            LET $tokens = search::analyze('snowball_en', $query);
            SELECT VALUE category FROM search_keyword WHERE search::analyze('snowball_en', word)[0] IN $tokens;
        ";
        match self.db.query(sql).bind(("query", query)).await {
            Ok(mut res) => {
                let categories: Vec<String> = res.take(1).unwrap_or_default();
                if categories.iter().any(|c| c == "Temporal") {
                    QueryCategory::Temporal
                } else if categories.iter().any(|c| c == "Preference") {
                    QueryCategory::Preference
                } else if categories.iter().any(|c| c == "User") {
                    QueryCategory::User
                } else {
                    QueryCategory::Default
                }
            }
            Err(_) => QueryCategory::Default,
        }
    }
}
