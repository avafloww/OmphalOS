#![allow(unused)]
use crate::{
    build::{ToolArgs, ToolArgsProvider},
    cow,
};
use clap::ValueEnum;

pub mod esp;

#[derive(ValueEnum, Debug, Clone, Copy)]
#[value(rename_all = "snake_case")]
pub enum Board {
    LilygoTDeck,
    LilygoTWatchS3,
}

impl Board {
    pub fn feature_name(&self) -> String {
        format!(
            "target-{}",
            match self {
                Board::LilygoTDeck => "lilygo_t_deck",
                Board::LilygoTWatchS3 => "lilygo_t_watch_s3",
            }
        )
    }

    pub fn platform(&self) -> Platform {
        match self {
            Board::LilygoTDeck => Platform::Esp(EspChip::Esp32s3),
            Board::LilygoTWatchS3 => Platform::Esp(EspChip::Esp32s3),
        }
    }

    pub fn esp_board(&self) -> Option<EspBoard> {
        match self {
            Board::LilygoTDeck => Some(EspBoard {
                flash: FlashCapacity::Size16M,
                psram: PsramCapacity::Opsram8M,
            }),
            Board::LilygoTWatchS3 => Some(EspBoard {
                flash: FlashCapacity::Size16M,
                psram: PsramCapacity::Opsram8M,
            }),
        }
    }
}

impl ToolArgsProvider for Board {
    fn tool_args(&self) -> Option<ToolArgs> {
        Some(ToolArgs {
            cargo_flags: Some(cow!("--features", self.feature_name())),
            ..Default::default()
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Arch {
    Xtensa,
    RV32IMC,
    RV32IMAC,
}

impl Arch {
    /// Returns the Rust target architecture name for the architecture.
    pub fn target_arch(&self) -> &'static str {
        match self {
            Arch::Xtensa => "xtensa",
            Arch::RV32IMC => "riscv32imc",
            Arch::RV32IMAC => "riscv32imac",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Platform {
    Esp(EspChip),
}

impl Platform {
    pub fn target_triple(&self) -> String {
        match self {
            Platform::Esp(chip) => match chip.arch() {
                Arch::Xtensa => format!("{}-{}-none-elf", chip.arch().target_arch(), chip.name()),
                _ => format!("{}-unknown-none-elf", chip.arch().target_arch()),
            },
        }
    }

    // (command, args)
    pub fn run_command<'a>(&self) -> (&'a str, Vec<&'a str>) {
        match self {
            Platform::Esp(_) => (
                "espflash",
                vec![
                    "flash",
                    "--target-app-partition",
                    "kernel",
                    "--monitor",
                    "./kernel",
                ],
            ),
        }
    }
}

impl ToolArgsProvider for Platform {
    fn tool_args(&self) -> Option<ToolArgs> {
        let toolchain = match self {
            Platform::Esp(_) => "esp",
        };

        let rustflags = match self {
            Platform::Esp(_) => {
                cow!("-C", "link-arg=-Tlinkall.x", "-C", "force-frame-pointers")
            }
        };

        Some(ToolArgs {
            rustc_rustflags: Some(rustflags),
            cargo_toolchain: Some(toolchain.into()),
            cargo_flags: Some(cow!(
                "--target",
                self.target_triple(),
                "-Zbuild-std=core,alloc"
            )),
            ..Default::default()
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub enum EspChip {
    Esp32s3,
    Esp32c3,
    Esp32c6,
}

impl EspChip {
    pub fn name(&self) -> &'static str {
        match self {
            EspChip::Esp32s3 => "esp32s3",
            EspChip::Esp32c3 => "esp32c3",
            EspChip::Esp32c6 => "esp32c6",
        }
    }

    pub fn arch(&self) -> Arch {
        match self {
            EspChip::Esp32s3 => Arch::Xtensa,
            EspChip::Esp32c3 => Arch::RV32IMC,
            EspChip::Esp32c6 => Arch::RV32IMAC,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EspBoard {
    pub flash: FlashCapacity,
    pub psram: PsramCapacity,
}

#[derive(Debug, Clone, Copy)]
pub enum FlashCapacity {
    Size4M,
    Size8M,
    Size16M,
}

impl FlashCapacity {
    pub fn flash_size_str(&self) -> &'static str {
        match self {
            FlashCapacity::Size4M => "4MB",
            FlashCapacity::Size8M => "8MB",
            FlashCapacity::Size16M => "16MB",
        }
    }

    pub fn user_part_size_str(&self) -> &'static str {
        match self {
            FlashCapacity::Size4M => "0x200000",
            FlashCapacity::Size8M => "0x600000",
            FlashCapacity::Size16M => "0xE00000",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum PsramCapacity {
    None,
    Psram2M,
    Psram4M,
    Psram8M,
    Opsram2M,
    Opsram4M,
    Opsram8M,
    Opsram16M,
}
