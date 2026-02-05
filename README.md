# Deskwork

🐕 A Claude-powered coding assistant with a native desktop interface.

## Overview

Deskwork is a desktop application that brings Claude's coding capabilities to your local machine. Think Claude Code, but with a proper GUI!

## Project Structure

```
deskwork/
├── Cargo.toml              # Workspace root
├── deskwork-core/          # Core library
│   └── src/lib.rs          # LLM integration, tools, database
└── deskwork-gui/           # GUI application
    └── src/main.rs         # egui-based desktop app
```

## Features (Planned)

- 💬 Chat interface with Claude
- 📁 Project file browsing and management
- 🛠️ Tool execution (file ops, shell commands, etc.)
- 💾 Conversation history with SQLite persistence
- 🎨 Native look and feel via egui

## Building

```bash
# Build the workspace
cargo build

# Run the GUI
cargo run -p deskwork-gui

# Or just:
cargo run
```

## Dependencies

This project uses:
- [egui](https://github.com/emilk/egui) - Immediate mode GUI
- [serdes-ai](https://github.com/...) - LLM integration (local dependency)
- [rusqlite](https://github.com/rusqlite/rusqlite) - SQLite database
- [tokio](https://tokio.rs/) - Async runtime

## License

MIT

## Author

Jan Feddersen
