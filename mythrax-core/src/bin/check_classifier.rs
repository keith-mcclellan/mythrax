use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::db::backend::QueryCategory;

fn normalize_spelling(word: &str) -> &str {
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

fn expand_synonyms(word: &str) -> &str {
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

fn is_temporal_vocab(stemmed: &str) -> bool {
    matches!(
        stemmed,
        "befor"
            | "after"
            | "previous"
            | "prior"
            | "earli"
            | "ago"
            | "last"
            | "later"
            | "next"
            | "recent"
            | "today"
            | "now"
            | "first"
            | "second"
            | "third"
            | "date"
            | "time"
            | "when"
            | "year"
            | "month"
            | "week"
            | "day"
            | "hour"
            | "calendar"
            | "schedul"
            | "meet"
            | "appoint"
            | "between"
            | "pass"
            | "durat"
            | "spend"
            | "spent"
            | "sunday"
            | "monday"
            | "tuesday"
            | "wednesday"
            | "thursday"
            | "friday"
            | "saturday"
            | "yesterday"
            | "tomorrow"
            | "confer"
            | "dure"
            | "past"
            | "histori"
            | "timelin"
    )
}

fn is_user_vocab(stemmed: &str) -> bool {
    matches!(
        stemmed,
        "name"
            | "age"
            | "profil"
            | "job"
            | "career"
            | "degre"
            | "graduat"
            | "work"
            | "email"
            | "phone"
            | "backgroun"
            | "address"
            | "famili"
            | "friend"
            | "spous"
            | "wife"
            | "husband"
            | "employ"
            | "cat"
            | "dog"
            | "pet"
            | "hamster"
            | "grandma"
            | "grandpa"
            | "mother"
            | "father"
            | "parent"
            | "brother"
            | "sister"
            | "sibling"
            | "son"
            | "daughter"
            | "child"
            | "commut"
            | "live"
            | "resid"
            | "born"
            | "school"
            | "birth"
            | "hometown"
            | "car"
            | "vehicl"
            | "sneaker"
            | "postcard"
            | "collect"
    )
}

fn is_preference_vocab(stemmed: &str) -> bool {
    matches!(
        stemmed,
        "prefer"
            | "favorit"
            | "favourit"
            | "like"
            | "dislik"
            | "love"
            | "hate"
            | "choic"
            | "opinion"
            | "choos"
            | "chose"
            | "select"
            | "book"
            | "vendor"
            | "hotel"
            | "restaur"
            | "flight"
            | "airlin"
            | "stay"
            | "recommend"
            | "suggest"
            | "accommod"
    )
}

fn original_classify_query(query: &str) -> QueryCategory {
    let tokens: Vec<String> = query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    let processed_tokens: Vec<String> = tokens
        .iter()
        .map(|token| {
            let normalized = normalize_spelling(token);
            let expanded = expand_synonyms(normalized);
            mythrax_core::retrieval::bm25::stem(expanded)
        })
        .collect();

    let has_temporal = processed_tokens.iter().any(|stemmed| {
        is_temporal_vocab(stemmed)
    });

    let has_preference = processed_tokens.iter().any(|stemmed| {
        is_preference_vocab(stemmed)
    });

    let has_user_vocab_match = processed_tokens.iter().any(|stemmed| {
        is_user_vocab(stemmed)
    });

    let lower_query = query.to_lowercase();
    let has_phrase_match = lower_query.contains("who am i")
        || lower_query.contains("about me")
        || tokens.windows(3).any(|w| w == ["who", "am", "i"])
        || tokens.windows(2).any(|w| w == ["about", "me"]);

    let has_user = has_user_vocab_match || has_phrase_match;

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

#[tokio::main]
async fn main() {
    let backend = SurrealBackend::new_in_memory().await.unwrap();
    StorageBackend::init(&backend).await.unwrap();

    // Load dataset questions
    let dataset_path = "bench_data/official/longmemeval_s_cleaned.json";
    let data = std::fs::read_to_string(dataset_path).unwrap();
    let val: serde_json::Value = serde_json::from_str(&data).unwrap();
    let questions = val.as_array().unwrap();

    println!("Mismatch analysis between original classifier and DB classifier on dev50 (first 50 queries):");
    println!("{:<60} | {:<12} | {:<12}", "Question", "Original", "DB Class");
    println!("{}", "-".repeat(90));

    let mut mismatches = 0;
    for q in questions.iter().take(50) {
        let question = q.get("question").unwrap().as_str().unwrap();
        let sync_cat = original_classify_query(question);
        let db_cat = backend.classify_query_db(question).await;

        if sync_cat != db_cat {
            mismatches += 1;
            if question.starts_with("When did I volunteer") {
                let tokens: Vec<String> = question
                    .to_lowercase()
                    .split(|c: char| !c.is_alphanumeric() && c != '-')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect();
                let processed: Vec<String> = tokens.iter().map(|t| mythrax_core::retrieval::bm25::stem(expand_synonyms(normalize_spelling(t)))).collect();
                println!("DEBUG: question='{}'", question);
                println!("DEBUG: tokens={:?}", tokens);
                println!("DEBUG: processed={:?}", processed);
                println!("DEBUG: is_when_temporal={}", is_temporal_vocab("when"));
            }
            println!("{:<60} | {:<12?} | {:<12?}", 
                if question.len() > 58 { &question[..58] } else { question }, 
                sync_cat, 
                db_cat
            );
        }
    }
    println!("Total mismatches on dev50: {}", mismatches);
}
