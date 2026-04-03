//! Skill resolver — resolve skill references in prompts.

use std::path::PathBuf;
use std::sync::Arc;

use tracing::debug;

use crate::{Skill, SkillRegistry};

/// A resolved skill with all dependencies loaded.
#[derive(Debug, Clone)]
pub struct ResolvedSkill {
    /// The skill definition.
    pub skill: Arc<Skill>,
    
    /// Skills that this skill depends on (imports).
    pub dependencies: Vec<Arc<Skill>>,
    
    /// All combined prompts from skill and dependencies.
    pub combined_prompt: String,
    
    /// All combined tools from skill and dependencies.
    pub combined_tools: Vec<String>,
}

/// Resolve skill references and dependencies.
pub struct SkillResolver {
    /// Skill registry for lookup.
    registry: Arc<SkillRegistry>,
    
    /// Base directory for relative skill imports.
    base_dir: PathBuf,
}

impl SkillResolver {
    /// Create a new resolver.
    pub fn new(registry: Arc<SkillRegistry>, base_dir: PathBuf) -> Self {
        Self {
            registry,
            base_dir,
        }
    }
    
    /// Resolve a skill by name, including dependencies.
    pub fn resolve(&self, name: &str) -> Option<ResolvedSkill> {
        let skill = self.registry.get(name);
        
        if skill.is_none() {
            debug!(name, "skill not found for resolution");
            return None;
        }
        
        let skill = skill.unwrap();
        self.resolve_with_deps(skill)
    }
    
    /// Resolve a skill with its dependencies.
    fn resolve_with_deps(&self, skill: Arc<Skill>) -> Option<ResolvedSkill> {
        // Find imported skills from metadata (if any)
        let imports: Vec<String> = vec![]; // Would come from metadata.extra["imports"]
        
        // Resolve each import
        let mut dependencies = Vec::new();
        for import_name in imports {
            if let Some(dep) = self.registry.get(&import_name) {
                dependencies.push(dep);
            } else {
                debug!(import = %import_name, "dependency not found, skipping");
            }
        }
        
        // Combine prompts
        let mut combined_prompt = String::new();
        
        // Add dependency prompts first
        for dep in &dependencies {
            combined_prompt.push_str(&format!("--- {} ---\n", dep.name));
            combined_prompt.push_str(&dep.prompt);
            combined_prompt.push_str("\n\n");
        }
        
        // Then add main skill prompt
        combined_prompt.push_str(&skill.prompt);
        
        // Combine tools
        let mut combined_tools = skill.metadata.tools.clone();
        for dep in &dependencies {
            combined_tools.extend(dep.metadata.tools.iter().cloned());
        }
        combined_tools.sort();
        combined_tools.dedup();
        
        Some(ResolvedSkill {
            skill,
            dependencies,
            combined_prompt,
            combined_tools,
        })
    }
    
    /// Find skills referenced in a prompt.
    ///
    /// Looks for patterns:
    /// - `@skill:name` — inline skill reference
    /// - `{{skill:name}}` — template-style reference
    pub fn find_skill_references(&self, prompt: &str) -> Vec<String> {
        let mut names = Vec::new();
        
        // Pattern: @skill:name
        let re_inline = regex::Regex::new(r"@skill:([a-zA-Z0-9_-]+)").unwrap();
        for cap in re_inline.captures_iter(prompt) {
            if let Some(name) = cap.get(1) {
                names.push(name.as_str().to_string());
            }
        }
        
        // Pattern: {{skill:name}}
        let re_template = regex::Regex::new(r"\{\{skill:([a-zA-Z0-9_-]+)\}\}").unwrap();
        for cap in re_template.captures_iter(prompt) {
            if let Some(name) = cap.get(1) {
                names.push(name.as_str().to_string());
            }
        }
        
        names.sort();
        names.dedup();
        names
    }
    
    /// Expand skill references in a prompt, replacing them with actual content.
    pub fn expand_prompt(&self, prompt: &str) -> String {
        let mut result = prompt.to_string();
        
        // Find all referenced skills
        let names = self.find_skill_references(prompt);
        
        for name in names {
            if let Some(resolved) = self.resolve(&name) {
                // Replace @skill:name
                let inline_pattern = format!("@skill:{}", name);
                result = result.replace(&inline_pattern, &resolved.combined_prompt);
                
                // Replace {{skill:name}}
                let template_pattern = format!("{{{{skill:{}}}}}", name);
                result = result.replace(&template_pattern, &resolved.combined_prompt);
            }
        }
        
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SkillMetadata, SkillTrigger};
    
    #[test]
    fn resolve_skill_no_deps() {
        let mut registry = SkillRegistry::new();
        registry.register(Arc::new(Skill::simple("base", "Base prompt")));
        
        let resolver = SkillResolver::new(Arc::new(registry), PathBuf::new());
        
        let resolved = resolver.resolve("base").expect("should resolve");
        assert_eq!(resolved.skill.name, "base");
        assert_eq!(resolved.combined_prompt, "Base prompt");
        assert!(resolved.dependencies.is_empty());
    }
    
    #[test]
    fn expand_inline_reference() {
        let mut registry = SkillRegistry::new();
        registry.register(Arc::new(Skill::simple("helper", "Helper content here")));
        
        let resolver = SkillResolver::new(Arc::new(registry), PathBuf::new());
        
        let prompt = "Use @skill:helper to assist with the task.";
        let expanded = resolver.expand_prompt(prompt);
        
        assert!(expanded.contains("Helper content here"));
        assert!(!expanded.contains("@skill:helper"));
    }
    
    #[test]
    fn expand_template_reference() {
        let mut registry = SkillRegistry::new();
        registry.register(Arc::new(Skill::simple("reviewer", "Review guidelines")));
        
        let resolver = SkillResolver::new(Arc::new(registry), PathBuf::new());
        
        let prompt = "Apply {{skill:reviewer}} when checking code.";
        let expanded = resolver.expand_prompt(prompt);
        
        assert!(expanded.contains("Review guidelines"));
        assert!(!expanded.contains("{{skill:reviewer}}"));
    }
    
    #[test]
    fn find_skill_references() {
        let registry = SkillRegistry::new();
        let resolver = SkillResolver::new(Arc::new(registry), PathBuf::new());
        
        let prompt = "Use @skill:helper and {{skill:reviewer}} for this task.";
        let names = resolver.find_skill_references(prompt);
        
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"helper".to_string()));
        assert!(names.contains(&"reviewer".to_string()));
    }
}