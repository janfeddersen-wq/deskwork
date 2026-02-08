# IMPLEMENTATION PROMPT: Knowledge-Work Plugin System for Deskwork

> **Context for Claude Code:** You are implementing a plugin system into an existing desktop application called "deskwork" that already has MCP support. This document is your complete specification. Follow it section by section. Ask clarifying questions if the codebase deviates from assumptions made here.

---

## 1. WHAT YOU ARE BUILDING

You are integrating Anthropic's open-source **knowledge-work-plugins** architecture into deskwork. These plugins are NOT traditional code plugins — they are **structured Markdown and JSON files** that give Claude domain-specific expertise, slash commands, and connections to external tools via MCP.

**Source repository:** `https://github.com/anthropics/knowledge-work-plugins`

Clone this repo into the project workspace first:

```bash
git clone https://github.com/anthropics/knowledge-work-plugins.git
```

Study the file structure before writing any code. The key directories are:
- `legal/` — the primary plugin we're implementing first
- Each plugin follows the same structure (see Section 3)

---

## 2. ARCHITECTURE OVERVIEW

The system has three layers. Build them in this order:

```
┌─────────────────────────────────────────────────┐
│  LAYER 3: PLUGIN RUNTIME                        │
│  Loads plugins, resolves skills/commands,        │
│  injects context into Claude conversations       │
├─────────────────────────────────────────────────┤
│  LAYER 2: MCP INTEGRATION                        │
│  deskwork already supports this — we wire         │
│  plugin .mcp.json configs into the existing       │
│  MCP client infrastructure                        │
├─────────────────────────────────────────────────┤
│  LAYER 1: PLUGIN LOADER & REGISTRY               │
│  File-based discovery, manifest parsing,          │
│  validation, installation management              │
└─────────────────────────────────────────────────┘
```

---

## 3. PLUGIN FILE STRUCTURE — UNDERSTAND THIS FIRST

Every plugin in the repo follows this exact layout:

```
plugin-name/
├── .claude-plugin/
│   └── plugin.json          # Manifest: name, version, description, metadata
├── .mcp.json                # MCP server connections (tool wiring)
├── commands/
│   ├── command-name.md      # Each file = one slash command
│   └── ...
├── skills/
│   ├── skill-name.md        # Domain knowledge Claude draws on automatically
│   └── ...
├── assets/                  # Optional: templates, reference docs, examples
│   └── ...
├── README.md
├── CONNECTORS.md            # Documents which MCP servers are needed
└── plugin-name.local.md     # User-customizable config (playbooks, org-specific settings)
```

### 3.1 — plugin.json (Manifest)

This is the identity card for each plugin. Parse it to register the plugin.

```json
{
  "name": "legal",
  "version": "1.0.0",
  "description": "AI-powered legal assistant for in-house counsel",
  "author": "anthropics",
  "license": "Apache-2.0",
  "skills": ["skills/*.md"],
  "commands": ["commands/*.md"],
  "connectors": ".mcp.json"
}
```

### 3.2 — Skills (Auto-Injected Context)

Skill files are Markdown containing domain expertise. They are **not** explicitly invoked — they are injected into Claude's system prompt or context window when the plugin is active. Claude automatically draws on them based on conversation relevance.

Example: `skills/contract-review.md` contains clause analysis frameworks, risk assessment criteria, standard negotiation positions, etc.

**Implementation requirement:** When a plugin is active, concatenate all its skill files and include them in the system prompt sent to Claude via the API.

### 3.3 — Commands (Explicit Slash Commands)

Command files define user-triggered actions. Each `.md` file is a structured prompt template with:
- A command name (derived from filename, e.g., `review-contract.md` → `/legal:review-contract`)
- Input parameters the command expects
- Step-by-step instructions for Claude to follow
- Output format specifications

**Implementation requirement:** Parse command files, register them as available slash commands in the UI, and when invoked, inject the command's content as a user-turn prompt to Claude along with any provided inputs.

### 3.4 — .mcp.json (Tool Connections)

This maps abstract tool categories to concrete MCP server configurations:

```json
{
  "mcpServers": {
    "cloud-storage": {
      "command": "npx",
      "args": ["-y", "@anthropic/mcp-server-box"],
      "env": { "BOX_API_KEY": "${BOX_API_KEY}" }
    },
    "chat": {
      "command": "npx",
      "args": ["-y", "@anthropic/mcp-server-slack"],
      "env": { "SLACK_TOKEN": "${SLACK_TOKEN}" }
    }
  }
}
```

**Implementation requirement:** Merge plugin `.mcp.json` entries into deskwork's existing MCP configuration. Use deskwork's existing MCP client infrastructure to spawn and manage these servers. If a server can't be started (missing credentials, unavailable), the plugin should **gracefully degrade** — note unavailable tools but don't fail.

### 3.5 — local.md (User Configuration)

The `legal.local.md` file contains customizable settings like:
- Organization's standard contract positions (liability caps, indemnification scope)
- Acceptable ranges and escalation triggers
- Company-specific terminology and preferences
- Approval workflows

**Implementation requirement:** This file is user-editable. Provide a UI path for users to open and modify it. Include it in the context sent to Claude alongside skill files.

---

## 4. IMPLEMENTATION STEPS — LAYER 1: PLUGIN LOADER & REGISTRY

### 4.1 — Define the Plugin data model

Create a data model / type that represents a loaded plugin:

```
Plugin {
  id: string                    // e.g., "legal"
  name: string                  // from plugin.json
  version: string               // from plugin.json
  description: string           // from plugin.json
  path: string                  // filesystem path to plugin root
  enabled: boolean              // user toggle
  skills: SkillFile[]           // parsed skill markdown files
  commands: CommandFile[]        // parsed command markdown files
  mcpConfig: McpServerConfig{}  // parsed .mcp.json
  localConfig: string | null    // contents of .local.md if it exists
  status: "active" | "inactive" | "error"
  errors: string[]              // any loading errors
}

SkillFile {
  name: string                  // filename without extension
  content: string               // raw markdown content
  path: string                  // file path
}

CommandFile {
  name: string                  // filename without extension → slash command name
  content: string               // raw markdown content (the prompt template)
  path: string                  // file path
  pluginId: string              // parent plugin id for namespacing
  slashCommand: string          // computed: "/{pluginId}:{name}"
}
```

### 4.2 — Plugin discovery and loading

Build a PluginLoader module that:

1. **Scans a plugins directory** (e.g., `~/.deskwork/plugins/`) for subdirectories
2. **Validates** each directory has `.claude-plugin/plugin.json`
3. **Parses** the manifest
4. **Reads all skill files** matching the glob patterns in the manifest
5. **Reads all command files** matching the glob patterns
6. **Parses `.mcp.json`** if present
7. **Reads the `.local.md`** file if present
8. **Returns a Plugin object** or an error state

```
function loadPlugin(pluginPath: string): Plugin | PluginError
function discoverPlugins(pluginsDir: string): Plugin[]
```

### 4.3 — Plugin Registry

Build a PluginRegistry that:
- Maintains the list of discovered plugins
- Tracks enabled/disabled state (persist to deskwork's settings)
- Provides lookup by id, by command name, by MCP server category
- Emits events when plugins are enabled/disabled/reloaded

```
class PluginRegistry {
  plugins: Map<string, Plugin>
  
  enable(pluginId: string): void
  disable(pluginId: string): void
  getPlugin(pluginId: string): Plugin
  getCommandHandler(slashCommand: string): CommandFile | null
  getActiveSkills(): SkillFile[]        // all skills from enabled plugins
  getActiveMcpConfigs(): McpServerConfig[]  // all MCP configs from enabled plugins
  reload(): void                        // re-scan and re-load all plugins
}
```

### 4.4 — Plugin installation

Support two installation methods:

**Method A — Clone from repo:**
```bash
# User provides a git URL or selects from marketplace
git clone https://github.com/anthropics/knowledge-work-plugins.git /tmp/kwp
cp -r /tmp/kwp/legal ~/.deskwork/plugins/legal
```

**Method B — Manual directory copy:**
User drops a plugin folder into `~/.deskwork/plugins/`

After installation, run `registry.reload()` to pick up the new plugin.

---

## 5. IMPLEMENTATION STEPS — LAYER 2: MCP WIRING

Deskwork already supports MCP. The plugin system needs to:

### 5.1 — Merge MCP configurations

When a plugin is enabled, read its `.mcp.json` and merge those server definitions into deskwork's active MCP configuration. Namespace server names to avoid collisions:

```
// Original .mcp.json key: "cloud-storage"
// Namespaced key: "legal:cloud-storage"
```

### 5.2 — Environment variable resolution

Plugin `.mcp.json` files use `${VAR_NAME}` placeholders. Resolve these from:
1. deskwork's own credential/secrets store (preferred)
2. System environment variables (fallback)
3. Prompt the user if missing (with option to save)

### 5.3 — Graceful degradation

If an MCP server fails to start or a credential is missing:
- Log the error on the plugin's `errors` array
- Set the specific connector as unavailable
- Do NOT prevent the plugin from loading
- Include a note in the context sent to Claude: "Note: The {connector-name} tool is currently unavailable. Suggest manual alternatives where this tool would normally be used."

### 5.4 — Tool discovery forwarding

When Claude requests available tools, aggregate tools from:
1. Deskwork's built-in tools
2. All MCP servers from enabled plugins
3. Any user-configured MCP servers

Present them all in the tool definitions sent to Claude's API.

---

## 6. IMPLEMENTATION STEPS — LAYER 3: PLUGIN RUNTIME

This is the core orchestration layer that connects plugins to Claude conversations.

### 6.1 — System prompt injection

When building the system prompt for a Claude API call, append plugin context:

```
BASE_SYSTEM_PROMPT
+ "\n\n--- ACTIVE PLUGINS ---\n\n"
+ for each enabled plugin:
    + "## Plugin: {plugin.name}\n"
    + "Description: {plugin.description}\n\n"
    + "### Domain Knowledge\n"
    + concatenate all skill file contents
    + "\n\n### Available Commands\n"
    + list all slash commands with brief descriptions
    + "\n\n### Configuration\n"
    + plugin.localConfig contents (if any)
    + "\n\n### Available Tools\n"
    + list connected MCP tools and their status
```

**IMPORTANT:** Skill content can be large. Implement a token budget:
- Set a max token allocation for plugin context (e.g., 30% of context window)
- If total skill content exceeds budget, prioritize:
  1. The plugin most relevant to the current conversation (use keyword matching or embeddings)
  2. The local.md config (always include — it's the user's customization)
  3. Skills referenced by invoked commands
  4. Remaining skills by relevance

### 6.2 — Slash command handling

When a user types a slash command in the chat input:

1. **Parse the command:** Extract plugin id and command name from `/{pluginId}:{commandName}` format
2. **Look up the command:** `registry.getCommandHandler(slashCommand)`
3. **Collect inputs:** If the command template specifies required inputs (file uploads, parameters), prompt the user via the UI
4. **Build the prompt:** Inject the command's markdown content as a user message, replacing any input placeholders with actual values
5. **Send to Claude:** Include the parent plugin's skills in the system prompt and the command prompt in the user turn
6. **Handle the response:** Route Claude's response back to the chat UI, including any tool use results

Example flow for `/legal:review-contract`:

```
User types: /legal:review-contract
  → UI prompts for: contract file upload, which side they represent, focus areas
  → System prompt includes: all legal skills + legal.local.md playbook
  → User message includes: the review-contract.md template + uploaded contract text + user context
  → Claude responds with: clause-by-clause analysis with GREEN/YELLOW/RED flags
  → UI renders: structured review output
```

### 6.3 — Slash command autocomplete

Build an autocomplete system for the chat input:
- When user types `/`, show all available commands from enabled plugins
- Group by plugin: "Legal", "Sales", etc.
- Show command description from the first line/header of the command file
- Support fuzzy matching

### 6.4 — Plugin context awareness

Even without explicit slash commands, Claude should leverage plugin skills contextually. If the legal plugin is enabled and a user asks "can you review this contract I'm attaching?", Claude should recognize the legal context and apply skills from the legal plugin.

To support this:
- Always include active skill content in the system prompt (within token budget)
- Claude will naturally reference the domain knowledge when relevant

---

## 7. THE LEGAL PLUGIN — SPECIFIC IMPLEMENTATION DETAILS

The legal plugin is the priority. Here's what it contains and how each part should work:

### 7.1 — Legal Plugin Commands

Implement these five slash commands:

| Command | Purpose | Required Inputs |
|---------|---------|----------------|
| `/legal:review-contract` | Clause-by-clause contract review against playbook | Contract file/text, party role, focus areas |
| `/legal:triage-nda` | Rapid NDA pre-screening with GREEN/YELLOW/RED flags | NDA file/text |
| `/legal:vendor-check` | Surface existing agreements with a vendor | Vendor name |
| `/legal:brief` | Generate legal briefings (daily/topic/incident) | Brief type, topic (if applicable) |
| `/legal:respond` | Templated responses for common inquiries | Inquiry type, details |

### 7.2 — The Playbook System (legal.local.md)

This is the most important file for the legal plugin. It defines the organization's standard positions. The default playbook from the repo includes:

- **Limitation of Liability:** Mutual cap at 12 months of fees (acceptable: 6–24 months)
- **Indemnification:** Mutual for IP infringement and data breach
- **IP Ownership:** Each party retains pre-existing IP
- **Data Protection:** DPA required for personal data processing, 72-hour breach notification
- **Termination:** Standard termination for convenience and cause provisions

Each clause type has:
- **Standard position** (the organization's preferred terms)
- **Acceptable range** (what can be approved without escalation)
- **Escalation triggers** (conditions that MUST go to a human attorney)

**Implementation requirement:** When `/legal:review-contract` runs, the playbook is the rubric. Claude compares each contract clause against the playbook and flags deviations as GREEN (within standard), YELLOW (within acceptable range but non-standard), or RED (outside acceptable range / escalation trigger hit).

### 7.3 — Legal Plugin MCP Connectors

The legal plugin's `.mcp.json` connects to:

| Category | Supported Tools | Purpose |
|----------|----------------|---------|
| Cloud Storage | Box, Egnyte | Access contract documents, templates |
| Chat | Slack | Pull context from legal channels, send notifications |
| Office Suite | Microsoft 365 | Read/write Word docs, emails |
| Project Management | Atlassian/Jira | Track legal matters, tickets |

The plugin's skill files use **`~~category`** placeholders (e.g., `~~cloud storage`, `~~chat`) instead of specific product names. This is the tool-agnosticism layer — swapping Box for Google Drive means editing only `.mcp.json`, not the skill files.

**Implementation requirement:** When rendering skill content for Claude, replace `~~category` placeholders with the actual tool name configured in `.mcp.json` for that category. If no tool is configured for a category, replace with "[not configured]" and append a note.

### 7.4 — Graceful output when tools are missing

If the user hasn't configured all connectors, Claude's responses should acknowledge this naturally:

```
"I've completed the contract review based on the document you provided. 
Note: I wasn't able to check for existing agreements with this vendor in 
your document management system since cloud storage isn't connected yet. 
You may want to manually check Box/Egnyte for any prior agreements."
```

This behavior comes from the skill files themselves — they instruct Claude to note gaps.

---

## 8. UI REQUIREMENTS

### 8.1 — Plugin Management Screen

Build a settings/preferences panel for plugins:
- List all discovered plugins with name, description, version, status
- Toggle to enable/disable each plugin
- "Configure" button that opens the `.local.md` file in an editor (or inline editing)
- "Connectors" section showing required MCP servers and their connection status
- Credential entry for MCP servers that need API keys
- "Install Plugin" action (git URL or folder path)
- "Reload Plugins" action

### 8.2 — Chat Integration

In the chat interface:
- Slash command autocomplete when typing `/`
- Visual indicator showing which plugins are active in the current conversation
- Structured rendering for plugin outputs (e.g., GREEN/YELLOW/RED badges for contract review)
- File upload support for commands that need document input

### 8.3 — Plugin Status Indicators

Show in the UI:
- Which plugins are enabled
- Which MCP connectors are connected vs. missing
- Any plugin loading errors

---

## 9. FILE ORGANIZATION IN DESKWORK CODEBASE

Place the new code in a logical module structure within the existing codebase:

```
src/
├── plugins/
│   ├── loader.ts              # Plugin discovery and file parsing
│   ├── registry.ts            # Plugin state management
│   ├── runtime.ts             # Context injection, command dispatch
│   ├── mcp-bridge.ts          # Merges plugin MCP configs with deskwork's MCP
│   ├── types.ts               # Plugin, SkillFile, CommandFile types
│   ├── slash-commands.ts      # Command parsing, autocomplete, execution
│   └── context-builder.ts     # Builds system prompt with plugin context
├── ui/
│   ├── plugin-manager/        # Settings screen components
│   └── chat/
│       ├── slash-autocomplete/ # Autocomplete dropdown
│       └── plugin-output/     # Structured output renderers
```

---

## 10. IMPLEMENTATION ORDER

Follow this sequence. Complete each step before moving to the next:

1. **Clone the knowledge-work-plugins repo** and study the legal plugin structure thoroughly
2. **Build types.ts** — define all data models
3. **Build loader.ts** — plugin discovery, manifest parsing, file reading
4. **Build registry.ts** — plugin state management, enable/disable
5. **Build mcp-bridge.ts** — merge plugin MCP configs into deskwork's MCP system
6. **Build context-builder.ts** — system prompt construction with skill injection and token budgeting
7. **Build slash-commands.ts** — command parsing, placeholder resolution, execution flow
8. **Build runtime.ts** — orchestration layer connecting everything
9. **Build the Plugin Management UI**
10. **Build the slash command autocomplete UI**
11. **Test end-to-end** with the legal plugin: enable it, configure a playbook, run `/legal:review-contract` with a sample contract
12. **Add remaining plugins** from the repo once legal works correctly

---

## 11. TESTING CHECKLIST

Before considering the implementation complete, verify:

- [ ] Legal plugin loads without errors from `~/.deskwork/plugins/legal/`
- [ ] All 5 legal slash commands appear in autocomplete
- [ ] `/legal:review-contract` accepts a file upload, asks for context, and produces a clause-by-clause review with GREEN/YELLOW/RED flags
- [ ] `/legal:triage-nda` produces a risk classification
- [ ] The playbook in `legal.local.md` can be edited and changes are reflected in subsequent reviews
- [ ] MCP connectors from `.mcp.json` are merged into deskwork's MCP config
- [ ] Missing MCP credentials produce a warning, not a crash
- [ ] Disabling a plugin removes its skills from the system prompt and its commands from autocomplete
- [ ] Plugin context stays within token budget even with multiple plugins enabled
- [ ] `~~category` placeholders in skills are resolved to actual tool names

---

## 12. IMPORTANT CONSTRAINTS

- **Do NOT modify the upstream plugin files.** The files from `knowledge-work-plugins` repo should be treated as read-only (except `.local.md` which is explicitly user-configurable). All customization happens in deskwork's code, not in the plugin files.
- **Do NOT build a package manager.** Keep installation simple — copy the folder, reload. No npm/pip for plugins.
- **Maintain the file-based architecture.** Plugins are Markdown and JSON. Don't convert them to a database or compiled format. The whole point is they're human-readable and editable.
- **Respect the tool-agnosticism pattern.** The `~~category` placeholder system exists for a reason. Preserve it.
- **Always include escalation guardrails.** The legal plugin explicitly defines situations that MUST escalate to a human attorney. Never suppress these.

---

## 13. REFERENCE LINKS

- **Plugin repo:** https://github.com/anthropics/knowledge-work-plugins
- **Legal plugin README:** https://github.com/anthropics/knowledge-work-plugins/blob/main/legal/README.md
- **Legal connectors doc:** https://github.com/anthropics/knowledge-work-plugins/blob/main/legal/CONNECTORS.md
- **MCP specification:** https://modelcontextprotocol.io/specification/2025-11-25
- **MCP TypeScript SDK:** https://github.com/modelcontextprotocol/typescript-sdk
- **MCP Python SDK:** https://github.com/modelcontextprotocol/python-sdk
- **Claude Tool Use docs:** https://platform.claude.com/docs/en/agents-and-tools/tool-use/overview
- **MCP Connector (API beta):** https://platform.claude.com/docs/en/agents-and-tools/mcp-connector
- **Building MCP servers:** https://modelcontextprotocol.io/docs/develop/build-server

---

*This prompt was generated from deep research into the knowledge-work-plugins repository, MCP specification, and Claude tool use documentation as of February 2026. Adapt to your codebase's conventions, language, and framework as needed.*
