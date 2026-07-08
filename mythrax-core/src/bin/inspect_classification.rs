use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct QuestionEntry {
    question_id: String,
    question: String,
    question_type: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open("bench_data/official/longmemeval_s_cleaned.json")?;
    let reader = BufReader::new(file);
    let questions: Vec<QuestionEntry> = serde_json::from_reader(reader)?;

    let mut sorted = questions;
    sorted.sort_by(|a, b| a.question_id.cmp(&b.question_id));

    let mut dev_subset = Vec::new();
    let mut counts = HashMap::new();
    let limits = [
        ("knowledge-update".to_string(), 8),
        ("multi-session".to_string(), 13),
        ("single-session-assistant".to_string(), 6),
        ("single-session-preference".to_string(), 3),
        ("single-session-user".to_string(), 7),
        ("temporal-reasoning".to_string(), 13),
    ].into_iter().collect::<HashMap<String, usize>>();

    for q in sorted {
        let limit = limits.get(&q.question_type).cloned().unwrap_or(0);
        let count = counts.entry(q.question_type.clone()).or_insert(0);
        if *count < limit {
            dev_subset.push(q);
            *count += 1;
        }
    }

    println!("{:<30} | {:<30} | {:<20} | {}", "ID", "Dataset Type", "Classified Cat", "Question");
    println!("{}", "-".repeat(130));

    for q in dev_subset {
        let classified = mythrax_core::db::backend::classify_query(&q.question);
        println!(
            "{:<30} | {:<30} | {:<20} | {}",
            q.question_id,
            q.question_type,
            format!("{:?}", classified),
            q.question
        );
    }

    Ok(())
}
