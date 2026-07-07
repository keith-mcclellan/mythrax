pub const INIT_SCHEMA: &str = "
    DEFINE TABLE IF NOT EXISTS entity SCHEMAFULL;
    DEFINE FIELD IF NOT EXISTS name ON entity TYPE string;
    DEFINE FIELD IF NOT EXISTS entity_type ON entity TYPE string;
    DEFINE FIELD IF NOT EXISTS summary ON entity TYPE string;
    DEFINE FIELD IF NOT EXISTS labels ON entity TYPE array<string>;
    DEFINE FIELD IF NOT EXISTS scope ON entity TYPE string DEFAULT 'general';
    DEFINE FIELD IF NOT EXISTS vault_path ON entity TYPE string DEFAULT '';
    DEFINE FIELD IF NOT EXISTS embedding ON entity TYPE option<array<float>>;
    DEFINE INDEX IF NOT EXISTS entity_name ON entity FIELDS name UNIQUE;
    DEFINE INDEX IF NOT EXISTS entity_scope ON entity FIELDS scope;
    DEFINE INDEX OVERWRITE entity_hnsw ON TABLE entity FIELDS embedding HNSW DIMENSION 768 DIST COSINE TYPE F32 EFC 200 M 16;


    DEFINE TABLE IF NOT EXISTS episode SCHEMAFULL;
    DEFINE FIELD IF NOT EXISTS title ON episode TYPE string;
    DEFINE FIELD IF NOT EXISTS content ON episode TYPE string;
    DEFINE FIELD IF NOT EXISTS source ON episode TYPE string DEFAULT '';
    DEFINE FIELD IF NOT EXISTS scope ON episode TYPE string DEFAULT 'general';
    DEFINE FIELD IF NOT EXISTS vault_path ON episode TYPE string DEFAULT '';
    DEFINE FIELD IF NOT EXISTS processed_in_dream ON episode TYPE bool DEFAULT false;
    DEFINE FIELD IF NOT EXISTS embedding ON episode TYPE option<array<float>>;
    DEFINE FIELD IF NOT EXISTS source_episode ON episode TYPE option<record<episode>>;
    DEFINE FIELD IF NOT EXISTS last_retrieved_at ON episode TYPE option<string>;
    DEFINE FIELD IF NOT EXISTS utility ON episode TYPE option<float>;
    DEFINE FIELD IF NOT EXISTS importance ON episode TYPE float DEFAULT 5.0;
    DEFINE FIELD IF NOT EXISTS created_at ON episode TYPE datetime DEFAULT time::now();
    DEFINE FIELD IF NOT EXISTS archived ON episode TYPE bool DEFAULT false;
    DEFINE FIELD IF NOT EXISTS archived_at ON episode TYPE option<datetime>;
    DEFINE FIELD IF NOT EXISTS discovery_tokens ON episode TYPE option<int>;
    DEFINE FIELD IF NOT EXISTS facts ON episode TYPE option<array<string>>;
    DEFINE FIELD IF NOT EXISTS concepts ON episode TYPE option<array<string>>;
    DEFINE FIELD IF NOT EXISTS files_read ON episode TYPE option<array<string>>;
    DEFINE FIELD IF NOT EXISTS files_modified ON episode TYPE option<array<string>>;
    DEFINE FIELD IF NOT EXISTS session_id ON episode TYPE option<string>;
    DEFINE FIELD IF NOT EXISTS word_count ON episode TYPE option<int>;
    DEFINE FIELD IF NOT EXISTS metrics ON episode TYPE option<record<episode_metrics>>;
    DEFINE FIELD IF NOT EXISTS node_type ON episode TYPE string DEFAULT 'agent_thought';
    DEFINE FIELD IF NOT EXISTS confidence ON episode TYPE float DEFAULT 0.90;
    DEFINE INDEX IF NOT EXISTS episode_scope ON episode FIELDS scope;
    DEFINE INDEX IF NOT EXISTS episode_concepts ON episode FIELDS concepts;
    DEFINE INDEX OVERWRITE episode_hnsw ON TABLE episode FIELDS embedding HNSW DIMENSION 768 DIST COSINE TYPE F32 EFC 200 M 16;
    DEFINE INDEX IF NOT EXISTS episode_session ON episode FIELDS session_id;
    DEFINE INDEX IF NOT EXISTS episode_vault_path ON episode FIELDS vault_path;
    DEFINE INDEX IF NOT EXISTS episode_scope_created ON episode FIELDS scope, created_at;
    DEFINE INDEX IF NOT EXISTS episode_node_type ON episode FIELDS node_type;
    DEFINE INDEX IF NOT EXISTS episode_processed_in_dream ON episode FIELDS processed_in_dream;
    DEFINE INDEX IF NOT EXISTS episode_created_at ON episode FIELDS created_at;
    DEFINE ANALYZER IF NOT EXISTS ascii TOKENIZERS blank, punct FILTERS lowercase, ascii;
    DEFINE ANALYZER IF NOT EXISTS snowball_en TOKENIZERS blank, punct FILTERS lowercase, snowball(english);
    DEFINE INDEX OVERWRITE episode_content_search ON TABLE episode FIELDS content FULLTEXT ANALYZER snowball_en BM25(1.2, 0.60);


    DEFINE TABLE IF NOT EXISTS wiki_node SCHEMAFULL;
    DEFINE FIELD IF NOT EXISTS name ON wiki_node TYPE string;
    DEFINE FIELD IF NOT EXISTS content ON wiki_node TYPE string;
    DEFINE FIELD IF NOT EXISTS scope ON wiki_node TYPE string DEFAULT 'general';
    DEFINE FIELD IF NOT EXISTS vault_path ON wiki_node TYPE string DEFAULT '';
    DEFINE FIELD IF NOT EXISTS embedding ON wiki_node TYPE option<array<float>>;
    DEFINE FIELD IF NOT EXISTS importance ON wiki_node TYPE float DEFAULT 5.0;
    DEFINE FIELD IF NOT EXISTS last_retrieved_at ON wiki_node TYPE option<string>;
    DEFINE FIELD IF NOT EXISTS created_at ON wiki_node TYPE datetime DEFAULT time::now();
    DEFINE FIELD IF NOT EXISTS utility ON wiki_node TYPE option<float>;
    DEFINE INDEX IF NOT EXISTS wiki_node_name ON wiki_node FIELDS name UNIQUE;
    DEFINE INDEX IF NOT EXISTS wiki_node_scope ON wiki_node FIELDS scope;
    DEFINE INDEX OVERWRITE wiki_node_hnsw ON TABLE wiki_node FIELDS embedding HNSW DIMENSION 768 DIST COSINE TYPE F32 EFC 200 M 16;


    DEFINE TABLE IF NOT EXISTS wisdom SCHEMAFULL;
    DEFINE FIELD IF NOT EXISTS target_pattern ON wisdom TYPE string;
    DEFINE FIELD IF NOT EXISTS action_to_avoid ON wisdom TYPE string;
    DEFINE FIELD IF NOT EXISTS causal_explanation ON wisdom TYPE string;
    DEFINE FIELD IF NOT EXISTS prescribed_remedy ON wisdom TYPE string;
    DEFINE FIELD IF NOT EXISTS tier ON wisdom TYPE string DEFAULT 'dynamic';
    DEFINE FIELD IF NOT EXISTS scope ON wisdom TYPE string DEFAULT 'general';
    DEFINE FIELD IF NOT EXISTS vault_path ON wisdom TYPE string DEFAULT '';
    DEFINE FIELD IF NOT EXISTS source_episodes ON wisdom TYPE array<string>;
    DEFINE FIELD IF NOT EXISTS generator_name ON wisdom TYPE string;
    DEFINE FIELD IF NOT EXISTS embedding ON wisdom TYPE option<array<float>>;
    DEFINE FIELD IF NOT EXISTS status ON wisdom TYPE string DEFAULT 'active';
    DEFINE FIELD IF NOT EXISTS superseded_at ON wisdom TYPE option<datetime>;
    DEFINE FIELD IF NOT EXISTS superseded_by ON wisdom TYPE option<string>;
    DEFINE FIELD IF NOT EXISTS importance ON wisdom TYPE float DEFAULT 5.0;
    DEFINE FIELD IF NOT EXISTS last_retrieved_at ON wisdom TYPE option<string>;
    DEFINE FIELD IF NOT EXISTS created_at ON wisdom TYPE datetime DEFAULT time::now();
    DEFINE FIELD IF NOT EXISTS rule_type ON wisdom TYPE string DEFAULT 'aesthetic';
    DEFINE INDEX IF NOT EXISTS wisdom_scope ON wisdom FIELDS scope;
    DEFINE INDEX IF NOT EXISTS wisdom_tier ON wisdom FIELDS tier;
    DEFINE INDEX OVERWRITE wisdom_hnsw ON TABLE wisdom FIELDS embedding HNSW DIMENSION 768 DIST COSINE TYPE F32 EFC 200 M 16;


    DEFINE TABLE IF NOT EXISTS hypothesis_node SCHEMALESS;
    DEFINE INDEX IF NOT EXISTS node_id_idx ON hypothesis_node FIELDS node_id UNIQUE;
    DEFINE INDEX IF NOT EXISTS hypothesis_scope ON hypothesis_node FIELDS scope;

    DEFINE TABLE IF NOT EXISTS handoff SCHEMAFULL;
    DEFINE FIELD IF NOT EXISTS parent_conversation_id ON handoff TYPE string;
    DEFINE FIELD IF NOT EXISTS subagent_conversation_id ON handoff TYPE string;
    DEFINE FIELD IF NOT EXISTS summary ON handoff TYPE string;
    DEFINE FIELD IF NOT EXISTS handoff_file_path ON handoff TYPE string;
    DEFINE FIELD IF NOT EXISTS scope ON handoff TYPE string DEFAULT 'general';
    DEFINE FIELD IF NOT EXISTS status ON handoff TYPE string DEFAULT 'PENDING';
    DEFINE FIELD IF NOT EXISTS created_at ON handoff TYPE datetime DEFAULT time::now();
    DEFINE FIELD IF NOT EXISTS embedding ON handoff TYPE option<array<float>>;
    DEFINE FIELD IF NOT EXISTS include_tool_execution ON handoff TYPE option<bool>;
    DEFINE INDEX IF NOT EXISTS handoff_scope ON handoff FIELDS scope;
    DEFINE INDEX IF NOT EXISTS handoff_parent ON handoff FIELDS parent_conversation_id;
    DEFINE INDEX IF NOT EXISTS handoff_subagent ON handoff FIELDS subagent_conversation_id;
    DEFINE INDEX OVERWRITE handoff_hnsw ON TABLE handoff FIELDS embedding HNSW DIMENSION 768 DIST COSINE TYPE F32 EFC 200 M 16;
    DEFINE TABLE IF NOT EXISTS profile SCHEMAFULL;
    DEFINE FIELD IF NOT EXISTS key ON profile TYPE string;
    DEFINE FIELD IF NOT EXISTS value ON profile TYPE string;
    DEFINE INDEX IF NOT EXISTS profile_key ON profile FIELDS key UNIQUE;

    DEFINE TABLE IF NOT EXISTS config SCHEMAFULL;
    DEFINE FIELD IF NOT EXISTS active_provider ON config TYPE string;
    DEFINE FIELD IF NOT EXISTS model ON config TYPE string;
    DEFINE FIELD IF NOT EXISTS cloud_provider ON config TYPE string;
    DEFINE FIELD IF NOT EXISTS is_override ON config TYPE bool DEFAULT false;
    DEFINE FIELD IF NOT EXISTS expires_at ON config TYPE option<string>;

    DEFINE TABLE IF NOT EXISTS metrics SCHEMAFULL;
    DEFINE FIELD IF NOT EXISTS target_id ON metrics TYPE record;
    DEFINE FIELD IF NOT EXISTS utility_score ON metrics TYPE float DEFAULT 1.0;
    DEFINE FIELD IF NOT EXISTS access_count ON metrics TYPE int DEFAULT 0;
    DEFINE FIELD IF NOT EXISTS last_accessed ON metrics TYPE datetime DEFAULT time::now();
    DEFINE INDEX IF NOT EXISTS metrics_target ON metrics FIELDS target_id UNIQUE;

    DEFINE TABLE IF NOT EXISTS episode_metrics SCHEMAFULL;
    DEFINE FIELD IF NOT EXISTS episode ON episode_metrics TYPE record<episode>;
    DEFINE FIELD IF NOT EXISTS utility ON episode_metrics TYPE option<float>;
    DEFINE FIELD IF NOT EXISTS last_retrieved_at ON episode_metrics TYPE option<datetime>;
    DEFINE FIELD IF NOT EXISTS word_count ON episode_metrics TYPE option<int>;

    DEFINE TABLE IF NOT EXISTS relates_to SCHEMAFULL TYPE RELATION IN episode | wiki_node | wisdom | handoff | entity | thought_node | belief_state OUT episode | wiki_node | wisdom | handoff | entity | thought_node | belief_state;
    DEFINE FIELD IF NOT EXISTS relation ON relates_to TYPE option<string>;
    DEFINE FIELD IF NOT EXISTS strength ON relates_to TYPE option<float>;
    DEFINE FIELD IF NOT EXISTS created_at ON relates_to TYPE datetime DEFAULT time::now();
    DEFINE FIELD IF NOT EXISTS valid_from ON relates_to TYPE option<datetime>;
    DEFINE FIELD IF NOT EXISTS valid_to   ON relates_to TYPE option<datetime>;
    DEFINE FIELD IF NOT EXISTS confidence ON relates_to TYPE float DEFAULT 1.0;
    DEFINE INDEX IF NOT EXISTS idx_relates_valid ON relates_to FIELDS valid_from, valid_to;

    DEFINE TABLE IF NOT EXISTS mentions SCHEMAFULL TYPE RELATION IN episode OUT entity;
    DEFINE FIELD IF NOT EXISTS created_at ON mentions TYPE datetime DEFAULT time::now();

    DEFINE TABLE IF NOT EXISTS short_term_memory SCHEMAFULL;
    DEFINE FIELD IF NOT EXISTS session_id ON short_term_memory TYPE string;
    DEFINE FIELD IF NOT EXISTS key ON short_term_memory TYPE string;
    DEFINE FIELD IF NOT EXISTS value ON short_term_memory TYPE string;
    DEFINE FIELD IF NOT EXISTS updated_at ON short_term_memory TYPE datetime DEFAULT time::now();
    DEFINE INDEX IF NOT EXISTS stm_session_key ON short_term_memory FIELDS session_id, key UNIQUE;

    DEFINE TABLE IF NOT EXISTS followed_by SCHEMAFULL TYPE RELATION IN episode OUT episode;
    DEFINE FIELD IF NOT EXISTS duration ON followed_by TYPE option<duration>;
    DEFINE FIELD IF NOT EXISTS created_at ON followed_by TYPE datetime DEFAULT time::now();

    DEFINE TABLE IF NOT EXISTS superseded_by SCHEMAFULL TYPE RELATION IN wisdom OUT wisdom;
    DEFINE FIELD IF NOT EXISTS reason ON superseded_by TYPE option<string>;
    DEFINE FIELD IF NOT EXISTS created_at ON superseded_by TYPE datetime DEFAULT time::now();

    DEFINE TABLE IF NOT EXISTS session_state SCHEMALESS;
    DEFINE TABLE IF NOT EXISTS checkpoint_node SCHEMALESS;
    DEFINE TABLE IF NOT EXISTS symbol_archive SCHEMALESS;

    DEFINE TABLE IF NOT EXISTS belief_state SCHEMAFULL;
    DEFINE FIELD IF NOT EXISTS session_id ON belief_state TYPE string;
    DEFINE FIELD IF NOT EXISTS tasks_todo ON belief_state TYPE array<string>;
    DEFINE FIELD IF NOT EXISTS hypotheses_tested ON belief_state TYPE array<string>;
    DEFINE FIELD IF NOT EXISTS confidence_score ON belief_state TYPE float;
    DEFINE FIELD IF NOT EXISTS uncertainty_areas ON belief_state TYPE array<string>;
    DEFINE FIELD IF NOT EXISTS updated_at ON belief_state TYPE string;
    DEFINE INDEX IF NOT EXISTS belief_state_session ON belief_state FIELDS session_id;

    DEFINE TABLE IF NOT EXISTS thought_node SCHEMAFULL;
    DEFINE FIELD IF NOT EXISTS title ON thought_node TYPE string;
    DEFINE FIELD IF NOT EXISTS content ON thought_node TYPE string;
    DEFINE FIELD IF NOT EXISTS scope ON thought_node TYPE string DEFAULT 'general';
    DEFINE FIELD IF NOT EXISTS vault_path ON thought_node TYPE option<string>;
    DEFINE FIELD IF NOT EXISTS created_at ON thought_node TYPE string;
    DEFINE INDEX IF NOT EXISTS thought_node_scope ON thought_node FIELDS scope;

    DEFINE TABLE IF NOT EXISTS chat_history SCHEMAFULL;
    DEFINE FIELD IF NOT EXISTS session_id ON chat_history TYPE string;
    DEFINE FIELD IF NOT EXISTS role ON chat_history TYPE string; -- 'user' or 'assistant'
    DEFINE FIELD IF NOT EXISTS content ON chat_history TYPE string;
    DEFINE FIELD IF NOT EXISTS created_at ON chat_history TYPE datetime DEFAULT time::now();
    DEFINE INDEX IF NOT EXISTS ch_session ON chat_history FIELDS session_id;

    DEFINE TABLE IF NOT EXISTS wiki_node_history SCHEMAFULL;
    DEFINE FIELD IF NOT EXISTS node_id ON wiki_node_history TYPE record<wiki_node>;
    DEFINE FIELD IF NOT EXISTS name ON wiki_node_history TYPE string;
    DEFINE FIELD IF NOT EXISTS content ON wiki_node_history TYPE string;
    DEFINE FIELD IF NOT EXISTS scope ON wiki_node_history TYPE string DEFAULT 'general';
    DEFINE FIELD IF NOT EXISTS vault_path ON wiki_node_history TYPE string DEFAULT '';
    DEFINE FIELD IF NOT EXISTS embedding ON wiki_node_history TYPE option<array<float>>;
    DEFINE FIELD IF NOT EXISTS importance ON wiki_node_history TYPE float DEFAULT 5.0;
    DEFINE FIELD IF NOT EXISTS last_retrieved_at ON wiki_node_history TYPE option<string>;
    DEFINE FIELD IF NOT EXISTS created_at ON wiki_node_history TYPE datetime DEFAULT time::now();
    DEFINE FIELD IF NOT EXISTS utility ON wiki_node_history TYPE option<float>;
    DEFINE FIELD IF NOT EXISTS changed_at ON wiki_node_history TYPE datetime DEFAULT time::now();
    DEFINE INDEX IF NOT EXISTS wiki_node_history_node ON wiki_node_history FIELDS node_id;
    DEFINE INDEX IF NOT EXISTS wiki_node_history_scope ON wiki_node_history FIELDS scope;

    DEFINE EVENT IF NOT EXISTS wiki_node_update_history ON TABLE wiki_node WHEN $event = 'UPDATE' THEN (
        CREATE wiki_node_history CONTENT {
            node_id: $value.id,
            name: $value.name,
            content: $value.content,
            scope: $value.scope,
            vault_path: $value.vault_path,
            embedding: $value.embedding,
            importance: $value.importance,
            last_retrieved_at: $value.last_retrieved_at,
            created_at: $value.created_at,
            utility: $value.utility,
            changed_at: time::now()
        }
    );

    UPSERT type::record('profile', 'search.enable_cross_encoder_rerank') CONTENT { key: 'search.enable_cross_encoder_rerank', value: 'true' };
    UPSERT type::record('profile', 'search.rerank_pool_size') CONTENT { key: 'search.rerank_pool_size', value: '15' };
    UPSERT type::record('profile', 'search.rerank_weight') CONTENT { key: 'search.rerank_weight', value: '0.15' };
    UPSERT type::record('profile', 'search.sigmoid_center') CONTENT { key: 'search.sigmoid_center', value: '0.45' };
    UPSERT type::record('profile', 'search.fusion_sigmoid_center') CONTENT { key: 'search.fusion_sigmoid_center', value: '0.6' };
    UPSERT type::record('profile', 'search.gamma_rerank') CONTENT { key: 'search.gamma_rerank', value: '0.2' };
    UPSERT type::record('profile', 'search.enable_spreading_activation') CONTENT { key: 'search.enable_spreading_activation', value: 'true' };
    UPSERT type::record('profile', 'search.enable_stm_retrieval') CONTENT { key: 'search.enable_stm_retrieval', value: 'true' };
    UPSERT type::record('profile', 'search.enable_access_reinforcement') CONTENT { key: 'search.enable_access_reinforcement', value: 'true' };
    UPSERT type::record('profile', 'compactor.enable_contradiction_detection') CONTENT { key: 'compactor.enable_contradiction_detection', value: 'true' };
    UPSERT type::record('profile', 'compactor.protect_procedural_nodes') CONTENT { key: 'compactor.protect_procedural_nodes', value: 'true' };
    UPSERT type::record('profile', 'compactor.enable_near_duplicate_merging') CONTENT { key: 'compactor.enable_near_duplicate_merging', value: 'true' };
    UPSERT type::record('profile', 'search.enable_calibrated_confidence') CONTENT { key: 'search.enable_calibrated_confidence', value: 'true' };
    UPSERT type::record('profile', 'search.enable_gaussian_temporal') CONTENT { key: 'search.enable_gaussian_temporal', value: 'true' };
    UPSERT type::record('profile', 'search.hnsw_ef') CONTENT { key: 'search.hnsw_ef', value: '64' };
    UPSERT type::record('profile', 'search.spreading_activation_attenuation') CONTENT { key: 'search.spreading_activation_attenuation', value: '0.7' };
    UPSERT type::record('profile', 'search.stm_relevance_threshold') CONTENT { key: 'search.stm_relevance_threshold', value: '0.4' };
    UPSERT type::record('profile', 'search.gaussian_temporal_sigma') CONTENT { key: 'search.gaussian_temporal_sigma', value: '168.0' };
    UPSERT type::record('profile', 'search.tfidf_pool_size') CONTENT { key: 'search.tfidf_pool_size', value: '100' };
";
