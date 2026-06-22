pub const INIT_SCHEMA: &str = "
    DEFINE TABLE entity SCHEMAFULL;
    DEFINE FIELD name ON entity TYPE string;
    DEFINE FIELD entity_type ON entity TYPE string;
    DEFINE FIELD summary ON entity TYPE string;
    DEFINE FIELD labels ON entity TYPE array<string>;
    DEFINE FIELD scope ON entity TYPE string DEFAULT 'general';
    DEFINE FIELD vault_path ON entity TYPE string DEFAULT '';
    DEFINE FIELD embedding ON entity TYPE option<array<float>>;
    DEFINE INDEX entity_name ON entity FIELDS name UNIQUE;
    DEFINE INDEX entity_scope ON entity FIELDS scope;

    DEFINE TABLE episode SCHEMAFULL;
    DEFINE FIELD title ON episode TYPE string;
    DEFINE FIELD content ON episode TYPE string;
    DEFINE FIELD source ON episode TYPE string DEFAULT '';
    DEFINE FIELD scope ON episode TYPE string DEFAULT 'general';
    DEFINE FIELD vault_path ON episode TYPE string DEFAULT '';
    DEFINE FIELD processed_in_dream ON episode TYPE bool DEFAULT false;
    DEFINE FIELD embedding ON episode TYPE option<array<float>>;
    DEFINE FIELD source_episode ON episode TYPE option<record<episode>>;
    DEFINE INDEX episode_scope ON episode FIELDS scope;

    DEFINE TABLE wiki_node SCHEMAFULL;
    DEFINE FIELD name ON wiki_node TYPE string;
    DEFINE FIELD content ON wiki_node TYPE string;
    DEFINE FIELD scope ON wiki_node TYPE string DEFAULT 'general';
    DEFINE FIELD vault_path ON wiki_node TYPE string DEFAULT '';
    DEFINE FIELD embedding ON wiki_node TYPE option<array<float>>;
    DEFINE INDEX wiki_node_name ON wiki_node FIELDS name UNIQUE;
    DEFINE INDEX wiki_node_scope ON wiki_node FIELDS scope;

    DEFINE TABLE wisdom SCHEMAFULL;
    DEFINE FIELD target_pattern ON wisdom TYPE string;
    DEFINE FIELD action_to_avoid ON wisdom TYPE string;
    DEFINE FIELD causal_explanation ON wisdom TYPE string;
    DEFINE FIELD prescribed_remedy ON wisdom TYPE string;
    DEFINE FIELD tier ON wisdom TYPE string DEFAULT 'dynamic';
    DEFINE FIELD scope ON wisdom TYPE string DEFAULT 'general';
    DEFINE FIELD vault_path ON wisdom TYPE string DEFAULT '';
    DEFINE FIELD source_episodes ON wisdom TYPE array<string>;
    DEFINE FIELD generator_name ON wisdom TYPE string;
    DEFINE FIELD embedding ON wisdom TYPE option<array<float>>;
    DEFINE INDEX wisdom_scope ON wisdom FIELDS scope;
    DEFINE INDEX wisdom_tier ON wisdom FIELDS tier;

    DEFINE TABLE hypothesis_node SCHEMALESS;
    DEFINE INDEX node_id_idx ON hypothesis_node FIELDS node_id UNIQUE;
    DEFINE INDEX hypothesis_scope ON hypothesis_node FIELDS scope;

    DEFINE TABLE handoff SCHEMAFULL;
    DEFINE FIELD parent_conversation_id ON handoff TYPE string;
    DEFINE FIELD subagent_conversation_id ON handoff TYPE string;
    DEFINE FIELD summary ON handoff TYPE string;
    DEFINE FIELD handoff_file_path ON handoff TYPE string;
    DEFINE FIELD scope ON handoff TYPE string DEFAULT 'general';
    DEFINE FIELD embedding ON handoff TYPE option<array<float>>;
    DEFINE INDEX handoff_scope ON handoff FIELDS scope;
    DEFINE INDEX handoff_parent ON handoff FIELDS parent_conversation_id;
    DEFINE INDEX handoff_subagent ON handoff FIELDS subagent_conversation_id;

    DEFINE TABLE profile SCHEMAFULL;
    DEFINE FIELD key ON profile TYPE string;
    DEFINE FIELD value ON profile TYPE string;
    DEFINE INDEX profile_key ON profile FIELDS key UNIQUE;

    DEFINE TABLE config SCHEMAFULL;
    DEFINE FIELD active_provider ON config TYPE string;
    DEFINE FIELD model ON config TYPE string;
    DEFINE FIELD cloud_provider ON config TYPE string;
    DEFINE FIELD is_override ON config TYPE bool DEFAULT false;
    DEFINE FIELD expires_at ON config TYPE option<string>;

    DEFINE TABLE metrics SCHEMAFULL;
    DEFINE FIELD target_id ON metrics TYPE record;
    DEFINE FIELD utility_score ON metrics TYPE float DEFAULT 1.0;
    DEFINE FIELD access_count ON metrics TYPE int DEFAULT 0;
    DEFINE FIELD last_accessed ON metrics TYPE datetime DEFAULT time::now();
    DEFINE INDEX metrics_target ON metrics FIELDS target_id UNIQUE;

    DEFINE TABLE relates_to SCHEMALESS;
    DEFINE FIELD created_at ON relates_to TYPE datetime DEFAULT time::now();

    DEFINE TABLE mentions SCHEMALESS;
    DEFINE FIELD created_at ON mentions TYPE datetime DEFAULT time::now();
";
