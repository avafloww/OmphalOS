use crate::{
    cow,
    target::{self, Platform},
    TargetArgs,
};
use anyhow::Result;
use cargo_metadata::camino::Utf8PathBuf;
use clap::ValueEnum;
use std::{borrow::Cow, collections::HashMap, process::Command};

#[derive(Debug, Clone, Copy)]
pub enum Action {
    Build,
    Run,
}

#[derive(ValueEnum, Debug, Clone, Copy)]
pub enum Profile {
    Debug,
    Release,
}

impl ToolArgsProvider for Profile {
    fn tool_args(&self) -> Option<ToolArgs> {
        match self {
            Profile::Debug => None,
            Profile::Release => Some(ToolArgs {
                cargo_flags: Some(cow!("--release")),
                ..Default::default()
            }),
        }
    }
}

impl ToString for Profile {
    fn to_string(&self) -> String {
        match self {
            Profile::Debug => "debug".to_string(),
            Profile::Release => "release".to_string(),
        }
    }
}

#[derive(Debug)]
pub struct ToolArgs<'a> {
    pub rustc_rustflags: Option<Vec<Cow<'a, str>>>,
    pub cargo_flags: Option<Vec<Cow<'a, str>>>,
    pub cargo_env: Option<HashMap<Cow<'a, str>, Cow<'a, str>>>,
    pub cargo_toolchain: Option<Cow<'a, str>>,
}

impl Default for ToolArgs<'_> {
    fn default() -> Self {
        Self {
            rustc_rustflags: None,
            cargo_flags: None,
            cargo_env: None,
            cargo_toolchain: None,
        }
    }
}

pub trait ToolArgsProvider {
    fn tool_args(&self) -> Option<ToolArgs>;
}

impl ToolArgs<'_> {
    pub fn combine<'a>(args: Vec<Option<ToolArgs<'a>>>) -> ToolArgs<'a> {
        let mut cargo_toolchain = None;
        let mut rustc_rustflags = Vec::new();
        let mut cargo_flags = Vec::new();
        let mut cargo_env = HashMap::new();

        for arg in args {
            if let Some(arg) = arg {
                if let Some(other_cargo_toolchain) = arg.cargo_toolchain {
                    if cargo_toolchain.is_some() {
                        panic!("Multiple toolchains specified");
                    }

                    cargo_toolchain = Some(other_cargo_toolchain);
                }

                if let Some(other_rustc_rustflags) = arg.rustc_rustflags {
                    other_rustc_rustflags.into_iter().for_each(|flag| {
                        rustc_rustflags.push(flag);
                    });
                }

                if let Some(other_cargo_flags) = arg.cargo_flags {
                    other_cargo_flags.into_iter().for_each(|flag| {
                        cargo_flags.push(flag);
                    });
                }

                if let Some(other_cargo_env) = arg.cargo_env {
                    other_cargo_env.into_iter().for_each(|(key, value)| {
                        cargo_env.insert(key, value);
                    });
                }
            }
        }

        ToolArgs {
            cargo_toolchain,
            rustc_rustflags: if rustc_rustflags.is_empty() {
                None
            } else {
                Some(rustc_rustflags)
            },
            cargo_flags: if cargo_flags.is_empty() {
                None
            } else {
                Some(cargo_flags)
            },
            cargo_env: if cargo_env.is_empty() {
                None
            } else {
                Some(cargo_env)
            },
        }
    }
}

#[derive(Debug)]
pub struct Builder {
    action: Action,
    target: TargetArgs,
    metadata: cargo_metadata::Metadata,
}

impl Builder {
    pub fn new(action: Action, target: TargetArgs) -> Result<Self> {
        let metadata = cargo_metadata::MetadataCommand::new().exec()?;
        Ok(Self {
            action,
            target,
            metadata,
        })
    }

    pub fn go(&mut self) -> Result<()> {
        match self.action {
            Action::Build => self.build(false),
            Action::Run => self.build(true),
        }
    }

    fn build(&self, run: bool) -> Result<()> {
        let profile = self.target.profile.unwrap_or(Profile::Debug);
        let board = self.target.board;
        let platform = board.platform();
        let tool_args = ToolArgs::combine(vec![
            profile.tool_args(),
            board.tool_args(),
            platform.tool_args(),
        ]);

        // set the working directory to the target directory
        // that way, temp files are not created in the workspace root
        let working_directory = self
            .metadata
            .workspace_root
            .join("target")
            .join(platform.target_triple())
            .join(profile.to_string());

        // ensure the working directory and its parents exist
        std::fs::create_dir_all(&working_directory)?;

        cargo("build", "kernel", tool_args, &working_directory)?;
        if run {
            self.run(&working_directory)?;
        }

        Ok(())
    }

    fn run(&self, working_directory: &Utf8PathBuf) -> Result<()> {
        let platform = self.target.board.platform();

        // write out board configs before running
        match platform {
            Platform::Esp(_) => {
                target::esp::write_board_config(self.target.board, working_directory)?
            }
        }

        let (command, args) = platform.run_command();

        Command::new(command)
            .args(args)
            .current_dir(working_directory)
            .spawn()?
            .wait()?;

        Ok(())
    }
}

fn cargo<'a>(
    subcommand: &str,
    project: &str,
    args: ToolArgs<'a>,
    working_directory: &Utf8PathBuf,
) -> Result<()> {
    let mut command = Command::new("cargo");
    command.current_dir(working_directory);

    let mut final_rustflags: Vec<Cow<'a, str>> = Vec::new();

    // set the toolchain
    if let Some(toolchain) = args.cargo_toolchain {
        command.arg(format!("+{}", toolchain));
    }

    // add the right args
    command.arg(subcommand);
    command.arg("-p");
    command.arg(project);

    // add the rest of the cargo flags
    if let Some(cargo_flags) = args.cargo_flags {
        for flag in cargo_flags {
            command.arg(flag.to_string());
        }
    }

    // set all cargo env vars
    if let Some(cargo_env) = args.cargo_env {
        for (key, value) in cargo_env {
            if key == "RUSTFLAGS" {
                panic!("RUSTFLAGS is not allowed in cargo_env; use rustc_rustflags instead");
            }

            command.env(key.to_string(), value.to_string());
        }
    }

    // set RUSTFLAGS in the env
    if let Some(rustc_rustflags) = args.rustc_rustflags {
        final_rustflags.extend(rustc_rustflags);
    }
    command.env("RUSTFLAGS", final_rustflags.join(" ").to_string());

    // execute the command
    command.spawn()?.wait()?;

    Ok(())
}
