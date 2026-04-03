//! Claw Skills — Text-based agent extensions.
//!
//! Skills are Markdown files with YAML frontmatter that define:
//! - Prompts to inject into the conversation
//! - Tools the skill can use
//! - Triggers that activate the skill
//! - Context files to prefetch
//!
//! This mirrors Claude Code's `src/skills/` system but redesigned for Rust.

mod skill;
mod loader;
mod registry;
mod activation;
mod resolver;

pub use skill::{Skill, SkillMetadata, SkillTrigger, SkillContext, TriggerMatch};
pub use loader::{SkillLoader, LoadError};
pub use registry::{SkillRegistry, SkillMatch};
pub use activation::{SkillActivator, ActivationResult, SessionSkillExt};
pub use resolver::{SkillResolver, ResolvedSkill};

/// Register all built-in skills into a registry.
pub fn register_builtin_skills(registry: &mut SkillRegistry) {
    // Built-in skills are embedded at compile time
    let code_review = Skill::from_markdown(include_str!("builtin/code_review.md"));
    let git_commit = Skill::from_markdown(include_str!("builtin/git_commit.md"));
    let refactor = Skill::from_markdown(include_str!("builtin/refactor.md"));
    
    if let Some(s) = code_review {
        registry.register(std::sync::Arc::new(s));
    }
    if let Some(s) = git_commit {
        registry.register(std::sync::Arc::new(s));
    }
    if let Some(s) = refactor {
        registry.register(std::sync::Arc::new(s));
    }
}