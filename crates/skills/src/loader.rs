//! Skill loader — parse Markdown files with YAML frontmatter.

use std::path::{Path, PathBuf};
use std::fs;

use thiserror::Error;
use tracing::{debug, warn};

use crate::{Skill, SkillMetadata};

/// Errors that can occur while loading skills.
#[derive(Debug, Error)]
pub enum LoadError {
    #[error("IO error reading {path}: {source}")]
    Io { path: PathBuf, #[source] source: std::io::Error },
    
    #[error("Invalid YAML frontmatter in {path}: {source}")]
    YamlParse { path: PathBuf, #[source] source: serde_yaml::Error },
    
    #[error("Missing frontmatter delimiter in {path}")]
    MissingDelimiter { path: PathBuf },
    
    #[error("Missing required field 'name' in {path}")]
    MissingName { path: PathBuf },
    
    #[error("Glob pattern error: {source}")]
    GlobPattern { #[source] source: glob::PatternError },
}

/// Load skills from a directory.
pub struct SkillLoader {
    /// Base directory for skill files.
    base_dir: PathBuf,
    
    /// Whether to recurse into subdirectories.
    recursive: bool,
}

impl SkillLoader {
    /// Create a loader for a specific directory.
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
            recursive: true,
        }
    }
    
    /// Set whether to recurse into subdirectories.
    pub fn recursive(mut self, yes: bool) -> Self {
        self.recursive = yes;
        self
    }
    
    /// Load all skills from configured directory.
    pub fn load_all(&self) -> Result<Vec<Skill>, LoadError> {
        let mut skills = Vec::new();
        
        let pattern = if self.recursive {
            format!("{}/**/*.md", self.base_dir.display())
        } else {
            format!("{}/*.md", self.base_dir.display())
        };
        
        debug!(pattern = %pattern, "scanning for skills");

        for entry in glob::glob(&pattern).map_err(|source| LoadError::GlobPattern { source })? {
            let path = entry.map_err(|e| LoadError::Io {
                path: self.base_dir.clone(),
                source: e.into_error(),
            })?;
            
            if let Some(skill) = self.load_file(&path)? {
                skills.push(skill);
            }
        }
        
        // Sort by priority (higher first)
        skills.sort_by(|a, b| b.metadata.priority.cmp(&a.metadata.priority));
        
        debug!(count = skills.len(), "loaded skills");
        Ok(skills)
    }
    
    /// Load a single skill file.
    pub fn load_file(&self, path: &Path) -> Result<Option<Skill>, LoadError> {
        let content = fs::read_to_string(path).map_err(|e| LoadError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
        
        parse_skill_markdown(path, &content)
    }
}

/// Parse a skill from Markdown content with YAML frontmatter.
fn parse_skill_markdown(path: &Path, content: &str) -> Result<Option<Skill>, LoadError> {
    let content = content.trim();
    
    if !content.starts_with("---") {
        warn!(path = %path.display(), "no frontmatter found, skipping");
        return Ok(None);
    }
    
    // Find the closing delimiter
    let end_marker_pos = content[3..]
        .find("\n---")
        .map(|p| p + 3);
    
    let end_pos = match end_marker_pos {
        Some(p) => p + 4,
        None => {
            return Err(LoadError::MissingDelimiter {
                path: path.to_path_buf(),
            });
        }
    };
    
    // Extract frontmatter and body
    let frontmatter = &content[3..end_pos - 4];
    let body = content[end_pos..].trim();
    
    // Parse YAML frontmatter
    let mut metadata: SkillMetadata = serde_yaml::from_str(frontmatter).map_err(|e| {
        LoadError::YamlParse {
            path: path.to_path_buf(),
            source: e,
        }
    })?;
    
    // Extract name from frontmatter
    let raw_yaml: serde_yaml::Value = serde_yaml::from_str(frontmatter)
        .map_err(|e| LoadError::YamlParse {
            path: path.to_path_buf(),
            source: e,
        })?;
    
    let name = raw_yaml.get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            // Derive name from filename
            path.file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
        })
        .ok_or_else(|| LoadError::MissingName {
            path: path.to_path_buf(),
        })?;
    
    // Extract description
    let description = raw_yaml.get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();
    
    Ok(Some(Skill {
        name,
        description,
        prompt: body.to_string(),
        metadata,
        source: Some(path.to_path_buf()),
        enabled: true,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn parse_simple_skill() {
        let content = r#"---
name: test-skill
description: A test skill
priority: 10
---
This is the skill prompt.
"#;
        
        let skill = parse_skill_markdown(Path::new("test.md"), content)
            .expect("parse should succeed")
            .expect("skill should exist");
        
        assert_eq!(skill.name, "test-skill");
        assert_eq!(skill.description, "A test skill");
        assert!(skill.prompt.contains("skill prompt"));
        assert_eq!(skill.metadata.priority, 10);
    }
    
    #[test]
    fn parse_skill_with_tools() {
        let content = r#"---
name: file-reader
tools:
  - file_read
  - glob
  - grep
---
Read files and search.
"#;
        
        let skill = parse_skill_markdown(Path::new("file_reader.md"), content)
            .expect("parse should succeed")
            .expect("skill should exist");
        
        assert!(skill.can_use_tool("file_read"));
        assert!(skill.can_use_tool("glob"));
        assert!(!skill.can_use_tool("bash")); // Not in allowed list
    }
    
    #[test]
    fn parse_skill_with_denied_tools() {
        let content = r#"---
name: safe-reader
denied_tools:
  - bash
  - file_write
---
Only read, never write.
"#;
        
        let skill = parse_skill_markdown(Path::new("safe.md"), content)
            .expect("parse should succeed")
            .expect("skill should exist");
        
        assert!(skill.can_use_tool("file_read")); // Allowed (not denied)
        assert!(!skill.can_use_tool("bash")); // Denied
        assert!(!skill.can_use_tool("file_write")); // Denied
    }
    
    #[test]
    fn missing_frontmatter_returns_none() {
        let content = "Just some markdown without frontmatter.";
        
        let result = parse_skill_markdown(Path::new("plain.md"), content)
            .expect("parse should succeed");
        
        assert!(result.is_none());
    }
}