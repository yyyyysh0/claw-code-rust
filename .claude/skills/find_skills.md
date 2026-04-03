---
name: find-skills
description: 查找和列出所有可用的skills
triggers:
  - type: command
    command: /skills
    alias: [/find-skills, /list-skills]
  - type: pattern
    pattern: "列出.*技能|查找.*skills|available skills"
    case_insensitive: true
  - type: keyword
    keywords: [skills, 技能列表, available skills]
priority: 9
tags: [meta, skills, discovery]
---

# Find Skills Skill

用户想要查看当前可用的skills列表。

请列出所有已加载的skills及其触发条件。

用户请求: {{trigger_input}}
