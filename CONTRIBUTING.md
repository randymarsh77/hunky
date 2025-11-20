# Contributing to Git Stream

Thank you for your interest in contributing to Git Stream! This document provides guidelines and information for contributors.

## Development Setup

### Using Nix (Recommended)

1. Install Nix with flakes enabled
2. Clone the repository
3. Enter the development environment:
   ```bash
   nix develop
   ```

### Using Cargo Directly

1. Install Rust toolchain (1.70+)
2. Clone the repository
3. Build:
   ```bash
   cargo build
   ```

## Project Structure

- `src/main.rs` - Entry point and async runtime setup
- `src/app.rs` - Main application state and event loop
- `src/git.rs` - Git operations using libgit2
- `src/diff.rs` - Data structures for diffs and hunks
- `src/watcher.rs` - File system watching with notify
- `src/syntax.rs` - Syntax highlighting integration
- `src/ui.rs` - TUI rendering with ratatui

## Code Style

- Follow standard Rust conventions
- Run `cargo fmt` before committing
- Run `cargo clippy` to catch common issues
- Add comments for complex logic
- Keep functions focused and testable

## Testing

Currently, the project relies on manual testing:

```bash
# Run in debug mode
cargo run

# Run in another terminal
./demo.sh
```

Future: We'll add automated tests for core functionality.

## Making Changes

1. **Fork the repository**
2. **Create a feature branch**
   ```bash
   git checkout -b feature/your-feature-name
   ```
3. **Make your changes**
4. **Test thoroughly**
   - Run the application
   - Try all key bindings
   - Test with real git changes
5. **Commit with clear messages**
   ```bash
   git commit -m "feat: add search functionality"
   ```
6. **Push and create a pull request**

## Commit Message Convention

Follow conventional commits:

- `feat:` - New feature
- `fix:` - Bug fix
- `docs:` - Documentation only
- `style:` - Code style changes (formatting)
- `refactor:` - Code refactoring
- `perf:` - Performance improvements
- `test:` - Adding tests
- `chore:` - Maintenance tasks

## Areas for Contribution

### High Priority

- [ ] Automated tests (unit and integration)
- [ ] Better error handling and user feedback
- [ ] Performance optimization for large diffs
- [ ] Configuration file support

### Feature Additions

- [ ] Enhanced syntax highlighting in diffs
- [ ] Search functionality
- [ ] Filter by file pattern
- [ ] Snapshot save/load
- [ ] Git staging from TUI
- [ ] Mouse support
- [ ] Custom themes

### Documentation

- [ ] More usage examples
- [ ] Video demonstrations
- [ ] Architecture documentation
- [ ] API documentation

### Platform Support

- [ ] Windows testing and fixes
- [ ] Linux testing and fixes
- [ ] macOS optimization

## Code Review Process

1. All changes require review
2. CI checks must pass (when implemented)
3. Maintain backward compatibility
4. Document breaking changes clearly

## Questions?

Open an issue for:
- Bug reports
- Feature requests
- Questions about the codebase
- Suggestions for improvements

## License

By contributing, you agree that your contributions will be licensed under the same license as the project.

---

Happy coding! 🦀
