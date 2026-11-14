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

# Test the plasmoid (untranslated; see "Translating the widget" for testing translations)
cd ../plasmoid
plasmoidviewer -a .
```

## Protocol & reverse engineering

The AirPods protocol implementation lives in [`service/src/airpods/`](service/src/airpods/); see [D-Bus examples](DBUS_EXAMPLES.md) for using the service.

No special hardware is needed to investigate the protocol. On macOS, searching strings in `/usr/sbin/bluetoothd` for AirPods or AAP identifies the Bluetooth daemon's protocol handler. A decompiler such as IDA Pro, Ghidra, or Binary Ninja can then be used to trace a feature's command handler and packet construction (for example, listening mode when toggling ANC).

Alternatively, capture Bluetooth packets with PacketLogger from Xcode's Additional Tools on macOS, or enable the HCI snoop log in Android Developer Options. Compare packets while toggling features to identify commands and values; this approach does not require reverse engineering experience.

See the [maintainer's explanation](https://github.com/can1357/kAirPods/issues/8) for more detail.

## Translating the widget

Translations are gettext catalogs in `plasmoid/translate/`: `template.pot` lists every string, and each language has a `<lang>.po` file (`de.po`, `nl.po`, ...). Only `.po` files are committed; installers compile them with `plasmoid/translate/build.sh` into `plasmoid/contents/locale/<lang>/LC_MESSAGES/plasma_applet_org.kairpods.plasma.mo`, which is where Plasma looks for this widget's translations.

1. **Start a language** (skip if `<lang>.po` already exists; needs `gettext`):

   ```bash
   cd plasmoid/translate
   msginit --no-translator --locale=de --input=template.pot --output-file=de.po
   ```

2. **Translate** `de.po` with Lokalize, Poedit or a text editor. Keep placeholders such as `%1` and `%2`; `msgctxt` lines describe where a short string appears. To translate the widget's name and description in the widget explorer, add `"Name[de]"` and `"Description[de]"` next to `Name`/`Description` in the `KPlugin` section of `plasmoid/metadata.json`.

3. **Test** by installing the widget and loading it by id. Loading it from a source directory (`plasmoidviewer -a .`) does not pick up bundled translations on Plasma 6.

   ```bash
   plasmoid/translate/build.sh
   kpackagetool6 --type Plasma/Applet --upgrade plasmoid || kpackagetool6 --type Plasma/Applet --install plasmoid
   LANGUAGE=de LANG=de_DE.UTF-8 plasmoidviewer -a org.kairpods.plasma
   ```

   Set `LANGUAGE`: Plasma sessions export it and it takes priority over `LANG`. A running plasmashell loads catalogs once, so restart it (`systemctl --user restart plasma-plasmashell`) to see updated translations on the panel.

4. **Submit** a pull request with the `.po` file (and any `metadata.json` additions).

### Adding or changing strings

Wrap every user-visible QML string: `i18n("Text")`, `i18nc("context for translators", "L")` for short or ambiguous strings, `i18np("%1 device", "%1 devices", count)` for counts, and `%1` placeholders instead of string concatenation. Then run `plasmoid/translate/merge.sh` and commit the updated `template.pot` and `.po` files; CI fails when the template is out of date.

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

- Run unit tests: `cargo test` (when available)
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
