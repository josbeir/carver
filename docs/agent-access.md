# Agent access

Carver can expose its library to local AI clients through the `carver-mcp` stdio server. This is
especially handy when your notes are where thinking happens: write down rough ideas, meeting
notes, research, and project plans in Carver, then let an agent search and read that context when
helping you plan a project. With write access enabled, it can also draft or update plans, create
notes for new work, and organize notes and categories—so the useful result stays in your library
rather than disappearing into a chat transcript.

Open the **Connect an agent** entry in Carver's menu to choose Codex, Claude Code, GitHub Copilot
CLI, VS Code Copilot, OpenCode, or a generic stdio-MCP client and copy a user-level setup command.
The setup screen detects native, Flatpak, and Snap installs so the agent process opens the same
private library as Carver.

The server is read-only by default. Opt into reversible note changes explicitly with
`--allow-write`; permanent trash deletion, settings changes, raw database access, and managed
asset bytes are never exposed.

Agents can list categories and notes, search and read note source, and inspect the recoverable
trash. Category listings include each category's icon and accent colour. With write access
enabled, agents can create, rename, and update category appearance; create, save, move, trash,
restore, and adjust the creation and modification timestamps of notes; and trash or restore
categories. Every note write uses Carver's revision check, so an agent must reload a note after a
conflicting edit. `create_note` and `save_note` accept `markdown: true` to convert CommonMark
input into Carver's canonical source. Their existing note fields remain at the top level and a
`report` field carries the version 2 importer-fidelity assessment; unverified Markdown is marked
`dropped` / `fallback` so an agent cannot mistake missing evidence for a clean import.
`get_note` returns that canonical source, while
`get_note` with `markdown: true` returns a Markdown rendering for agents that prefer it.

`carver-mcp` is a local stdio process, not a network service. It opens the same XDG-scoped library
as the installed application, including the separate Flatpak or Snap data area when applicable.
Treat note contents returned to an agent as untrusted data, and review an agent's proposed changes
before enabling write access.

For headless setup, print the relevant command with:

```sh
carver-mcp configure codex
carver-mcp configure claude-code --allow-write
carver-mcp configure copilot
carver-mcp configure vscode
carver-mcp configure opencode
carver-mcp configure generic
```

[Back to Carver](../README.md)
