# Product

## Register

product

## Users

Software developers (primarily macOS power users) who run multiple AI coding agents in parallel. They keep several repositories open simultaneously, spawn sub-agents for parallel tasks, and need to see at a glance which agents are working, waiting, or done without losing focus on their current task.

## Product Purpose

Burrow is a native macOS terminal multiplexer purpose-built for AI agent workflows. It wraps real PTYs in a multi-workspace IDE shell so developers can orchestrate Claude Code, Aider, Codex, and similar agents side-by-side. Success means agents are visibly managed, orchestration is frictionless, and the tool stays out of the way when work is flowing.

## Brand Personality

Focused. Precise. Unobtrusive.

The tool should feel like a well-worn workshop — serious, built for professionals who already know what they're doing. Not playful, not marketing-polished. Dense information without clutter.

## Anti-references

- VS Code (too general-purpose, too much chrome)
- JetBrains IDEs (too heavy, too many panels)
- iTerm2 (no hierarchy, no status)
- Generic SaaS dashboards with big cards, hero metrics, gradient text

## Design Principles

1. **Density without noise.** Show more in less space. Hierarchy through size and opacity, not extra containers.
2. **Status at a glance.** Agents run asynchronously; the UI must surface state without requiring focus.
3. **Professional tool aesthetics.** Native feel, terminal-adjacent, no decorative gradients or rounded-corner excess.
4. **Predictable chrome.** The shell should disappear; only the content (terminals, agents) should demand attention.
5. **Theme-agnostic structure.** The layout must work across dark, light, Monokai, Nebula, etc. — lean on CSS vars, never hardcode palette values.

## Accessibility & Inclusion

WCAG AA minimum for text contrast. Keyboard navigable. Status communicated via shape and text, not color alone (status dots supplement text labels).
