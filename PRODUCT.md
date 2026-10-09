# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

A single developer (the owner) running the app locally next to their IDE, driving long Claude Code coding sessions from a browser tab instead of the terminal.

## Product Purpose

A web front-end for the Claude Code CLI. It spawns `claude` as a long-lived process speaking the Agent SDK stream-json protocol and renders the session: prompts, assistant markdown, tool calls and results, permission requests, interruption, and per-turn cost/duration. Success: the session is easier to follow, steer, and approve than in the terminal, without losing any of the CLI's power.

## Positioning

It is the real Claude Code agent (same CLI, same flags the SDK and VS Code extension use), not a re-implementation: every tool call, permission prompt and result is the CLI's own event, surfaced in a browser.

## Operating Context

- Runs on localhost (ASP.NET Core Blazor Server, interactive server render mode), one user.
- Each session is bound to a working directory and a permission mode (`default`, `acceptEdits`, `plan`, `auto`) chosen before it starts.
- Used alongside an IDE and terminal; sessions can be long, with many tool calls.

## Capabilities and Constraints

- Today: single session page with working directory, permission mode, new session, send (Ctrl+Enter), stop/interrupt, markdown rendering (Markdig, raw HTML disabled), highlight.js code blocks, collapsible tool call/result entries, allow/deny permission cards, result meta (subtype, duration, cost USD).
- Planned screens (mockups requested): chat session, permission request, start/new session, multi-session navigation (sidebar, history, parallel sessions).
- Stack: .NET 10 Blazor Server, Markdig, highlight.js. No auth.
- Undecided: session persistence/history storage, how parallel sessions are surfaced.

## Brand Commitments

Product name in UI: "Claude Code". The previous VS Code–style dark look is explicitly not binding (user asked for a new direction).

**Dark theme is mandatory** (user, 2026-10-09: devs don't work in light themes). Rejected directions: transit-line signage, ATC flight strips, split-flap board, light IDE-chat canon.

Mascot: BlazorKawaii 2.2.0 `RubberDuck` (the user's own package); its mood reflects session state.

## Evidence on Hand

No screenshots, logos, or real session transcripts in the repo. Mockup content must be labeled synthetic; never invent costs, model capabilities, or features the CLI does not have.

## Product Principles

1. The agent's actions are the content: tool calls, diffs and results must be scannable at a glance and inspectable in full.
2. Approval is a decision, not a nag: permission requests show exactly what will run or change.
3. Never hide state: working directory, mode, busy/idle, cost are always visible.
4. Keyboard-first, built for long sessions.
