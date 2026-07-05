#[derive(Debug, Clone, Default)]
pub struct BoostSignals {
    pub person_name: bool,
    pub exact_quote: bool,
    pub temporal_proximity: f32,
    pub keyword_overlap: f32,
    pub symbolic_hit: f32,
}

#[derive(Debug, Clone)]
pub struct BoostWeights {
    pub person_name: f32,
    pub exact_quote: f32,
    pub temporal_proximity: f32,
    pub keyword_overlap: f32,
    pub symbolic_hit: f32,
}

impl Default for BoostWeights {
    fn default() -> Self {
        Self {
            person_name: 0.40,
            exact_quote: 0.60,
            temporal_proximity: 0.20,
            keyword_overlap: 0.30,
            symbolic_hit: 0.50,
        }
    }
}

pub fn apply_boosts(base_dist: f32, sig: &BoostSignals, w: &BoostWeights) -> f32 {
    let mut total_boost = 0.0;
    if sig.person_name {
        total_boost += w.person_name * base_dist;
    }
    if sig.exact_quote {
        total_boost += w.exact_quote * base_dist;
    }
    total_boost += sig.temporal_proximity * w.temporal_proximity;
    total_boost += sig.keyword_overlap * w.keyword_overlap;
    total_boost += sig.symbolic_hit * w.symbolic_hit * base_dist;

    (base_dist - total_boost).clamp(-2.0, 2.0)
}
