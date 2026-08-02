use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtractedFactDto {
    pub hypothesis: String,
    pub causal_insight: String,
    #[serde(default)]
    pub item_type: Option<String>,
    #[serde(default)]
    pub raw_evidence: Vec<String>,
    #[serde(default)]
    pub artifact_refs: Vec<String>,
    pub metacognitive_confidence: i32,
    #[serde(default)]
    pub slug: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtractFactsResponse {
    #[serde(default)]
    pub facts: Vec<ExtractedFactDto>,
    #[serde(default)]
    pub no_facts_reason: Option<String>,
}


#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FormHypothesisDto {
    pub claim: String,
    pub insight: String,
    #[serde(default)]
    pub item_type: Option<String>,
    pub fact_indices: Vec<usize>,
    #[serde(default)]
    pub slug: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FormHypothesesResponse {
    pub hypotheses: Vec<FormHypothesisDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RefineHypothesisResponse {
    pub action: String, // "support", "contradict", "irrelevant"
    pub new_confidence: f32,
    pub refined_insight: String,
    pub reasoning: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AncestorMergeResponse {
    pub suggested_path: String,
    pub title: String,
    pub markdown_content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraduationResponse {
    pub scope: String, // "project_specific" or "universal"
    pub reasoning: String,
}

pub fn build_episode_extraction_prompt(transcript: &str) -> (String, String) {
    let system = r#"You are an Arbor leaf-insight extractor implementing the HTR cognitive memory model. Given an agent conversation transcript (raw turns), extract all technical facts, syntax rules, API patterns, user directives, preferences, architectural decisions, and mechanisms tested following the formal Arbor node structure. Do NOT cap the output count—extract every distinct technical fact, code syntax rule, and invariant present:
  h_n = hypothesis (verifiable claim, preference, syntax rule, or mechanism tested)
  ι_n = causal_insight (2-3 concise sentences: what was tried, what happened, and WHY)
  item_type = 'direction' (if this is a user instruction, preference, or workflow constraint), 'insight' (if causal mechanism or observation), or 'fact'
  r_n = raw_evidence (observable facts, metrics, code snippets, or log snippets)
  μ_n = artifact_refs (referenced file paths, code symbols, or commit hashes)

OUTPUT SCHEMA (Strict JSON):
{
  "facts": [
    {
      "hypothesis": "string",
      "causal_insight": "string",
      "item_type": "direction|insight|fact",
      "raw_evidence": ["string"],
      "artifact_refs": ["string"],
      "metacognitive_confidence": 0-100
    }
  ]
}"#.to_string();

    let user = format!("TRANSCRIPT:\n{}", transcript);
    (system, user)
}

pub fn build_document_extraction_prompt(content: &str, vault_path: &str) -> (String, String) {
    let system = r#"You are a technical document and syntax analyst. Given a document (plan, spec, architecture doc, or web documentation), extract all atomic technical facts, syntax rules, API signatures, database schema definitions, and design constraints following the formal Arbor node structure. Extract every distinct technical rule, syntax pattern, and invariant present:
  h_n = hypothesis (technical claim, syntax rule, schema definition, or invariant)
  ι_n = causal_insight (what rule or design was chosen, how it operates, and WHY)
  r_n = raw_evidence (exact syntax examples, code blocks, or quotes from the document)
  μ_n = artifact_refs (vault page paths or code symbols referenced)

OUTPUT SCHEMA (Strict JSON):
{
  "facts": [
    {
      "hypothesis": "string",
      "causal_insight": "string",
      "raw_evidence": ["string"],
      "artifact_refs": ["string"],
      "metacognitive_confidence": 0-100
    }
  ]
}"#.to_string();

    let user = format!("VAULT PATH: {}\nCONTENT:\n{}", vault_path, content);
    (system, user)
}

pub fn build_code_extraction_prompt(code_content: &str, file_path: &str) -> (String, String) {
    let system = r#"You are a codebase and syntax analyst. Given a source code file (.rs, .py, .ts, .go, etc.), extract all structural facts, syntax patterns, API signatures, database schema definitions, conventions, and invariants. Capture both high-level design decisions and specific technical syntax rules and mechanisms.

OUTPUT SCHEMA (Strict JSON):
{
  "facts": [
    {
      "hypothesis": "string",
      "causal_insight": "string",
      "raw_evidence": ["string"],
      "artifact_refs": ["string"],
      "metacognitive_confidence": 0-100
    }
  ]
}"#.to_string();

    let user = format!("FILE PATH: {}\nSOURCE CODE:\n{}", file_path, code_content);
    (system, user)
}

pub fn build_forge_extraction_prompt(section_content: &str, source_path: &str) -> (String, String) {
    let system = r#"You are a reference document analyst. Given a section of a technical paper, specification, API reference, or syntax documentation, extract all core technical knowledge, syntax rules, formal definitions, constraints, algorithms, and rationale.

OUTPUT SCHEMA (Strict JSON):
{
  "facts": [
    {
      "hypothesis": "string",
      "causal_insight": "string",
      "raw_evidence": ["string"],
      "artifact_refs": ["string"],
      "metacognitive_confidence": 0-100
    }
  ]
}"#.to_string();

    let user = format!("REFERENCE SOURCE: {}\nSECTION CONTENT:\n{}", source_path, section_content);
    (system, user)
}

pub fn build_skill_extraction_prompt(skill_content: &str, skill_path: &str) -> (String, String) {
    let system = r#"You are a skill analyst. Given a developer skill file (SKILL.md), extract operational facts capturing prescribed workflows, rules, constraints, tool invocation patterns, and compounding requirements.

OUTPUT SCHEMA (Strict JSON):
{
  "facts": [
    {
      "hypothesis": "string",
      "causal_insight": "string",
      "raw_evidence": ["string"],
      "artifact_refs": ["string"],
      "metacognitive_confidence": 0-100
    }
  ]
}"#.to_string();

    let user = format!("SKILL PATH: {}\nCONTENT:\n{}", skill_path, skill_content);
    (system, user)
}

pub fn build_hypothesis_formation_prompt(facts_summary: &str, pruned_constraints: &[String]) -> (String, String) {
    let constraints_str = if pruned_constraints.is_empty() {
        "None".to_string()
    } else {
        pruned_constraints.join("\n- ")
    };

    let system = format!(r#"You are a cognitive synthesizer. Given a cluster of topically coherent facts, form a generalized, testable hypothesis (claim) and distill a unified insight explaining WHY it holds.
Classify item_type as 'direction' if the cluster reflects a user directive, preference, or workflow constraint, 'rule' if universal wisdom, or 'insight'.

CRITICAL POLICY MANDATE: You MUST NOT propose hypotheses that violate these known-false negative policy constraints derived from pruned past attempts:
{}

OUTPUT SCHEMA (Strict JSON):
{{
  "hypotheses": [
    {{
      "claim": "string",
      "insight": "string",
      "item_type": "direction|insight|rule",
      "fact_indices": [0, 1, 2]
    }}
  ]
}}"#, constraints_str);

    let user = format!("FACT CLUSTER:\n{}", facts_summary);
    (system, user)
}

pub fn build_refinement_prompt(claim: &str, insight: &str, current_confidence: f32, new_fact_summary: &str) -> (String, String) {
    let system = r#"You are an HTR cognitive evaluator. Given an existing Hypothesis (with its current claim, insight, and confidence score) and a newly extracted Fact, evaluate their relationship:
- SUPPORT: The new fact confirms, expands, or strengthens the hypothesis.
- CONTRADICT: The new fact refutes, invalidates, or exposes a flaw in the hypothesis.
- IRRELEVANT: The new fact is unrelated.

OUTPUT SCHEMA (Strict JSON):
{
  "action": "support|contradict|irrelevant",
  "new_confidence": 0.0-1.0,
  "refined_insight": "string",
  "reasoning": "string"
}"#.to_string();

    let user = format!(
        "HYPOTHESIS CLAIM: {}\nCURRENT INSIGHT: {}\nCURRENT CONFIDENCE: {}\nNEW FACT:\n{}",
        claim, insight, current_confidence, new_fact_summary
    );
    (system, user)
}

pub fn build_ancestor_merge_prompt(validated_insights_summary: &str, scope: &str) -> (String, String) {
    let system = r#"You are a principal technical writer and memory synthesizer. Given a set of validated hypotheses (confidence >= 0.90) and their supporting child insights (ι_n), synthesize them into an ancestor understanding formatted as a clean Markdown wiki page.

Do NOT perform simple text concatenation. Abstract over the child insights to produce a cohesive, authoritative document. Suggest a vault file path.

OUTPUT SCHEMA (Strict JSON):
{
  "suggested_path": "string",
  "title": "string",
  "markdown_content": "string"
}"#.to_string();

    let user = format!("SCOPE: {}\nVALIDATED HYPOTHESES & CHILD INSIGHTS:\n{}", scope, validated_insights_summary);
    (system, user)
}

pub fn build_graduation_prompt(title: &str, content: &str) -> (String, String) {
    let system = r#"You are a scope evaluator. Given a merged wiki node and its insight, evaluate whether this knowledge is:
- PROJECT_SPECIFIC: Applies only to this specific repository or codebase.
- UNIVERSAL: A generalized pattern, user preference, or system constraint that applies across all projects.

OUTPUT SCHEMA (Strict JSON):
{
  "scope": "project_specific|universal",
  "reasoning": "string"
}"#.to_string();

    let user = format!("TITLE: {}\nCONTENT:\n{}", title, content);
    (system, user)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_builders_and_schema_deserialization() {
        let (sys, user) = build_episode_extraction_prompt("User turn");
        assert!(sys.contains("Arbor leaf-insight extractor"));
        assert!(user.contains("User turn"));

        let mock_extract_json = r#"{
            "facts": [
                {
                    "hypothesis": "Test claim",
                    "causal_insight": "Tried X, got Y because Z",
                    "raw_evidence": ["Log output"],
                    "artifact_refs": ["src/main.rs"],
                    "metacognitive_confidence": 95
                }
            ]
        }"#;
        let parsed: ExtractFactsResponse = serde_json::from_str(mock_extract_json).unwrap();
        assert_eq!(parsed.facts.len(), 1);
        assert_eq!(parsed.facts[0].hypothesis, "Test claim");

        let mock_form_json = r#"{
            "hypotheses": [
                {
                    "claim": "General claim",
                    "insight": "Unified insight",
                    "fact_indices": [0, 1]
                }
            ]
        }"#;
        let form_parsed: FormHypothesesResponse = serde_json::from_str(mock_form_json).unwrap();
        assert_eq!(form_parsed.hypotheses.len(), 1);

        let mock_refine_json = r#"{
            "action": "support",
            "new_confidence": 0.95,
            "refined_insight": "Better insight",
            "reasoning": "Direct match"
        }"#;
        let refine_parsed: RefineHypothesisResponse = serde_json::from_str(mock_refine_json).unwrap();
        assert_eq!(refine_parsed.action, "support");

        let mock_merge_json = r##"{
            "suggested_path": "wiki/test/topic.md",
            "title": "Topic Synthesis",
            "markdown_content": "# Topic"
        }"##;
        let merge_parsed: AncestorMergeResponse = serde_json::from_str(mock_merge_json).unwrap();
        assert_eq!(merge_parsed.title, "Topic Synthesis");

        let mock_grad_json = r#"{
            "scope": "universal",
            "reasoning": "Applies across all projects"
        }"#;
        let grad_parsed: GraduationResponse = serde_json::from_str(mock_grad_json).unwrap();
        assert_eq!(grad_parsed.scope, "universal");
    }
}

pub fn clean_json_payload(raw: &str) -> String {
    let trimmed = raw.trim();
    let stripped = if trimmed.starts_with("```") {
        let lines: Vec<&str> = trimmed.lines().collect();
        if lines.len() >= 2 && lines.last().map(|l| l.trim()).unwrap_or("") == "```" {
            lines[1..lines.len() - 1].join("\n")
        } else {
            trimmed.trim_matches('`').trim().to_string()
        }
    } else {
        trimmed.to_string()
    };

    if let (Some(start), Some(end)) = (stripped.find('{'), stripped.rfind('}')) {
        if start <= end {
            return stripped[start..=end].to_string();
        }
    }
    stripped
}

