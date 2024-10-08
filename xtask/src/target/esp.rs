use anyhow::{bail, Result};
use cargo_metadata::camino::Utf8PathBuf;
use std::{env::var, fs};

use super::{Board, EspChip, FlashCapacity, Platform};

const PARTITION_TABLE_TEMPLATE: &str = r#"
# Do not modify this file.
# It is automatically generated by `cargo xtask` at build time.
#
# OmphalOS partition table
#
# Partition type 0x69 is reserved for OFS partitions.
# OFS is the OmphalOS File System.
#
# Name,     Type,   SubType,    Offset,  Size,      Flags
nvs,        data,   nvs,        0x9000,   0x6000,
phy_init,   data,   phy,        0xf000,   0x1000,
kernel,     app,    factory,    0x10000,  0x40000,
system,     0x69,   0x00,       0x50000,  0x150000,
user,       0x69,   0x01,       0x200000, {user_part_size},
"#;

const ESPFLASH_TEMPLATE: &str = r#"
# Do not modify this file.
# It is automatically generated by `cargo xtask` at build time.

partition_table = {partition_table_path}

[connection]
[[usb_device]]
vid = "{usb_vid}"
pid = "{usb_pid}"

[flash]
size = "{flash_size}"
"#;

pub fn write_board_config(board: Board, output_dir: &Utf8PathBuf) -> Result<()> {
    match board.platform() {
        Platform::Esp(chip) => {
            let esp_board = match board.esp_board() {
                Some(esp_board) => esp_board,
                None => bail!("non-ESP board in esp::write_board_config"),
            };

            let (usb_vid, usb_pid) = match chip {
                EspChip::Esp32s3 => ("303a", "1001"),
                _ => unimplemented!("no USB VID/PID for {:?}", chip),
            };

            // write the partition table
            let partition_table_path = output_dir.join("partition_table.csv");
            let partition_table = PARTITION_TABLE_TEMPLATE
                .replace("{user_part_size}", esp_board.flash.user_part_size_str());
            fs::write(&partition_table_path, partition_table)?;

            // write the espflash config
            let espflash_path = output_dir.join("espflash.toml");
            let espflash_toml = ESPFLASH_TEMPLATE
                .replace(
                    "{partition_table_path}",
                    format!("{:?}", partition_table_path).as_str(),
                )
                .replace("{usb_vid}", usb_vid)
                .replace("{usb_pid}", usb_pid)
                .replace("{flash_size}", esp_board.flash.flash_size_str());
            fs::write(espflash_path, espflash_toml)?;

            Ok(())
        }
        _ => {
            panic!("non-ESP platform in esp::write_board_config");
        }
    }
}
