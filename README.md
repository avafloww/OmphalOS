# OmphalOS

**OmphalOS** is an hobby operating system that currently primarily targets mobile devices using embedded microcontrollers.

The only microcontroller supported at this time is the ESP32-S3 series; however, other microcontrollers may be
supported in the future.

## Supported Devices

### ESP32-S3

- [LILYGO T-Deck](https://www.lilygo.cc/products/t-deck) (`--board lilygo_t_deck`)
- [LILYGO T-Watch S3](https://www.lilygo.cc/products/t-watch-s3) (`--board lilygo_t_watch_s3`)

## Getting Started

### Prerequisites

- A supported device from the list above
- [Espressif Rust toolchain (installed via espup)](https://docs.esp-rs.org/book/installation/riscv-and-xtensa.html)
- `espflash` (installed via `cargo install espflash`)

### Build & Run

This project uses the `cargo xtask` pattern to build. To build and flash the project to your device, run the following
command from the root of the project:

```sh
cargo xtask run --board <board>
```

## License

This project is licensed under the MIT license.
