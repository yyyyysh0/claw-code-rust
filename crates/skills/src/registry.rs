//! Skill registry — store, query, and match skills.

use std::collections::HashMap;
use std::sync::Arc;
use std::path::PathBuf;

use tracing::{debug, info};

use crate::{Skill, SkillTrigger, TriggerMatch};
use crate::skill::TriggerKind;
use crate::{SkillLoader, LoadError};

/// A matched skill with activation context.
#[derive(Debug, Clone)]
pub struct SkillMatch {
    /// The matched skill.
    pub skill: Arc<Skill>,
    
    /// How the skill was matched.
    pub trigger_match: TriggerMatch,
    
    /// Match score (higher = better match).
    pub score: f32,
}

/// Registry for all loaded skills.
pub struct SkillRegistry {
    /// All registered skills.
    skills: HashMap<String, Arc<Skill>>,
    
    /// Skills sorted by priority.
    priority_order: Vec<Arc<Skill>>,
    
    /// Index of pattern triggers for fast matching.
    pattern_index: Vec<(regex::Regex, Arc<Skill>)>,
    
    /// Index of command triggers.
    command_index: HashMap<String, Arc<Skill>>,
    
    /// Index of keyword triggers.
    keyword_index: HashMap<String, Vec<Arc<Skill>>>,
}

impl SkillRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
            priority_order: Vec::new(),
            pattern_index: Vec::new(),
            command_index: HashMap::new(),
            keyword_index: HashMap::new(),
        }
    }
    
    /// Load skills from a directory.
    pub fn load_from_dir(&mut self, dir: impl Into<PathBuf>) -> Result<usize, LoadError> {
        let loader = SkillLoader::new(dir);
        let loaded = loader.load_all()?;
        let count = loaded.len();
        
        for skill in loaded {
            self.register(Arc::new(skill));
        }
        
        info!(count, "loaded skills from directory");
        Ok(count)
    }
    
    /// Register a skill.
    pub fn register(&mut self, skill: Arc<Skill>) {
        let name = skill.name.clone();
        
        // Add to main map
        self.skills.insert(name.clone(), Arc::clone(&skill));
        
        // Add to priority order
        self.priority_order.push(Arc::clone(&skill));
        self.priority_order.sort_by(|a, b| b.metadata.priority.cmp(&a.metadata.priority));
        
        // Build trigger indexes
        for trigger in &skill.metadata.triggers {
            match trigger {
                SkillTrigger::PatternMatch { pattern, case_insensitive } => {
                    let re_str = if *case_insensitive {
                        format!("(?i){}", pattern)
                    } else {
                        pattern.clone()
                    };
                    
                    if let Ok(re) = regex::Regex::new(&re_str) {
                        self.pattern_index.push((re, Arc::clone(&skill)));
                    }
                }
                
                SkillTrigger::SlashCommand { command, alias } => {
                    self.command_index.insert(command.clone(), Arc::clone(&skill));
                    for a in alias {
                        self.command_index.insert(a.clone(), Arc::clone(&skill));
                    }
                }
                
                SkillTrigger::Keywords { keywords } => {
                    for kw in keywords {
                        let kw_lower = kw.to_lowercase();
                        self.keyword_index
                            .entry(kw_lower)
                            .or_default()
                            .push(Arc::clone(&skill));
                    }
                }
                
                _ => {}
            }
        }
        
        debug!(name, priority = skill.metadata.priority, "registered skill");
    }
    
    /// Get a skill by name.
    pub fn get(&self, name: &str) -> Option<Arc<Skill>> {
        self.skills.get(name).map(Arc::clone)
    }
    
    /// Find skills that match the given input.
    pub fn find_matches(&self, input: &str) -> Vec<SkillMatch> {
        let mut matches: Vec<SkillMatch> = Vec::new();
        let input_lower = input.to_lowercase();
        let trimmed = input.trim();
        
        // Check command triggers (highest priority)
        if trimmed.starts_with('/') {
            if let Some(skill) = self.command_index.get(trimmed) {
                matches.push(SkillMatch {
                    skill: Arc::clone(skill),
                    trigger_match: TriggerMatch {
                        matched_text: trimmed.to_string(),
                        start: 0,
                        end: input.len(),
                        captures: vec![trimmed.to_string()],
                        trigger_kind: TriggerKind::Command,
                    },
                    score: 100.0, // Commands have highest score
                });
            }
        }
        
        // Check pattern triggers
        for (re, skill) in &self.pattern_index {
            if let Some(m) = re.find(input) {
                let captures: Vec<String> = re
                    .captures(input)
                    .map(|c| c.iter()
                        .filter_map(|g| g.map(|g| g.as_str().to_string()))
                        .collect())
                    .unwrap_or_default();
                
                matches.push(SkillMatch {
                    skill: Arc::clone(skill),
                    trigger_match: TriggerMatch {
                        matched_text: m.as_str().to_string(),
                        start: m.start(),
                        end: m.end(),
                        captures,
                        trigger_kind: TriggerKind::Pattern,
                    },
                    score: skill.metadata.priority as f32,
                });
            }
        }
        
        // Check keyword triggers
        for (kw, skills) in &self.keyword_index {
            if input_lower.contains(kw) {
                for skill in skills {
                    if let Some(pos) = input_lower.find(kw) {
                        matches.push(SkillMatch {
                            skill: Arc::clone(skill),
                            trigger_match: TriggerMatch {
                                matched_text: kw.clone(),
                                start: pos,
                                end: pos + kw.len(),
                                captures: vec![],
                                trigger_kind: TriggerKind::Keyword,
                            },
                            score: skill.metadata.priority as f32 * 0.5,
                        });
                    }
                }
            }
        }
        
        // Deduplicate and sort by score
        matches.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        
        // Remove duplicates (same skill multiple triggers)
        let mut seen: HashMap<String, bool> = HashMap::new();
        matches.retain(|m| {
            if seen.contains_key(&m.skill.name) {
                false
            } else {
                seen.insert(m.skill.name.clone(), true);
                true
            }
        });
        
        matches
    }
    
    /// Get the best matching skill.
    pub fn find_best_match(&self, input: &str) -> Option<SkillMatch> {
        self.find_matches(input).first().cloned()
    }
    
    /// List all registered skills.
    pub fn list(&self) -> Vec<Arc<Skill>> {
        self.priority_order.clone()
    }
    
    /// Count of registered skills.
    pub fn count(&self) -> usize {
        self.skills.len()
    }
    
    /// Enable/disable a skill by name.
    pub fn set_enabled(&mut self, name: &str, enabled: bool) -> bool {
        if let Some(skill) = self.skills.get(name) {
            let skill_arc = Arc::clone(skill);
            let mut new_skill = (*skill_arc).clone();
            new_skill.enabled = enabled;
            let new_arc = Arc::new(new_skill);
            
            self.skills.insert(name.to_string(), Arc::clone(&new_arc));

            // Update priority_order too
            for i in 0..self.priority_order.len() {
                if self.priority_order[i].name == name {
                    self.priority_order[i] = Arc::clone(&new_arc);
                }
            }
            true
        } else {
            false
        }
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SkillMetadata;
    
    fn make_test_skill(name: &str, triggers: Vec<SkillTrigger>, priority: u32) -> Skill {
        Skill {
            name: name.to_string(),
            description: "Test skill".to_string(),
            prompt: "Test prompt".to_string(),
            metadata: SkillMetadata {
                triggers,
                priority,
                ..Default::default()
            },
            source: None,
            enabled: true,
        }
    }
    
    #[test]
    fn register_and_find_skill() {
        let mut registry = SkillRegistry::new();
        
        let skill = make_test_skill("review", vec![
            SkillTrigger::SlashCommand { command: "/review".to_string(), alias: vec!["/r".to_string()] },
        ], 5);
        
        registry.register(Arc::new(skill));
        
        assert!(registry.get("review").is_some());
        assert_eq!(registry.count(), 1);
    }
    
    #[test]
    fn match_command_trigger() {
        let mut registry = SkillRegistry::new();
        
        let skill = make_test_skill("commit", vec![
            SkillTrigger::SlashCommand { command: "/commit".to_string(), alias: vec![] },
        ], 8);
        
        registry.register(Arc::new(skill));
        
        let matches = registry.find_matches("/commit");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].skill.name, "commit");
        assert_eq!(matches[0].score, 100.0);
    }
    
    #[test]
    fn match_pattern_trigger() {
        let mut registry = SkillRegistry::new();
        
        let skill = make_test_skill("code-helper", vec![
            SkillTrigger::PatternMatch { 
                pattern: r"help\s+me\s+(.+)".to_string(),
                case_insensitive: true,
            },
        ], 7);
        
        registry.register(Arc::new(skill));
        
        let matches = registry.find_matches("Help me debug this code");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].skill.name, "code-helper");
    }
    
    #[test]
    fn match_keyword_trigger() {
        let mut registry = SkillRegistry::new();
        
        let skill = make_test_skill("git-helper", vec![
            SkillTrigger::Keywords {
                keywords: vec!["git".to_string(), "commit".to_string(), "push".to_string()],
            },
        ], 6);
        
        registry.register(Arc::new(skill));
        
        let matches = registry.find_matches("I need to git push my changes");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].skill.name, "git-helper");
    }
    
    #[test]
    fn priority_ordering() {
        let mut registry = SkillRegistry::new();
        
        let low_skill = make_test_skill("low", vec![], 1);
        let high_skill = make_test_skill("high", vec![], 10);
        
        registry.register(Arc::new(low_skill));
        registry.register(Arc::new(high_skill));
        
        let list = registry.list();
        assert_eq!(list[0].name, "high"); // Higher priority first
        assert_eq!(list[1].name, "low");
    }
}