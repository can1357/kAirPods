# Contributing to kAirPods

Thank you for your interest in contributing to kAirPods! This document provides guidelines for contributing to the project.

## Getting Started

1. Fork the repository on GitHub
2. Clone your fork locally
3. Create a new branch for your feature or bugfix
4. Make your changes
5. Test thoroughly
6. Submit a pull request

## Development Setup

### Prerequisites

- Rust 1.88+
- KDE Plasma 6 development packages
- Qt 6.5+ development tools
- BlueZ development headers

### Building

```bash
# Build the service
cd service
cargo build

# Test the plasmoid
cd ../plasmoid
plasmoidviewer -a .
```

## Code Style

### Rust

- Follow standard Rust formatting (`cargo fmt`)
- Use `cargo clippy` to catch common issues
- Write idiomatic Rust code
- Add documentation comments for public APIs

### QML

- Follow KDE QML coding style
- Use consistent indentation (4 spaces)
- Prefer declarative style over imperative
- Keep components focused and reusable

## Testing

- Test with different AirPods models if possible
- Verify D-Bus interface functionality
- Check memory usage and performance
- Test error handling and edge cases

## Submitting Changes

1. **Commit Messages**

   - Use clear, descriptive commit messages
   - Reference issue numbers when applicable
   - Keep commits focused and atomic

2. **Pull Requests**
   - Describe what changes you've made
   - Explain why the changes are necessary
   - Include screenshots for UI changes
   - Ensure all tests pass

## Reporting Issues

- Use the GitHub issue tracker
- Include system information (Plasma version, distro, etc.)
- Provide steps to reproduce
- Include relevant logs from `journalctl --user -u kairpodsd`

## Code of Conduct

- Be respectful and constructive
- Welcome newcomers and help them get started
- Focus on what is best for the community
- Show empathy towards other community members

## Questions?

Feel free to open an issue for any questions about contributing.
