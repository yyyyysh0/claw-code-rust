---
name: refactor
description: Analyze and refactor code for improved maintainability
triggers:
  - command: /refactor
  - pattern: "refactor.*"
  - keyword:
      keywords: [refactor, clean up, simplify, optimize code]
tools: [file_read, file_edit, file_write, glob, grep, bash]
denied_tools: []  # All tools allowed for refactoring
priority: 7
tags: [refactoring, code-quality, maintenance]
---

# Refactoring Skill

You are performing code refactoring to improve code quality while preserving behavior.

## Refactoring Principles

1. **Preserve Behavior**
   - Never change what the code does
   - Only change how it does it
   - Verify tests still pass

2. **Small Steps**
   - Make incremental changes
   - Each change should be reversible
   - Test after each step

3. **Common Techniques**
   - Extract Function/Method
   - Extract Variable
   - Rename Variable/Method
   - Replace Magic Number with Named Constant
   - Decompose Conditional
   - Consolidate Duplicate Conditional Fragments
   - Replace Nested Conditional with Guard Clauses
   - Introduce Parameter Object
   - Remove Assignments to Parameters
   - Replace Method with Method Object

## Refactoring Workflow

1. **Analysis Phase**
   - Read the target code
   - Identify code smells and issues
   - Prioritize refactoring opportunities

2. **Planning Phase**
   - List specific refactorings to apply
   - Estimate risk level for each
   - Order refactorings by dependency

3. **Execution Phase**
   - Apply refactorings one at a time
   - Verify behavior preservation
   - Document changes

4. **Verification Phase**
   - Run tests if available
   - Check for regressions
   - Validate improvements

## Code Smells to Detect

- Long methods (>10 lines usually)
- Large classes
- Long parameter lists (>3 parameters)
- Duplicate code
- Dead code
- Magic numbers/strings
- Complex conditionals
- Deeply nested code
- Inappropriate intimacy (tight coupling)
- Feature envy (method uses another class more)

## Output Format

```
## Refactoring Plan

### Identified Issues
[List code smells and problems found]

### Proposed Changes
[Ordered list of refactorings with rationale]

### Risk Assessment
[Low/Medium/High risk per change]

## Execution
[Apply refactorings step by step]

## Summary
[What was improved, metrics if applicable]
```

Focus on the following code: {{trigger_input}}

Working directory: {{cwd}}