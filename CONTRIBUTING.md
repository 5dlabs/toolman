# Contributing to Toolman

Thank you for your interest in contributing to Toolman! We welcome contributions from the community.

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/your-username/toolman.git`
3. Create a feature branch: `git checkout -b feature/your-feature-name`
4. Make your changes
5. Test thoroughly (see Testing section)
6. Commit with descriptive messages
7. Push to your fork: `git push origin feature/your-feature-name`
8. Create a Pull Request

## Development Setup

### Prerequisites

- Rust 1.75+ (use `rustup` to install)
- Node.js 18+ (for npm-based MCP servers)
- Python 3.8+ (for Python-based MCP servers)

### Building

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Run tests
cargo test

# Run clippy (linter)
cargo clippy --all-features -- -D warnings

# Format code
cargo fmt
```

## Testing

### Unit Tests

All new functionality should include unit tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_your_feature() {
        // Your test here
    }
}
```

### Integration Tests

Integration tests go in the `tests/` directory:

```bash
cargo test --test '*'
```

### Manual Testing

**IMPORTANT**: Features must be tested in Cursor IDE:

1. Build the release binaries
2. Start the HTTP server
3. Configure Cursor to use Toolman
4. Test your changes in the actual UI
5. Verify tools appear and function correctly

## Code Style

- Follow Rust standard style guidelines
- Use `cargo fmt` before committing
- Keep functions focused and small
- Add documentation comments for public APIs
- Use meaningful variable and function names

## Pull Request Guidelines

### PR Title

Use conventional commit format:
- `feat:` for new features
- `fix:` for bug fixes
- `docs:` for documentation
- `test:` for test additions
- `refactor:` for code refactoring
- `chore:` for maintenance tasks

Example: `feat: add support for new MCP server`

### PR Description

Include:
- What changes were made
- Why the changes were necessary
- How to test the changes
- Screenshots (if UI changes)
- Related issues (use `Fixes #123`)

### PR Checklist

- [ ] Tests pass (`cargo test`)
- [ ] Clippy passes (`cargo clippy`)
- [ ] Code formatted (`cargo fmt`)
- [ ] Documentation updated
- [ ] Tested in Cursor IDE
- [ ] No sensitive data exposed

## Adding New MCP Servers

To add support for a new MCP server:

1. Add server configuration to `servers-config.json`
2. Test tool discovery
3. Document any special requirements
4. Add integration tests
5. Update README with server details

## Reporting Issues

When reporting issues:

1. Check existing issues first
2. Use issue templates
3. Include:
   - Toolman version
   - OS and version
   - Steps to reproduce
   - Expected vs actual behavior
   - Logs (if applicable)

## Security

- Never commit secrets or API keys
- Use environment variables for sensitive data
- Report security issues privately to maintainers

## Questions?

- Open a discussion for general questions
- Check documentation first
- Be respectful and constructive

Thank you for contributing to Toolman! 🛠️