use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone)]
pub struct ActiveNodeInfo {
    pub importance: f32,
    pub node_type: String, // "episode", "wiki_node", "wisdom", "stm", "handoff"
    pub pinned: bool,
}

pub struct PagingManager {
    pub capacity: usize,
    pub lru_queue: VecDeque<String>,
    pub active_nodes: HashMap<String, ActiveNodeInfo>,
}

impl PagingManager {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            lru_queue: VecDeque::new(),
            active_nodes: HashMap::new(),
        }
    }

    pub fn access_node(&mut self, id: String, info: ActiveNodeInfo) {
        if self.active_nodes.contains_key(&id) {
            if let Some(pos) = self.lru_queue.iter().position(|x| x == &id) {
                self.lru_queue.remove(pos);
            }
        }
        self.active_nodes.insert(id.clone(), info);
        self.lru_queue.push_back(id);
    }

    pub fn evict_if_needed(&mut self) -> Vec<String> {
        let mut evicted = Vec::new();
        while self.active_nodes.len() > self.capacity {
            let mut found_candidate = false;
            for i in 0..self.lru_queue.len() {
                let id = &self.lru_queue[i];
                if let Some(info) = self.active_nodes.get(id) {
                    if !info.pinned {
                        let id_to_evict = self.lru_queue.remove(i).unwrap();
                        self.active_nodes.remove(&id_to_evict);
                        evicted.push(id_to_evict);
                        found_candidate = true;
                        break;
                    }
                }
            }
            if !found_candidate {
                break;
            }
        }
        evicted
    }
}
