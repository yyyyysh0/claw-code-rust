---
name: code-review
description: Review code for quality, security, and best practices
triggers:
  - command: /review
  - pattern: "review.*code"
  - keyword:
      keywords: [review, critique, analyze code]
tools: [file_read, glob, grep]
context_files:
  - README.md
  - .claude/rules.md
priority: 10
tags: [code-quality, security, best-practices]
---

# Code Review Skill

You are performing a comprehensive code review. Analyze the provided code and provide feedback on:

## Review Checklist

1. **Code Quality**
   - Readability and clarity
   - Naming conventions
   - Code organization and structure
   - DRY (Don't Repeat Yourself) principle adherence

2. **Security**
   - Input validation
   - Authentication/authorization concerns
   - Sensitive data handling
   - Potential vulnerabilities (SQL injection, XSS, etc.)

3. **Performance**
   - Algorithm efficiency
   - Resource usage (memory, CPU, I/O)
   - N+1 query problems
   - Caching opportunities

4. **Best Practices**
   - Language-specific conventions
   - Error handling patterns
   - Testing coverage suggestions
   - Documentation quality

5. **Maintainability**
   - Code complexity (cyclomatic complexity)
   - Dependency management
   - Refactoring opportunities

## Output Format

Provide your review in this structure:

```
## Summary
[Brief overall assessment]

## Critical Issues 🔴
[Issues that must be addressed before merge]

## Suggested Improvements 🟡
[Non-critical but recommended changes]

## Positive Observations 🟢
[Good practices observed]

## Detailed Review
[Line-by-line or section-by-section analysis]
```

Focus on actionable feedback. Explain **why** each issue matters and **how** to fix it.

{{context_files}}

---

Review the following code for: {{trigger_input}}