use mythrax_core::retrieval::bm25::stem;

fn main() {
    let preference = vec![
        "prefer", "favorite", "favourite", "like", "dislike", "love", "hate", "choice", "opinion", "choose", 
        "chose", "select", "book", "vendor", "hotel", "motel", "hostel", "resort", "restaurant", "flight", 
        "airline", "stay", "lodging", "recommend", "suggest", "accommodation", "staying"
    ];

    let user = vec![
        "my", "me", "i", "myself", "mine", "we", "us", "our", "ourselves", "profile", "bio", "resume", 
        "age", "name", "birthday", "career", "job", "occupation", "profession", "work", "degree", "graduation", 
        "major", "spouse", "husband", "wife", "partner", "parent", "mother", "father", "mom", "dad", 
        "sibling", "brother", "sister", "child", "son", "daughter", "friend", "buddy", "pal", "colleague", 
        "employer", "company", "corporation", "firm", "email", "phone", "address", "hometown", "live", 
        "reside", "born", "birth", "background", "car", "vehicle", "sneaker", "postcard", "collect", 
        "cat", "dog", "pet", "hamster", "grandma", "grandpa"
    ];

    let temporal = vec![
        "before", "previous", "prior", "earlier", "ago", "last", "yesterday", "after", "following", 
        "subsequently", "later", "next", "tomorrow", "recent", "today", "now", "first", "second", 
        "third", "fourth", "fifth", "date", "time", "when", "year", "month", "week", "day", "hour", 
        "calendar", "schedule", "meeting", "appointment", "appt", "mtg", "conference", "between", 
        "during", "past", "history", "timeline", "spend", "spent", "duration", "sunday", "monday", 
        "tuesday", "wednesday", "thursday", "friday", "saturday"
    ];

    println!("Preference seeds:");
    for w in preference {
        let stemmed = stem(w);
        println!("    UPSERT search_keyword:{} CONTENT {{ word: '{}', category: 'Preference' }};", stemmed, stemmed);
    }

    println!("\nUser seeds:");
    for w in user {
        let stemmed = stem(w);
        println!("    UPSERT search_keyword:{} CONTENT {{ word: '{}', category: 'User' }};", stemmed, stemmed);
    }

    println!("\nTemporal seeds:");
    for w in temporal {
        let stemmed = stem(w);
        println!("    UPSERT search_keyword:{} CONTENT {{ word: '{}', category: 'Temporal' }};", stemmed, stemmed);
    }
}
