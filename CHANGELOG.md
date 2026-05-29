# Changelog

All notable changes to OmnySSH are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
Versions follow [Semantic Versioning](https://semver.org/).

---

## 1.0.4 — 2026-05-29

### Features
- **Terminal no longer prompts for the password on password-auth hosts**: When a host is configured with a stored password, the interactive terminal now supplies it automatically via `SSH_ASKPASS` instead of asking on every connection (metrics, quick-commands, and snippets already did this). Keys and the SSH agent are still tried first; the password is only used as a fallback. The password is passed to `ssh` through the child process environment — never written to disk, shown on the command line, or sent to the remote.

### Bug Fixes
- **Log files no longer fill the disk**: On startup OmnySSH now prunes rolling log files older than 7 days from its config directory. The cleanup is best-effort and never blocks startup, works on every platform via the native config path, and only touches `omnyssh.log*` files — `config.toml`, `hosts.toml`, and `snippets.toml` are left untouched.
- **Man page now installs reliably**: `install.sh` failed to install the man page into the system directory (e.g. `/usr/local/share/man` on macOS) because it never elevated with `sudo`, so `man omny` reported "No manual entry". The man page is now downloaded to a temp file first and installed with a `sudo` fallback, mirroring the binary install.

### Other
- Removed the vestigial empty `package.json` and `package-lock.json` (left over from an earlier project name); they served no purpose in this Rust project.

---

## 1.0.3 — 2026-05-21

### Features
- **Termux / Android support**: Releases now include a statically linked `aarch64-unknown-linux-musl` build that runs on Termux. `install.sh` detects Termux via `$TERMUX_VERSION` / `$PREFIX` and installs the binary into `$PREFIX/bin` (and the man page into `$PREFIX/share/man/man1`) without `sudo`.

---

## 1.0.2 — 2026-05-16

### Features
- **Automatic update checks**: On startup OmnySSH checks GitHub Releases for a newer version and shows a popup when one is available. You can install the update, skip that version, or disable checks entirely. Failed or offline checks are silent and never delay startup.
- **In-app self-update**: For manual / `install.sh` installs on Linux and macOS, an update can be downloaded and installed from within the app — the release archive is verified against its SHA-256 checksum before the binary is replaced. Homebrew, Cargo, and Nix installs instead show the matching upgrade command.
- **Top processes on the detail page**: The server detail view now shows the three busiest processes by CPU usage, along with their CPU and memory percentages.

### Bug Fixes
- **Windows double input fixed**: Each keystroke is now registered once instead of twice (e.g. "j" no longer produced "jj"). Key-release events reported by the Windows console are no longer treated as input.
- **Ubuntu 22.04 compatibility**: Linux release binaries are now built against an older glibc, fixing the `version 'GLIBC_2.39' not found` error when running on Ubuntu 22.04 and similarly aged distributions.
- **Mouse scroll inside full-screen apps fixed**: On the Terminal screen, the mouse wheel now scrolls inside `vim`, `less`, `htop`, and other alternate-screen apps. The wheel is forwarded to the foreground application — as native mouse-wheel events when it enabled mouse reporting, or as cursor-key presses otherwise. The normal screen still scrolls local scrollback.
- **Multi-line paste into the terminal fixed**: Bracketed paste is now implemented. Pasting multi-line text into the Terminal screen no longer drops the first characters, and editors like `vim` insert it verbatim without cascading auto-indent.
- **Top processes exclude OmnySSH's own connection**: The detail-page top-processes panel no longer lists OmnySSH's metric-polling SSH connection. Its `sshd` process chain is filtered out by PID, so an idle server shows real workload while SSH sessions from other users still appear.
- **Bracketed paste restored after system SSH**: Connecting to a host via the system `ssh` binary no longer leaves bracketed paste disabled on return, so multi-line paste into the terminal keeps working.
- **Host keys are now verified**: A server's host key is recorded in `~/.ssh/known_hosts` on first connection (trust on first use). A later key change — or an unreadable `known_hosts` — refuses the connection instead of silently accepting an unverified key.
- **Password authentication reliably disabled**: Auto SSH Key Setup now adds the `PasswordAuthentication`, `UsePAM`, and challenge/keyboard-interactive directives when `sshd_config` omits them, so password login is disabled even on configs that previously relied on compiled-in defaults.
- **System SSH launch failure no longer quits the app**: If the `ssh` binary cannot be started, OmnySSH restores the TUI and reports the error in the status bar instead of exiting.
- **Remote command exit status honoured**: SSH command execution now exposes a non-zero remote exit status; the key-setup sudo check uses it to detect missing sudo access correctly.

### Other
- Release archives are now published alongside a `SHA256SUMS` checksum file.
- **Internal refactor**: The oversized `app.rs` was split into focused submodules under `src/app/` (host, snippets, file manager, terminal, update, input, and action dispatch). This is a pure code reorganization with no change in behaviour.
- **Test coverage**: Added unit tests for previously untested core logic — snippet parameter substitution, host/snippet form validation, the SSH-config/manual host merge, `hosts.toml` serialization, form-field UTF-8 editing, host/snippet filtering and sorting, file-panel selection, and terminal pane state.

---

## 1.0.1 — 2026-04-22

### Bug Fixes
- **TUI display corruption fixed**: Log output no longer bleeds through the TUI interface. All logging is now redirected to a log file (`~/.config/omnyssh/omnyssh.log` on Linux, `~/Library/Application Support/omnyssh/omnyssh.log.*` on macOS) instead of stderr, preventing raw error messages (such as SSH timeout warnings) from corrupting the terminal display during background operations.
- **Error notifications**: Connection failures, discovery timeouts, and snippet execution errors are now displayed as concise notifications in the status bar instead of being silently logged.
- **SFTP connection freeze fixed**: SFTP connections now run in the background with a 30-second timeout, preventing the UI from freezing indefinitely when connecting to slow or unresponsive servers. A "Connecting… (30s timeout)" indicator is displayed during the connection attempt.
- **Terminal scroll fixed**: Two-finger trackpad and mouse-wheel scroll on the Terminal screen now scrolls local scrollback instead of cycling the remote shell's command history. Previously, mouse capture was disabled on the Terminal screen to allow native mouse text selection, which caused host terminal emulators to translate scroll gestures into ArrowUp/ArrowDown keys that bash readline interpreted as history navigation. Mouse capture is now kept on across all screens.
- **Native drag-to-select preserved**: Mouse capture now enables only button and scroll-wheel reporting (`?1000h` + `?1006h`), dropping the aggressive any-motion tracking (`?1002h` / `?1003h`) that crossterm enables by default. In terminals that honor the modifier-bypass for mouse reporting (iTerm2 on macOS, most Linux terminals), hold `Option` (iTerm2) or `Shift` (Linux) while dragging to select and copy text in the Terminal screen without the application intercepting the drag. Note: macOS Terminal.app does not support modifier-bypass for mouse reporting at all — users on Terminal.app should switch to iTerm2 or a similar emulator for in-app text selection.

---

## 1.0.0 — 2026-04-18

First production-ready release of OmnySSH.

### Features

#### Dashboard
- Server cards with live **CPU / RAM / Disk** metrics, uptime, and load average
- Colour-coded thresholds: 🟢 < 60%, 🟡 60–85%, 🔴 > 85%
- Async metrics collection — each host polled independently via SSH
- Cross-platform parsers: Linux (`top`/`free`/`/proc/stat`), macOS (`vm_stat`), Alpine BusyBox
- Configurable poll interval (default 30 s) with exponential backoff on failure
- Sort by name / CPU / RAM / status (`s`)
- Filter by tag (`t`)
- Manual refresh (`r`)
- Connection status indicator: `●` online, `◐` connecting, `✗` failed
- Connection pool: one SSH connection per host, reused for all metrics

#### Host management
- Host list with instant fuzzy search (`/`)
- Automatic import from `~/.ssh/config` (Host, HostName, User, Port, IdentityFile, ProxyJump, Include)
- Add / Edit / Delete hosts via TUI forms
- Tags and notes for each host
- Persistence in `~/.config/omnyssh/hosts.toml` — original `~/.ssh/config` is never modified
- Delete confirmation popup

#### File Manager (SFTP)
- Split-panel browser: local files ↔ remote SFTP
- Directory navigation with `h/j/k/l` and arrow keys
- File operations: upload, download, delete, mkdir, rename
- Progress bar with percentage for transfers
- Multiple file selection with `Space`
- Copy (`c`) / Paste (`p`) across panels
- Plain-text file preview
- Host-picker popup for remote panel

#### Snippets
- Save, edit, and delete global and host-scoped command snippets
- Parameterised snippets with `{{placeholder}}` syntax
- Quick-execute (`x`): run ad-hoc commands from the Dashboard
- Broadcast mode (`b`): execute on multiple hosts in parallel
- Fuzzy search on the Snippets screen
- Persistence in `~/.config/omnyssh/snippets.toml`

#### Multi-session terminal
- PTY-backed terminal with tabs (`Ctrl+T` / `Ctrl+W`)
- Split-view: `Ctrl+\` vertical, `Ctrl+-` horizontal
- Tab navigation with `Ctrl+Right` / `Ctrl+Left`
- Activity indicator on tabs with unseen output
- Full VT100 terminal emulation (`portable-pty` + `vt100`)
- Non-blocking render — terminal never freezes the UI

#### Themes & Configuration
- 4 built-in colour themes: `default`, `dracula`, `nord`, `gruvbox`
- `--theme <THEME>` CLI flag to override theme at runtime: `omny --theme dracula`
- Fully configurable keybindings via `[keybindings]` in config
- `--config <FILE>` flag to load a custom config
- `--help` / `--version` flags

#### General
- Cross-platform: Linux, macOS, Windows (single static binary)
- Panic hook that restores the terminal before printing backtrace
- `russh`-based async SSH client (no external `ssh` binary dependency for metrics)
- CI: GitHub Actions matrix for Ubuntu, macOS, Windows

---

## Development history

| Date | Version | Milestone |
|------|---------|-----------|
| 2026-04-04 | `0.0.1` | Project skeleton — TUI shell, event loop, placeholder screens |
| 2026-04-05 | `0.1.0` | Host list, SSH connect, fuzzy search — first MVP |
| 2026-04-06 | `0.2.0` | Live metrics dashboard with async polling |
| 2026-04-07 | `0.3.0` | Command snippets, quick-execute, broadcast |
| 2026-04-08 | `0.4.0` | SFTP file manager with split-panel UI |
| 2026-04-09 | `0.5.0` | Multi-session PTY tabs and split-view |
| 2026-04-10 | **`1.0.0`** | **Themes, configurable keybindings, production release** |
