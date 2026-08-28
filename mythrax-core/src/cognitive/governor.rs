use std::collections::{HashMap, HashSet};

// Typestate pattern states
#[derive(Debug, Clone, Default)]
pub struct Exploring;

#[derive(Debug, Clone, Default)]
pub struct Executing;

#[derive(Debug, Clone, Default)]
pub struct Validating;

pub trait FsmState {}
impl FsmState for Exploring {}
impl FsmState for Executing {}
impl FsmState for Validating {}

#[derive(Debug, Clone)]
pub struct ContextGovernor<S: FsmState> {
    pub state: std::marker::PhantomData<S>,
    pub confidence: f32,
    pub pinned_nodes: HashSet<String>,
    pub unpinned_nodes: HashSet<String>,
}

impl ContextGovernor<Exploring> {
    pub fn new() -> Self {
        Self {
            state: std::marker::PhantomData,
            confidence: 0.0,
            pinned_nodes: HashSet::new(),
            unpinned_nodes: HashSet::new(),
        }
    }

    pub fn with_confidence(mut self, conf: f32) -> Self {
        self.confidence = conf;
        self
    }

    pub fn transition_to_executing(self) -> ContextGovernor<Executing> {
        ContextGovernor {
            state: std::marker::PhantomData,
            confidence: self.confidence,
            pinned_nodes: self.pinned_nodes,
            unpinned_nodes: self.unpinned_nodes,
        }
    }
}

impl ContextGovernor<Executing> {
    pub fn transition_to_validating(self) -> ContextGovernor<Validating> {
        ContextGovernor {
            state: std::marker::PhantomData,
            confidence: self.confidence,
            pinned_nodes: self.pinned_nodes,
            unpinned_nodes: self.unpinned_nodes,
        }
    }
}

impl ContextGovernor<Validating> {
    pub fn transition_to_exploring(self) -> ContextGovernor<Exploring> {
        ContextGovernor {
            state: std::marker::PhantomData,
            confidence: self.confidence,
            pinned_nodes: self.pinned_nodes,
            unpinned_nodes: self.unpinned_nodes,
        }
    }
}

impl<S: FsmState> ContextGovernor<S> {
    pub fn should_bypass_vector_search(&self) -> bool {
        self.confidence >= 0.85
    }

    pub fn pin_node(&mut self, node_id: String) {
        self.unpinned_nodes.remove(&node_id);
        self.pinned_nodes.insert(node_id);
    }

    pub fn add_unpinned_node(&mut self, node_id: String) {
        if !self.pinned_nodes.contains(&node_id) {
            self.unpinned_nodes.insert(node_id);
        }
    }

    pub fn evict_unpinned_nodes(&mut self) -> Vec<String> {
        let evicted: Vec<String> = self.unpinned_nodes.drain().collect();
        evicted
    }

    // Localized Personalized PageRank (PPR) implementation
    pub fn calculate_ppr_weights(
        &self,
        edges: &[(String, String)],
        damping_factor: f32,
        max_iterations: usize,
    ) -> HashMap<String, f32> {
        // Optimization: Map string node identifiers to contiguous integers.
        // This avoids string cloning and HashMap lookups in the inner PageRank loop.
        let mut node_to_id = HashMap::new();
        let mut id_to_node = Vec::new();

        let mut int_edges = Vec::with_capacity(edges.len());
        for (u, v) in edges {
            let u_id = *node_to_id.entry(u.as_str()).or_insert_with(|| {
                let id = id_to_node.len();
                id_to_node.push(u.clone());
                id
            });
            let v_id = *node_to_id.entry(v.as_str()).or_insert_with(|| {
                let id = id_to_node.len();
                id_to_node.push(v.clone());
                id
            });
            int_edges.push((u_id, v_id));
        }

        let n = id_to_node.len();
        if n == 0 {
            return HashMap::new();
        }

        let mut out_degree = vec![0.0; n];
        for &(u, _) in &int_edges {
            out_degree[u] += 1.0;
        }

        let mut ranks = vec![1.0 / (n as f32); n];
        let mut new_ranks = vec![0.0; n];

        let base_rank = (1.0 - damping_factor) / (n as f32);

        for _ in 0..max_iterations {
            for r in &mut new_ranks {
                *r = base_rank;
            }

            for &(u, v) in &int_edges {
                new_ranks[v] += damping_factor * (ranks[u] / out_degree[u]);
            }

            std::mem::swap(&mut ranks, &mut new_ranks);
        }

        let mut result = HashMap::with_capacity(n);
        for (i, node) in id_to_node.into_iter().enumerate() {
            result.insert(node, ranks[i]);
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fsm_transitions() {
        let exploring = ContextGovernor::<Exploring>::new().with_confidence(0.9);
        assert!(exploring.should_bypass_vector_search());

        let executing = exploring.transition_to_executing();
        assert_eq!(executing.confidence, 0.9);

        let validating = executing.transition_to_validating();
        assert_eq!(validating.confidence, 0.9);

        let exploring_again = validating.transition_to_exploring();
        assert_eq!(exploring_again.confidence, 0.9);
    }

    #[test]
    fn test_metacognitive_governor_routing() {
        let high_conf = ContextGovernor::<Exploring>::new().with_confidence(0.9);
        assert!(high_conf.should_bypass_vector_search());

        let low_conf = ContextGovernor::<Exploring>::new().with_confidence(0.5);
        assert!(!low_conf.should_bypass_vector_search());
    }

    #[test]
    fn test_tiered_context_page_swapping() {
        let mut governor = ContextGovernor::<Exploring>::new();
        governor.pin_node("node1".to_string());
        governor.add_unpinned_node("node2".to_string());
        governor.add_unpinned_node("node3".to_string());

        assert_eq!(governor.pinned_nodes.len(), 1);
        assert_eq!(governor.unpinned_nodes.len(), 2);

        let evicted = governor.evict_unpinned_nodes();
        assert_eq!(evicted.len(), 2);
        assert_eq!(governor.unpinned_nodes.len(), 0);
        assert_eq!(governor.pinned_nodes.len(), 1);
        assert!(governor.pinned_nodes.contains("node1"));
    }

    #[test]
    fn test_ppr_edge_weighting() {
        let governor = ContextGovernor::<Exploring>::new();
        let edges = vec![
            ("A".to_string(), "B".to_string()),
            ("A".to_string(), "C".to_string()),
            ("B".to_string(), "C".to_string()),
            ("C".to_string(), "A".to_string()),
        ];

        let ranks = governor.calculate_ppr_weights(&edges, 0.85, 10);
        assert!(ranks.contains_key("A"));
        assert!(ranks.contains_key("B"));
        assert!(ranks.contains_key("C"));

        // Sum should be close to 1.0 (some precision loss is expected but relative orders apply)
        let sum: f32 = ranks.values().sum();
        assert!((sum - 1.0).abs() < 0.05);
    }
}
