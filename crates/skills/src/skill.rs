//! Core Skill definition and metadata.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// A skill is a text-based agent extension defined in Markdown.
///
/// Skills are loaded from `.md` files with YAML frontmatter:
///
/// ```markdown
/// ---
/// name: code-review
/// description: Review code for quality and security issues
/// triggers:
///   - pattern: "review.*code"
///   - command: /review
/// tools: [file_read, grep, glob]
/// context_files:
///   - .claude/rules.md
///   - README.md
/// priority: 10
/// ---
///
/// You are a code reviewer. Analyze the following code...
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    /// Unique skill identifier.
    pub name: String,
    
    /// Human-readable description.
    pub description: String,
    
    /// The skill prompt template (Markdown body after frontmatter).
    pub prompt: String,
    
    /// Metadata from YAML frontmatter.
    #[serde(flatten)]
    pub metadata: SkillMetadata,
    
    /// Source file path (if loaded from disk).
    #[serde(skip)]
    pub source: Option<PathBuf>,
    
    /// Whether this skill is currently enabled.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool { true }

/// Metadata extracted from YAML frontmatter.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillMetadata {
    /// Version string for skill updates.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    
    /// Author information.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author: String,
    
    /// Priority for skill ordering (higher = more important).
    #[serde(default = "default_priority")]
    pub priority: u32,
    
    /// List of tool names this skill is allowed to use.
    /// If empty, all tools are allowed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    
    /// Tools explicitly denied for this skill.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub denied_tools: Vec<String>,
    
    /// Files to prefetch when skill activates.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_files: Vec<String>,
    
    /// Conditions that trigger this skill.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<SkillTrigger>,
    
    /// Tags for categorization and search.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

fn default_priority() -> u32 { 5 }

/// Conditions that can activate a skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SkillTrigger {
    /// Regex pattern match against user input.
    #[serde(rename = "pattern")]
    PatternMatch {
        pattern: String,
        #[serde(default)]
        case_insensitive: bool,
    },
    
    /// Slash command trigger (e.g., `/review`).
    #[serde(rename = "command")]
    SlashCommand {
        command: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        alias: Vec<String>,
    },
    
    /// File path trigger (activated when matching files exist).
    #[serde(rename = "file_path")]
    FileGlob {
        glob: String,
    },
    
    /// Keyword presence in conversation.
    #[serde(rename = "keyword")]
    Keywords {
        keywords: Vec<String>,
    },
    
    /// Manual activation only.
    #[serde(rename = "manual")]
    ManualOnly,
    
    /// Custom condition evaluated by plugin.
    #[serde(rename = "custom")]
    CustomCondition {
        condition: String,
        #[serde(default, skip_serializing_if = "HashMap::is_empty")]
        params: HashMap<String, serde_json::Value>,
    },
}

/// Runtime context passed to an activated skill.
#[derive(Debug, Clone)]
pub struct SkillContext {
    /// The session ID.
    pub session_id: String,
    
    /// Working directory.
    pub cwd: PathBuf,
    
    /// The user input that triggered this skill.
    pub trigger_input: String,
    
    /// Matched trigger details.
    pub trigger_match: TriggerMatch,
    
    /// Preloaded context file contents.
    pub context_contents: HashMap<PathBuf, String>,
    
    /// Additional variables for prompt interpolation.
    pub variables: HashMap<String, String>,
}

/// Details about how a trigger matched.
#[derive(Debug, Clone)]
pub struct TriggerMatch {
    /// Matched text or pattern.
    pub matched_text: String,
    
    /// Start position in input.
    pub start: usize,
    
    /// End position in input.
    pub end: usize,
    
    /// Captured groups (for regex patterns).
    pub captures: Vec<String>,
    
    /// The trigger kind that matched.
    pub trigger_kind: TriggerKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerKind {
    Command,
    Pattern,
    Keyword,
    Manual,
}

impl Skill {
    /// Create a minimal skill with just name and prompt.
    pub fn simple(name: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            prompt: prompt.into(),
            metadata: SkillMetadata::default(),
            source: None,
            enabled: true,
        }
    }
    
    /// Parse from Markdown content with YAML frontmatter.
    pub fn from_markdown(content: &str) -> Option<Self> {
        let content = content.trim();
        
        if !content.starts_with("---") {
            return None;
        }
        
        // Find closing delimiter
        let end_marker_pos = content[3..]
            .find("\n---")
            .map(|p| p + 3);
        
        let end_pos = match end_marker_pos {
            Some(p) => p + 4,
            None => return None,
        };
        
        let frontmatter = &content[3..end_pos - 4];
        let body = content[end_pos..].trim();
        
        // Parse YAML
        let metadata: SkillMetadata = serde_yaml::from_str(frontmatter)
            .map_err(|e| {
                tracing::warn!("Failed to parse skill frontmatter: {}", e);
                e
            })
            .ok()?;
        
        // Name is required
        if metadata.tags.is_empty() && body.is_empty() {
            return None;
        }
        
        Some(Self {
            name: content.lines().nth(1)
                .and_then(|l| l.strip_prefix("name:"))
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| "unnamed-skill".to_string()),
            description: String::new(),
            prompt: body.to_string(),
            metadata,
            source: None,
            enabled: true,
        })
    }
    
    /// Check if this skill can use a specific tool.
    pub fn can_use_tool(&self, tool_name: &str) -> bool {
        // Denied tools always block
        if self.metadata.denied_tools.iter().any(|t| t == tool_name) {
            return false;
        }
        
        // If tools list is empty, all (non-denied) tools are allowed
        if self.metadata.tools.is_empty() {
            return true;
        }
        
        // Otherwise, must be in allowed list
        self.metadata.tools.iter().any(|t| t == tool_name)
    }
    
    /// Check if this skill should trigger for given input.
    pub fn check_trigger(&self, input: &str) -> Option<TriggerMatch> {
        if !self.enabled {
            return None;
        }
        
        for trigger in &self.metadata.triggers {
            match trigger {
                SkillTrigger::SlashCommand { command, alias } => {
                    let trimmed = input.trim();
                    if trimmed == command || alias.iter().any(|a| trimmed == a) {
                        return Some(TriggerMatch {
                            matched_text: trimmed.to_string(),
                            start: 0,
                            end: input.len(),
                            captures: vec![trimmed.to_string()],
                            trigger_kind: TriggerKind::Command,
                        });
                    }
                }
                
                SkillTrigger::PatternMatch { pattern, case_insensitive } => {
                    let re_str = if *case_insensitive {
                        format!("(?i){}", pattern)
                    } else {
                        pattern.clone()
                    };
                    
                    if let Ok(re) = regex::Regex::new(&re_str) {
                        if let Some(m) = re.find(input) {
                            let captures: Vec<String> = re
                                .captures(input)
                                .map(|c| c.iter()
                                    .filter_map(|g| g.map(|g| g.as_str().to_string()))
                                    .collect())
                                .unwrap_or_default();
                            
                            return Some(TriggerMatch {
                                matched_text: m.as_str().to_string(),
                                start: m.start(),
                                end: m.end(),
                                captures,
                                trigger_kind: TriggerKind::Pattern,
                            });
                        }
                    }
                }
                
                SkillTrigger::Keywords { keywords } => {
                    let input_lower = input.to_lowercase();
                    for kw in keywords {
                        let kw_lower = kw.to_lowercase();
                        if let Some(pos) = input_lower.find(&kw_lower) {
                            return Some(TriggerMatch {
                                matched_text: kw.clone(),
                                start: pos,
                                end: pos + kw.len(),
                                captures: vec![kw.clone()],
                                trigger_kind: TriggerKind::Keyword,
                            });
                        }
                    }
                }
                
                SkillTrigger::ManualOnly => {
                    // Manual skills don't auto-trigger
                }
                
                SkillTrigger::FileGlob { .. } | SkillTrigger::CustomCondition { .. } => {
                    // These require special handling
                }
            }
        }
        
        None
    }
    
    /// Render the prompt with context interpolation.
    pub fn render_prompt(&self, ctx: &SkillContext) -> String {
        let mut result = self.prompt.clone();
        
        // Replace {{variable}} placeholders
        for (key, value) in &ctx.variables {
            result = result.replace(&format!("{{{{{}}}}}", key), value);
        }
        
        // Replace {{trigger_input}}
        result = result.replace("{{trigger_input}}", &ctx.trigger_input);
        
        // Replace {{cwd}}
        result = result.replace("{{cwd}}", ctx.cwd.to_string_lossy().as_ref());
        
        // Replace {{context_files}} with concatenated contents
        if result.contains("{{context_files}}") {
            let files_content = ctx.context_contents
                .iter()
                .map(|(path, content)| format!("--- {} ---\n{}", path.display(), content))
                .collect::<Vec<_>>()
                .join("\n\n");
            result = result.replace("{{context_files}}", &files_content);
        }
        
        result
    }
}