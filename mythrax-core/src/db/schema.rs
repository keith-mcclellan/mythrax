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
    DEFINE FIELD IF NOT EXISTS severity ON wisdom TYPE option<string> DEFAULT 'info';
    DEFINE FIELD IF NOT EXISTS blocking ON wisdom TYPE option<bool> DEFAULT false;
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
    DEFINE FIELD IF NOT EXISTS llm_post_inference_delay_ms ON config TYPE option<int>;

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
    DEFINE FIELD IF NOT EXISTS expires_at ON short_term_memory TYPE option<datetime>;
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

    UPSERT type::record('profile', 'search.enable_cross_encoder_rerank') CONTENT { key: 'search.enable_cross_encoder_rerank', value: 'false' };
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

    DEFINE TABLE IF NOT EXISTS search_keyword SCHEMAFULL;
    DEFINE FIELD IF NOT EXISTS word ON search_keyword TYPE string;
    DEFINE FIELD IF NOT EXISTS category ON search_keyword TYPE string;
    DEFINE INDEX IF NOT EXISTS keyword_word ON search_keyword FIELDS word UNIQUE;
    DEFINE INDEX OVERWRITE keyword_search ON TABLE search_keyword FIELDS word FULLTEXT ANALYZER snowball_en;

    UPSERT search_keyword:prefer CONTENT { word: 'prefer', category: 'Preference' };
    UPSERT search_keyword:favorite CONTENT { word: 'favorite', category: 'Preference' };
    UPSERT search_keyword:favourite CONTENT { word: 'favourite', category: 'Preference' };
    UPSERT search_keyword:like CONTENT { word: 'like', category: 'Preference' };
    UPSERT search_keyword:dislike CONTENT { word: 'dislike', category: 'Preference' };
    UPSERT search_keyword:love CONTENT { word: 'love', category: 'Preference' };
    UPSERT search_keyword:hate CONTENT { word: 'hate', category: 'Preference' };
    UPSERT search_keyword:choice CONTENT { word: 'choice', category: 'Preference' };
    UPSERT search_keyword:opinion CONTENT { word: 'opinion', category: 'Preference' };
    UPSERT search_keyword:choose CONTENT { word: 'choose', category: 'Preference' };
    UPSERT search_keyword:chose CONTENT { word: 'chose', category: 'Preference' };
    UPSERT search_keyword:select CONTENT { word: 'select', category: 'Preference' };
    UPSERT search_keyword:book CONTENT { word: 'book', category: 'Preference' };
    UPSERT search_keyword:vendor CONTENT { word: 'vendor', category: 'Preference' };
    UPSERT search_keyword:hotel CONTENT { word: 'hotel', category: 'Preference' };
    UPSERT search_keyword:motel CONTENT { word: 'motel', category: 'Preference' };
    UPSERT search_keyword:hostel CONTENT { word: 'hostel', category: 'Preference' };
    UPSERT search_keyword:resort CONTENT { word: 'resort', category: 'Preference' };
    UPSERT search_keyword:restaurant CONTENT { word: 'restaurant', category: 'Preference' };
    UPSERT search_keyword:flight CONTENT { word: 'flight', category: 'Preference' };
    UPSERT search_keyword:airline CONTENT { word: 'airline', category: 'Preference' };
    UPSERT search_keyword:stay CONTENT { word: 'stay', category: 'Preference' };
    UPSERT search_keyword:lodging CONTENT { word: 'lodging', category: 'Preference' };
    UPSERT search_keyword:staying CONTENT { word: 'staying', category: 'Preference' };
    UPSERT search_keyword:recommend CONTENT { word: 'recommend', category: 'Preference' };
    UPSERT search_keyword:suggest CONTENT { word: 'suggest', category: 'Preference' };
    UPSERT search_keyword:recommendation CONTENT { word: 'recommendation', category: 'Preference' };
    UPSERT search_keyword:suggestion CONTENT { word: 'suggestion', category: 'Preference' };
    UPSERT search_keyword:accommodation CONTENT { word: 'accommodation', category: 'Preference' };
    UPSERT search_keyword:recipe CONTENT { word: 'recipe', category: 'Preference' };
    UPSERT search_keyword:dinner CONTENT { word: 'dinner', category: 'Preference' };
    UPSERT search_keyword:cook CONTENT { word: 'cook', category: 'Preference' };
    UPSERT search_keyword:cooking CONTENT { word: 'cooking', category: 'Preference' };
    UPSERT search_keyword:serve CONTENT { word: 'serve', category: 'Preference' };

    UPSERT search_keyword:profile CONTENT { word: 'profile', category: 'User' };
    UPSERT search_keyword:bio CONTENT { word: 'bio', category: 'User' };
    UPSERT search_keyword:resume CONTENT { word: 'resume', category: 'User' };
    UPSERT search_keyword:age CONTENT { word: 'age', category: 'User' };
    UPSERT search_keyword:name CONTENT { word: 'name', category: 'User' };
    UPSERT search_keyword:birthday CONTENT { word: 'birthday', category: 'User' };
    UPSERT search_keyword:career CONTENT { word: 'career', category: 'User' };
    UPSERT search_keyword:job CONTENT { word: 'job', category: 'User' };
    UPSERT search_keyword:occupation CONTENT { word: 'occupation', category: 'User' };
    UPSERT search_keyword:profession CONTENT { word: 'profession', category: 'User' };
    UPSERT search_keyword:work CONTENT { word: 'work', category: 'User' };
    UPSERT search_keyword:degree CONTENT { word: 'degree', category: 'User' };
    UPSERT search_keyword:graduation CONTENT { word: 'graduation', category: 'User' };
    UPSERT search_keyword:major CONTENT { word: 'major', category: 'User' };
    UPSERT search_keyword:spouse CONTENT { word: 'spouse', category: 'User' };
    UPSERT search_keyword:husband CONTENT { word: 'husband', category: 'User' };
    UPSERT search_keyword:wife CONTENT { word: 'wife', category: 'User' };
    UPSERT search_keyword:partner CONTENT { word: 'partner', category: 'User' };
    UPSERT search_keyword:parent CONTENT { word: 'parent', category: 'User' };
    UPSERT search_keyword:mother CONTENT { word: 'mother', category: 'User' };
    UPSERT search_keyword:father CONTENT { word: 'father', category: 'User' };
    UPSERT search_keyword:mom CONTENT { word: 'mom', category: 'User' };
    UPSERT search_keyword:dad CONTENT { word: 'dad', category: 'User' };
    UPSERT search_keyword:sibling CONTENT { word: 'sibling', category: 'User' };
    UPSERT search_keyword:brother CONTENT { word: 'brother', category: 'User' };
    UPSERT search_keyword:sister CONTENT { word: 'sister', category: 'User' };
    UPSERT search_keyword:child CONTENT { word: 'child', category: 'User' };
    UPSERT search_keyword:son CONTENT { word: 'son', category: 'User' };
    UPSERT search_keyword:daughter CONTENT { word: 'daughter', category: 'User' };
    UPSERT search_keyword:friend CONTENT { word: 'friend', category: 'User' };
    UPSERT search_keyword:buddy CONTENT { word: 'buddy', category: 'User' };
    UPSERT search_keyword:pal CONTENT { word: 'pal', category: 'User' };
    UPSERT search_keyword:colleague CONTENT { word: 'colleague', category: 'User' };
    UPSERT search_keyword:employer CONTENT { word: 'employer', category: 'User' };
    UPSERT search_keyword:company CONTENT { word: 'company', category: 'User' };
    UPSERT search_keyword:corporation CONTENT { word: 'corporation', category: 'User' };
    UPSERT search_keyword:firm CONTENT { word: 'firm', category: 'User' };
    UPSERT search_keyword:email CONTENT { word: 'email', category: 'User' };
    UPSERT search_keyword:phone CONTENT { word: 'phone', category: 'User' };
    UPSERT search_keyword:address CONTENT { word: 'address', category: 'User' };
    UPSERT search_keyword:hometown CONTENT { word: 'hometown', category: 'User' };
    UPSERT search_keyword:live CONTENT { word: 'live', category: 'User' };
    UPSERT search_keyword:reside CONTENT { word: 'reside', category: 'User' };
    UPSERT search_keyword:born CONTENT { word: 'born', category: 'User' };
    UPSERT search_keyword:birth CONTENT { word: 'birth', category: 'User' };
    UPSERT search_keyword:background CONTENT { word: 'background', category: 'User' };
    UPSERT search_keyword:car CONTENT { word: 'car', category: 'User' };
    UPSERT search_keyword:vehicle CONTENT { word: 'vehicle', category: 'User' };
    UPSERT search_keyword:sneaker CONTENT { word: 'sneaker', category: 'User' };
    UPSERT search_keyword:postcard CONTENT { word: 'postcard', category: 'User' };
    UPSERT search_keyword:collect CONTENT { word: 'collect', category: 'User' };
    UPSERT search_keyword:cat CONTENT { word: 'cat', category: 'User' };
    UPSERT search_keyword:dog CONTENT { word: 'dog', category: 'User' };
    UPSERT search_keyword:pet CONTENT { word: 'pet', category: 'User' };
    UPSERT search_keyword:hamster CONTENT { word: 'hamster', category: 'User' };
    UPSERT search_keyword:grandma CONTENT { word: 'grandma', category: 'User' };
    UPSERT search_keyword:grandpa CONTENT { word: 'grandpa', category: 'User' };

    UPSERT search_keyword:before CONTENT { word: 'before', category: 'Temporal' };
    UPSERT search_keyword:previous CONTENT { word: 'previous', category: 'Temporal' };
    UPSERT search_keyword:previously CONTENT { word: 'previously', category: 'Temporal' };
    UPSERT search_keyword:prior CONTENT { word: 'prior', category: 'Temporal' };
    UPSERT search_keyword:earlier CONTENT { word: 'earlier', category: 'Temporal' };
    UPSERT search_keyword:ago CONTENT { word: 'ago', category: 'Temporal' };
    UPSERT search_keyword:last CONTENT { word: 'last', category: 'Temporal' };
    UPSERT search_keyword:yesterday CONTENT { word: 'yesterday', category: 'Temporal' };
    UPSERT search_keyword:after CONTENT { word: 'after', category: 'Temporal' };
    UPSERT search_keyword:following CONTENT { word: 'following', category: 'Temporal' };
    UPSERT search_keyword:subsequently CONTENT { word: 'subsequently', category: 'Temporal' };
    UPSERT search_keyword:later CONTENT { word: 'later', category: 'Temporal' };
    UPSERT search_keyword:next CONTENT { word: 'next', category: 'Temporal' };
    UPSERT search_keyword:tomorrow CONTENT { word: 'tomorrow', category: 'Temporal' };
    UPSERT search_keyword:recent CONTENT { word: 'recent', category: 'Temporal' };
    UPSERT search_keyword:recently CONTENT { word: 'recently', category: 'Temporal' };
    UPSERT search_keyword:today CONTENT { word: 'today', category: 'Temporal' };
    UPSERT search_keyword:now CONTENT { word: 'now', category: 'Temporal' };
    UPSERT search_keyword:first CONTENT { word: 'first', category: 'Temporal' };
    UPSERT search_keyword:second CONTENT { word: 'second', category: 'Temporal' };
    UPSERT search_keyword:third CONTENT { word: 'third', category: 'Temporal' };
    UPSERT search_keyword:fourth CONTENT { word: 'fourth', category: 'Temporal' };
    UPSERT search_keyword:fifth CONTENT { word: 'fifth', category: 'Temporal' };
    UPSERT search_keyword:date CONTENT { word: 'date', category: 'Temporal' };
    UPSERT search_keyword:time CONTENT { word: 'time', category: 'Temporal' };
    UPSERT search_keyword:when CONTENT { word: 'when', category: 'Temporal' };
    UPSERT search_keyword:year CONTENT { word: 'year', category: 'Temporal' };
    UPSERT search_keyword:month CONTENT { word: 'month', category: 'Temporal' };
    UPSERT search_keyword:week CONTENT { word: 'week', category: 'Temporal' };
    UPSERT search_keyword:day CONTENT { word: 'day', category: 'Temporal' };
    UPSERT search_keyword:hour CONTENT { word: 'hour', category: 'Temporal' };
    UPSERT search_keyword:calendar CONTENT { word: 'calendar', category: 'Temporal' };
    UPSERT search_keyword:schedule CONTENT { word: 'schedule', category: 'Temporal' };
    UPSERT search_keyword:meeting CONTENT { word: 'meeting', category: 'Temporal' };
    UPSERT search_keyword:appointment CONTENT { word: 'appointment', category: 'Temporal' };
    UPSERT search_keyword:appt CONTENT { word: 'appt', category: 'Temporal' };
    UPSERT search_keyword:mtg CONTENT { word: 'mtg', category: 'Temporal' };
    UPSERT search_keyword:conference CONTENT { word: 'conference', category: 'Temporal' };
    UPSERT search_keyword:between CONTENT { word: 'between', category: 'Temporal' };
    UPSERT search_keyword:during CONTENT { word: 'during', category: 'Temporal' };
    UPSERT search_keyword:past CONTENT { word: 'past', category: 'Temporal' };
    UPSERT search_keyword:history CONTENT { word: 'history', category: 'Temporal' };
    UPSERT search_keyword:timeline CONTENT { word: 'timeline', category: 'Temporal' };
    UPSERT search_keyword:spend CONTENT { word: 'spend', category: 'Temporal' };
    UPSERT search_keyword:spent CONTENT { word: 'spent', category: 'Temporal' };
    UPSERT search_keyword:duration CONTENT { word: 'duration', category: 'Temporal' };
    UPSERT search_keyword:sunday CONTENT { word: 'sunday', category: 'Temporal' };
    UPSERT search_keyword:monday CONTENT { word: 'monday', category: 'Temporal' };
    UPSERT search_keyword:tuesday CONTENT { word: 'tuesday', category: 'Temporal' };
    UPSERT search_keyword:wednesday CONTENT { word: 'wednesday', category: 'Temporal' };
    UPSERT search_keyword:thursday CONTENT { word: 'thursday', category: 'Temporal' };
    UPSERT search_keyword:friday CONTENT { word: 'friday', category: 'Temporal' };
    UPSERT search_keyword:saturday CONTENT { word: 'saturday', category: 'Temporal' };
";
