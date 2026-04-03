use std::io::{self, BufRead, Write};
use std::sync::Arc;

use anyhow::Result;
use clap::Parser;

use claw_core::{query, Message, QueryEvent, SessionConfig, SessionState};
use claw_permissions::PermissionMode;
use claw_tools::{ToolOrchestrator, ToolRegistry};
use claw_skills::{SkillActivator, SkillRegistry, SessionSkillExt};

mod config;
mod onboarding;

/// Output format for non-interactive (print/query) mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    /// Plain text — assistant text only, streamed to stdout.
    Text,
    /// Newline-delimited JSON events (one JSON object per line).
    StreamJson,
    /// Single JSON object written after the turn completes.
    Json,
}

impl std::str::FromStr for OutputFormat {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "text" => Ok(OutputFormat::Text),
            "stream-json" => Ok(OutputFormat::StreamJson),
            "json" => Ok(OutputFormat::Json),
            other => anyhow::bail!("unknown output format '{}' (text|stream-json|json)", other),
        }
    }
}

/// Claw RS — a modular agent runtime with skills support.
#[derive(Parser, Debug)]
#[command(name = "claw-rs", version, about)]
struct Cli {
    /// Model to use (e.g. claude-sonnet-4-20250514, qwen3.5:9b)
    #[arg(short, long)]
    model: Option<String>,

    /// System prompt
    #[arg(
        short,
        long,
        default_value = "You are a helpful coding assistant. \
        Use tools when appropriate to help the user. Be concise."
    )]
    system: String,

    /// Permission mode: auto, interactive, deny
    #[arg(short, long, default_value = "auto")]
    permission: String,

    /// Run a single prompt non-interactively then exit
    #[arg(short = 'q', long)]
    query: Option<String>,

    /// Run a single prompt non-interactively then exit (alias for --query)
    #[arg(long)]
    print: Option<String>,

    /// Output format for non-interactive mode: text (default), stream-json, json
    #[arg(long, default_value = "text")]
    output_format: OutputFormat,

    /// Maximum turns per conversation
    #[arg(long, default_value = "100")]
    max_turns: usize,

    /// Provider: anthropic, ollama, openai (auto-detected if not set)
    #[arg(long)]
    provider: Option<String>,

    /// Ollama server URL
    #[arg(long, default_value = "http://localhost:11434")]
    ollama_url: String,

    /// Skills directory to load skills from
    #[arg(long)]
    skills_dir: Option<String>,

    /// Disable skill auto-activation
    #[arg(long)]
    no_skills: bool,

    /// List available skills
    #[arg(long)]
    list_skills: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let cli = Cli::parse();
    let cwd = std::env::current_dir()?;

    // --print is an alias for --query; --query takes precedence if both given
    let single_prompt = cli.query.or(cli.print);
    let interactive = single_prompt.is_none();

    let permission_mode = match cli.permission.as_str() {
        "auto" => PermissionMode::AutoApprove,
        "interactive" => PermissionMode::Interactive,
        "deny" => PermissionMode::Deny,
        other => {
            eprintln!("unknown permission mode '{}', using auto", other);
            PermissionMode::AutoApprove
        }
    };

    // Register tools
    let mut registry = ToolRegistry::new();
    claw_tools::register_builtin_tools(&mut registry);
    let registry = Arc::new(registry);
    let orchestrator = ToolOrchestrator::new(Arc::clone(&registry));

    // Initialize skill registry
    let mut skill_registry = SkillRegistry::new();
    
    // Load built-in skills
    claw_skills::register_builtin_skills(&mut skill_registry);
    
    // Load skills from directory if specified
    if let Some(skills_dir) = &cli.skills_dir {
        skill_registry.load_from_dir(skills_dir)?;
    }
    
    // Also try loading from default skills directory (user home)
    let default_skills_dir = dirs::home_dir()
        .map(|h| h.join(".claude/skills"))
        .unwrap_or_else(|| cwd.join(".claude/skills"));
    
    if default_skills_dir.exists() {
        match skill_registry.load_from_dir(&default_skills_dir) {
            Ok(count) => eprintln!("Loaded {} custom skills from {:?}", count, default_skills_dir),
            Err(e) => eprintln!("Error loading skills from {:?}: {}", default_skills_dir, e),
        }
    }
    
    let skill_registry = Arc::new(skill_registry);
    let skill_activator = SkillActivator::new(Arc::clone(&skill_registry), cwd.clone());

    // Handle --list-skills
    if cli.list_skills {
        println!("Available Skills:");
        println!("================\n");
        for skill in skill_registry.list() {
            println!("  {} (priority: {})", skill.name, skill.metadata.priority);
            if !skill.description.is_empty() {
                println!("    {}", skill.description);
            }
            if !skill.metadata.triggers.is_empty() {
                println!("    triggers:");
                for trigger in &skill.metadata.triggers {
                    match trigger {
                        claw_skills::SkillTrigger::SlashCommand { command, alias } => {
                            println!("      - command: {} (aliases: {})", command, alias.join(", "));
                        }
                        claw_skills::SkillTrigger::PatternMatch { pattern, .. } => {
                            println!("      - pattern: {}", pattern);
                        }
                        claw_skills::SkillTrigger::Keywords { keywords } => {
                            println!("      - keywords: {}", keywords.join(", "));
                        }
                        _ => {}
                    }
                }
            }
            println!();
        }
        println!("Use skills with slash commands (e.g., /review) or let them auto-activate on keywords.");
        return Ok(());
    }

    // Resolve provider: CLI flags > env vars > config file > onboarding
    let resolved = config::resolve_provider(
        cli.provider.as_deref(),
        cli.model.as_deref(),
        &cli.ollama_url,
        interactive,
    )?;

    let session_config = SessionConfig {
        model: resolved.model,
        system_prompt: cli.system.clone(),
        max_turns: cli.max_turns,
        permission_mode,
        ..Default::default()
    };

    let mut session = SessionState::new(session_config, cwd.clone());

    // Single-query / print mode
    if let Some(prompt) = single_prompt {
        // Check for skill activation
        if !cli.no_skills {
            if let Some(skill_match) = skill_activator.should_auto_activate(&prompt) {
                let activation = skill_activator.activate(skill_match).await?;
                session.apply_skill_activation(activation);
            }
        }
        
        session.push_message(Message::user(prompt));
        let on_event = make_event_callback(cli.output_format);
        query(
            &mut session,
            resolved.provider.as_ref(),
            Arc::clone(&registry),
            &orchestrator,
            Some(on_event),
        )
        .await?;

        if cli.output_format == OutputFormat::Json {
            let last_assistant = session
                .messages
                .iter()
                .rev()
                .find(|m| matches!(m.role, claw_core::Role::Assistant));
            if let Some(msg) = last_assistant {
                let text: String = msg
                    .content
                    .iter()
                    .filter_map(|b| match b {
                        claw_core::ContentBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("");
                println!(
                    "{}",
                    serde_json::json!({
                        "type": "result",
                        "text": text,
                        "session_id": session.id,
                        "input_tokens": session.total_input_tokens,
                        "output_tokens": session.total_output_tokens,
                    })
                );
            }
        }

        return Ok(());
    }

    // Interactive REPL
    println!("Claw RS v{}", env!("CARGO_PKG_VERSION"));
    println!("Type your message, or 'exit' / Ctrl-D to quit.");
    println!("Skills: {} loaded, use /review, /commit, /refactor or keywords.", 
             skill_registry.count());
    println!();

    let on_event = make_event_callback(OutputFormat::Text);
    let stdin = io::stdin();
    loop {
        print!("> ");
        io::stdout().flush()?;

        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == "exit" || line == "quit" {
            break;
        }
        
        // Handle skill listing command
        if line == "/skills" {
            println!("\nAvailable Skills:");
            for skill in skill_registry.list() {
                println!("  {} - {}", skill.name, skill.description);
            }
            println!();
            continue;
        }

        // Check for skill activation
        if !cli.no_skills {
            let matches = skill_activator.prefetch(line);
            if !matches.is_empty() {
                // Show which skill(s) are being activated
                for m in &matches {
                    eprintln!("🎯 Activating skill: {}", m.skill.name);
                }
                
                // Activate the best match
                if let Some(best) = matches.first() {
                    let activation = skill_activator.activate(best.clone()).await?;
                    session.apply_skill_activation(activation);
                }
            }
        }

        session.push_message(Message::user(line));

        if let Err(e) = query(
            &mut session,
            resolved.provider.as_ref(),
            Arc::clone(&registry),
            &orchestrator,
            Some(Arc::clone(&on_event)),
        )
        .await
        {
            eprintln!("error: {}", e);
        }
        
        println!();
    }

    eprintln!(
        "\n[session: {} turns, {} in / {} out tokens]",
        session.turn_count, session.total_input_tokens, session.total_output_tokens
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Event callback factory
// ---------------------------------------------------------------------------

fn make_event_callback(format: OutputFormat) -> Arc<dyn Fn(QueryEvent) + Send + Sync> {
    Arc::new(move |event| match format {
        OutputFormat::Text => handle_event_text(event),
        OutputFormat::StreamJson => handle_event_stream_json(event),
        OutputFormat::Json => {
            match &event {
                QueryEvent::ToolUseStart { name, .. } => {
                    eprintln!("⚡ calling tool: {}", name);
                }
                QueryEvent::ToolResult { is_error, content, .. } => {
                    if *is_error {
                        eprintln!("❌ tool error: {}", truncate(content, 200));
                    }
                }
                _ => {}
            }
        }
    })
}

fn handle_event_text(event: QueryEvent) {
    match event {
        QueryEvent::TextDelta(text) => {
            print!("{}", text);
            let _ = io::stdout().flush();
        }
        QueryEvent::ToolUseStart { name, .. } => {
            eprintln!("\n⚡ calling tool: {}", name);
        }
        QueryEvent::ToolResult { is_error, content, .. } => {
            if is_error {
                eprintln!("❌ tool error: {}", truncate(&content, 200));
            } else {
                eprintln!("✅ tool done ({})", byte_summary(&content));
            }
        }
        QueryEvent::TurnComplete { .. } => {
            println!();
        }
        QueryEvent::Usage { input_tokens, output_tokens } => {
            eprintln!("  [tokens: {} in / {} out]", input_tokens, output_tokens);
        }
    }
}

fn handle_event_stream_json(event: QueryEvent) {
    let obj = match event {
        QueryEvent::TextDelta(text) => {
            serde_json::json!({ "type": "text_delta", "text": text })
        }
        QueryEvent::ToolUseStart { id, name } => {
            serde_json::json!({ "type": "tool_use_start", "id": id, "name": name })
        }
        QueryEvent::ToolResult { tool_use_id, content, is_error } => {
            serde_json::json!({
                "type": "tool_result",
                "tool_use_id": tool_use_id,
                "content": content,
                "is_error": is_error,
            })
        }
        QueryEvent::TurnComplete { stop_reason } => {
            serde_json::json!({ "type": "turn_complete", "stop_reason": format!("{:?}", stop_reason) })
        }
        QueryEvent::Usage { input_tokens, output_tokens } => {
            serde_json::json!({ "type": "usage", "input_tokens": input_tokens, "output_tokens": output_tokens })
        }
    };
    println!("{}", obj);
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

fn byte_summary(s: &str) -> String {
    let len = s.len();
    if len < 1024 {
        format!("{} bytes", len)
    } else {
        format!("{:.1} KB", len as f64 / 1024.0)
    }
}