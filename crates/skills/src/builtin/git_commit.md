---
name: git-commit
description: Generate well-structured git commit messages following conventions
triggers:
  - command: /commit
  - pattern: "commit.*changes"
  - keyword:
      keywords: [commit, git commit, make a commit]
tools: [bash, file_read, glob]
context_files:
  - .git/COMMIT_EDITMSG
priority: 8
tags: [git, version-control, automation]
---

# Git Commit Skill

You are generating a commit message for staged changes. Follow these conventions:

## Commit Message Format

Use the Conventional Commits specification:

```
<type>(<scope>): <description>

[optional body]

[optional footer(s)]
```

## Types

- `feat`: A new feature
- `fix`: A bug fix
- `docs`: Documentation only changes
- `style`: Changes that do not affect meaning (white-space, formatting)
- `refactor`: Code change that neither fixes a bug nor adds a feature
- `perf`: Performance improvement
- `test`: Adding missing tests or correcting existing tests
- `chore`: Changes to build process or auxiliary tools
- `ci`: CI/CD configuration changes
- `revert`: Reverts a previous commit

## Guidelines

1. **Subject Line**
   - Use imperative mood ("add feature" not "added feature")
   - No period at the end
   - Max 72 characters
   - Start with lowercase

2. **Body** (optional)
   - Explain **what** and **why**, not **how**
   - Wrap at 72 characters
   - Separate from subject with blank line

3. **Footer** (optional)
   - Reference issues: `Closes #123`
   - Breaking changes: `BREAKING CHANGE: description`

## Process

1. Use `git diff --cached` to see staged changes
2. Analyze the changes to determine type and scope
3. Generate an appropriate commit message
4. Present the message for user confirmation

## Example Output

```
feat(api): add user authentication endpoint

Implement JWT-based authentication for the user API.
Includes login, logout, and token refresh endpoints.

Closes #42
```

Generate a commit message for the currently staged changes.