//! Skill activation — inject skill context into conversation.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use tracing::{debug, info};

use claw_core::{Message, ContentBlock, Role};
use crate::{Skill, SkillContext, SkillMatch, TriggerMatch, SkillRegistry};

/// Result of activating a skill.
#[derive(Debug)]
pub struct ActivationResult {
    /// The system prompt to prepend.
    pub system_prompt: String,
    
    /// User message containing the skill prompt.
    pub skill_message: Message,
    
    /// Context files that were loaded.
    pub loaded_files: HashMap<PathBuf, String>,
    
    /// Tools that should be available for this skill.
    pub available_tools: Vec<String>,
    
    /// Tools that should be blocked for this skill.
    pub blocked_tools: Vec<String>,
}

/// Activates skills and prepares their context.
pub struct SkillActivator {
    /// Skill registry for lookup.
    registry: Arc<SkillRegistry>,
    
    /// Working directory for resolving context files.
    cwd: PathBuf,
}

impl SkillActivator {
    /// Create a new activator.
    pub fn new(registry: Arc<SkillRegistry>, cwd: PathBuf) -> Self {
        Self {
            registry,
            cwd,
        }
    }
    
    /// Activate a single skill match.
    pub async fn activate(&self, match_: SkillMatch) -> Result<ActivationResult> {
        let skill = &match_.skill;
        info!(name = %skill.name, "activating skill");
        
        // Load context files
        let loaded_files = self.load_context_files(skill).await?;
        
        // Determine available and blocked tools
        // (This would normally use ToolRegistry, but we keep it simple here)
        let available_tools: Vec<String> = if skill.metadata.tools.is_empty() {
            vec![] // Empty means all tools allowed
        } else {
            skill.metadata.tools.clone()
        };
        
        let blocked_tools = skill.metadata.denied_tools.clone();
        
        // Build skill context
        let skill_ctx = SkillContext {
            session_id: uuid::Uuid::new_v4().to_string(),
            cwd: self.cwd.clone(),
            trigger_input: match_.trigger_match.matched_text.clone(),
            trigger_match: match_.trigger_match.clone(),
            context_contents: loaded_files.clone(),
            variables: HashMap::new(),
        };
        
        // Render the prompt
        let rendered_prompt = skill.render_prompt(&skill_ctx);
        
        // Create skill message
        let skill_message = Message {
            role: Role::User,
            content: vec![ContentBlock::Text { text: rendered_prompt }],
        };
        
        // Build system prompt
        let system_prompt = format!(
            "You are operating with the '{}' skill enabled.\n{}\n\n",
            skill.name,
            skill.description
        );
        
        debug!(
            name = %skill.name,
            available = available_tools.len(),
            blocked = blocked_tools.len(),
            "skill activated"
        );
        
        Ok(ActivationResult {
            system_prompt,
            skill_message,
            loaded_files,
            available_tools,
            blocked_tools,
        })
    }
    
    /// Activate multiple skills (combine their prompts).
    pub async fn activate_batch(&self, matches: Vec<SkillMatch>) -> Result<ActivationResult> {
        if matches.is_empty() {
            return Ok(ActivationResult {
                system_prompt: String::new(),
                skill_message: Message::user(""),
                loaded_files: HashMap::new(),
                available_tools: vec![],
                blocked_tools: vec![],
            });
        }
        
        // Combine multiple skills
        let mut combined_system = String::new();
        let mut combined_prompt = String::new();
        let mut all_loaded_files = HashMap::new();
        let mut all_available = Vec::new();
        let mut all_blocked = Vec::new();
        
        for match_ in matches {
            let result = self.activate(match_).await?;
            
            combined_system.push_str(&result.system_prompt);
            combined_system.push_str("\n");
            
            combined_prompt.push_str("---\n");
            combined_prompt.push_str(&result.skill_message.content.iter()
                .filter_map(|b| match b {
                    ContentBlock::Text { text } => Some(text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n"));
            combined_prompt.push_str("\n\n");
            
            all_loaded_files.extend(result.loaded_files);
            all_available.extend(result.available_tools);
            all_blocked.extend(result.blocked_tools);
        }
        
        // Deduplicate tools
        all_available.sort();
        all_available.dedup();
        all_blocked.sort();
        all_blocked.dedup();
        
        // Remove tools that are both available and blocked
        all_available.retain(|t| !all_blocked.contains(t));
        
        Ok(ActivationResult {
            system_prompt: combined_system,
            skill_message: Message {
                role: Role::User,
                content: vec![ContentBlock::Text { text: combined_prompt }],
            },
            loaded_files: all_loaded_files,
            available_tools: all_available,
            blocked_tools: all_blocked,
        })
    }
    
    /// Load context files specified by a skill.
    async fn load_context_files(&self, skill: &Skill) -> Result<HashMap<PathBuf, String>> {
        let mut files = HashMap::new();
        
        for file_spec in &skill.metadata.context_files {
            let path = if file_spec.starts_with('/') {
                PathBuf::from(file_spec)
            } else {
                self.cwd.join(file_spec)
            };
            
            if path.exists() {
                match tokio::fs::read_to_string(&path).await {
                    Ok(content) => {
                        debug!(path = %path.display(), "loaded context file");
                        files.insert(path, content);
                    }
                    Err(e) => {
                        debug!(path = %path.display(), error = %e, "failed to load context file");
                    }
                }
            }
        }
        
        Ok(files)
    }
    
    /// Prefetch skills based on user input (skill prefetch like Claude Code).
    pub fn prefetch(&self, input: &str) -> Vec<SkillMatch> {
        self.registry.find_matches(input)
    }
    
    /// Check if a skill should auto-activate.
    pub fn should_auto_activate(&self, input: &str) -> Option<SkillMatch> {
        let matches = self.registry.find_matches(input);
        
        // Auto-activate only for high-confidence matches
        matches
            .iter()
            .find(|m| m.score >= 50.0) // Threshold for auto-activation
            .cloned()
    }
}

/// Extension trait to add skill support to SessionState.
pub trait SessionSkillExt {
    /// Activate a skill and inject its context.
    fn apply_skill_activation(&mut self, result: ActivationResult);
}

impl SessionSkillExt for claw_core::SessionState {
    fn apply_skill_activation(&mut self, result: ActivationResult) {
        // Prepend skill system prompt
        if !result.system_prompt.is_empty() {
            self.config.system_prompt = format!(
                "{}\n{}",
                result.system_prompt,
                self.config.system_prompt
            );
        }
        
        // Add skill message to conversation
        if !result.skill_message.content.is_empty() {
            self.push_message(result.skill_message);
        }
    }
}