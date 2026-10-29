# DT - Developer Tools

Rust CLI tools for git and development workflows, converted from shell scripts for better performance, error handling, and user experience.

## 🚀 Quick Installation

### Option 1: Automatic Install (Recommended)
```bash
# Clone or download this repository
git clone <repository-url>
cd <repository-name>

# Quick install script
./install.sh
```

### Option 2: Manual Install
```bash
# Build and install to ~/.local/bin
make install

# OR build optimized release version manually
cargo build --release
cp target/release/dt ~/.local/bin/
```

### Option 3: Custom Location
```bash
# Install to any directory you prefer
cargo build --release
cp target/release/dt /your/preferred/bin/directory/

# Make sure your chosen directory is in PATH
export PATH="/your/preferred/bin/directory:$PATH"
```

## 🛤️ Add to PATH (One-time setup)

Make sure your installation directory is in your PATH so you can use `dt` from anywhere:

### Check if already in PATH:
```bash
echo $PATH | grep -q "$HOME/.local/bin" && echo "✅ Already in PATH" || echo "❌ Need to add to PATH"
```

### Add ~/.local/bin to PATH permanently:
```bash
# For bash users
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc

# For zsh users  
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc

# Apply immediately
export PATH="$HOME/.local/bin:$PATH"
```

### Verify Installation:
```bash
# Check if dt is found
which dt

# Test basic functionality
dt --help
```

## 📖 Usage

### Checkout - Smart Git Branch Switching
```bash
# Smart checkout that handles remote/local branches automatically
dt checkout feature-branch

# Create new branch (equivalent to git checkout -b)
dt checkout -b new-feature

# Preview what would happen (safe testing)
dt checkout feature-branch --dry-run
```

**Smart Logic:**
- If branch exists locally → switches to it
- If branch exists on remote but not locally → creates tracking branch
- If branch doesn't exist anywhere → creates new local branch
- Preserves all original `gitcheckout.sh` behavior

### Squash - Interactive Git Squash
```bash
# Squash last 3 commits interactively
dt squash 3

# Squash with specific message
dt squash 3 -m "Combined feature implementation"

# Preview what would be squashed (safe testing)
dt squash 3 --dry-run
```

**Interactive Features:**
- Shows preview of commits to be squashed
- Prompts for commit message with smart defaults
- Confirms before executing
- Automatic rollback on failure
- Preserves all original `squash.sh` behavior

### Rovo - CLI Wrapper
```bash
# Default: Pass through to acli rovodev
dt rovo status
dt rovo run
dt rovo [any acli rovodev args]

# Special: Login workflow
dt rovo login
```

**Wrapper Features:**
- **Default behavior**: Passes all arguments to `acli rovodev`
- **Special `login` subcommand**: Handles authentication and launches rovodev
- Preserves all original `rovo.sh` behavior

### Skills - Manage Claude Skills
```bash
dt skills                                  # List all skills: on/off, model-invocable, where they live
dt skills list --json                      # Machine-readable
dt skills list --repo ~/code/other-repo    # Also show another repo's project skills
dt skills disable unslop                   # Turn a global skill off (reversible)
dt skills enable unslop
dt skills install owner/repo --off         # Install from GitHub, disabled
dt skills install https://host/x.git --path skills/foo
dt skills install ./my-skill --on          # Install from a local folder, enabled
```

**How it works:**
- Skills are found in `~/.claude/skills` (global), `~/.claude/skills/synced` (claude.ai synced), `.claude/skills` in the current git repo (project) and installed plugins
- Disabling sets `"skillOverrides": {"name": "off"}` in `~/.claude/settings.json`; skill files are never touched and other settings are preserved
- **Project and plugin skills are shown as `[locked]`** and can't be toggled here
- `install` without `--on`/`--off` asks interactively; it refuses to overwrite without `--force`
- Respects `CLAUDE_CONFIG_DIR`

## 🧪 Safe Testing with Dry-Run

All destructive operations support `--dry-run` for safe testing:

```bash
dt checkout new-branch --dry-run    # Shows what would happen
dt squash 5 --dry-run               # Preview commits without changes
```

Perfect for understanding behavior before making actual changes!

## 🔧 Development (For Contributors)

### Quick Development Workflow
```bash
make dev               # Fast debug build + install for testing
make watch             # Auto-rebuild on file changes
make test-all          # Test all commands quickly
make coverage          # Generate test coverage reports (auto-installs tarpaulin)
```

### Individual Command Testing
```bash
make test-checkout     # Test checkout command
make test-squash       # Test squash command  
make test-rovo         # Test rovo command
```

### Git Pre-commit Hook (Optional)
```bash
# Copy the pre-commit hook for automatic checks
cp hooks/pre-commit .git/hooks/
chmod +x .git/hooks/pre-commit
```

## ✨ Key Features

- **⚡ Fast**: Rust performance with optimized builds
- **🛡️ Safe**: Dry-run mode for all destructive operations  
- **🎨 Rich UX**: Colored output with emojis for clear feedback
- **📚 Self-documenting**: Comprehensive `--help` for all commands
- **🔄 Robust**: Graceful error handling with automatic rollback
- **🔍 Transparent**: Shows exactly what will happen before doing it

## 📋 Requirements

### Required for All Users
- **Git** - Required for `dt checkout` and `dt squash` commands
  - Most systems: `git --version` to check if installed
  - Install: [git-scm.com](https://git-scm.com/downloads)

### Required for Building from Source
- **Rust 1.70+** - Required to build the `dt` binary
  - Check: `rustc --version`
  - Install: [rustup.rs](https://rustup.rs/)
  - Includes `cargo` build tool

### Optional (for Development)
- **cargo-tarpaulin** - For test coverage reports
  - Install: `cargo install cargo-tarpaulin` or use `make install-coverage`
  - Auto-installed when running `make coverage`

### Optional (for Rovo functionality)
- **acli** - Required only for `dt rovo` commands
  - Check: `acli --version` 
  - Without acli: `dt rovo` commands will show an error message
  - With acli: Full rovo functionality available

### Runtime Dependencies
- **Shell with PATH support** - bash, zsh, fish, etc.
- **Standard Unix tools** - which, echo (usually pre-installed)

### Platform Support
- ✅ **macOS** - Fully supported
- ✅ **Linux** - Fully supported  
- ✅ **Windows** - Should work (untested)
- ✅ **WSL** - Fully supported

## 🆘 Troubleshooting

### Command not found
```bash
# Check if dt is in PATH
which dt

# If not found, add your installation directory to PATH
export PATH="/your/installation/directory:$PATH"
```

### Permission issues
```bash
# Make sure the binary is executable
chmod +x ~/.local/bin/dt

# Check directory permissions
ls -la ~/.local/bin/
```

### Build issues
```bash
# Clean and rebuild
cargo clean
cargo build --release
```
