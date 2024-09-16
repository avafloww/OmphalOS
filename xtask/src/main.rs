use clap::{Args, Parser};

mod build;
mod target;

#[derive(Parser, Debug)]
#[command(bin_name = "cargo xtask", about = "OmphalOS build system")]
enum MainCommand {
    Build(BuildCommand),
    Run(RunCommand),
}

#[derive(Args, Debug)]
struct TargetArgs {
    #[arg(short, long)]
    pub board: target::Board,

    #[arg(short, long)]
    pub profile: Option<build::Profile>,
}

#[derive(Args, Debug)]
struct BuildCommand {
    #[command(flatten)]
    pub target_args: TargetArgs,
}

#[derive(Args, Debug)]
struct RunCommand {
    #[command(flatten)]
    pub target_args: TargetArgs,
}

fn main() -> anyhow::Result<()> {
    match MainCommand::parse() {
        MainCommand::Build(m) => build::Builder::new(build::Action::Build, m.target_args)?.go(),
        MainCommand::Run(m) => build::Builder::new(build::Action::Run, m.target_args)?.go(),
    }
}

// cow!("--target", target_triple)
// becomes:
// vec!["--target".into(), target_triple.into()]
#[macro_export]
macro_rules! cow {
    ($($e:expr),*) => {
        vec![$($e.into()),*]
    };
}
