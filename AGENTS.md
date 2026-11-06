# Agent notes

Firmware for two ESP32-C3 boards, built as two binaries from one crate: `control_panel` and `meeting_sign`. They talk over UART with `postcard` messages framed by COBS, so a change to a shared message type needs both binaries rebuilt and reflashed. The README's link to `embedded-meeting-sign` points at the old Arduino Nano version of the sign; the `meeting_sign` binary here replaces it.

## Hardware commands

- `cargo run --release --bin <name>` flashes the connected board through `espflash` and then streams its logs until Ctrl-C. Give the command to me to run instead of running it in a shell that blocks.
- `cargo test --config .cargo/probe-rs.toml` runs the tests on a real board through `probe-rs`. Run `probe-rs list` first, and run the tests only with a board attached and a timeout on the command.

## Before committing

CI (`.github/workflows/rust_ci.yml`) runs a release build, `fmt --check` and clippy with `-D warnings`. Run those same three before a commit; nothing else gates it.

## Docs register

car-sign and random-laser-pointer name this repo as the model for their writing register: terse, command-first, reasons inline, no promotional adjectives. Keep README edits in that register.
