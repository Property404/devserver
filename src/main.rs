use anyhow::{bail, Result};
use clap::Parser;
use std::env;
use std::fmt::Display;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use std::path::PathBuf;
use std::process::Command;

const DEFAULT_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Arguments {
    /// Build command to run when source files change
    #[clap(short, long)]
    build: Option<String>,
    /// Don't automatically reload
    #[clap(long)]
    noreload: bool,
    /// The address/port
    #[clap(short, long, default_value_t=DEFAULT_ADDR)]
    address: SocketAddr,
    /// The path to serve
    #[clap(long)]
    path: Option<PathBuf>,
    /// Extra headers to serve
    #[clap(long)]
    header: Vec<String>,
}

fn main() -> Result<()> {
    let args = Arguments::parse();
    let mut actions = Vec::new();
    if let Some(cmd) = args.build {
        let func: devserver::Action = Box::new(move || {
            let mut command = Command::new("sh");
            let command = command.arg("-c").arg(&cmd);
            let output = command.output().map_err(|err| {
                let err: Box<dyn Display> = Box::new(err);
                err
            })?;
            let stdout = String::from_utf8(output.stdout).unwrap();
            print!("{stdout}");
            let stderr = String::from_utf8(output.stderr).unwrap();
            print!("{stderr}");
            if !output.status.success() {
                return Err(Box::new(format!("{stdout}{stderr}")));
            }
            Ok(())
        });
        actions.push(func);
    };

    let mut headers = String::new();
    for header in args.header.iter().cloned().map(|h| {
        if !h.contains(':') {
            bail!("Header must contain ':'");
        }
        if h.contains('\r') || h.contains('\n') {
            bail!("Header cannot contain line breaks");
        }
        if !h.is_ascii() {
            bail!("Header must be ASCII");
        }
        Ok(h)
    }) {
        headers.push_str("\r\n");
        headers.push_str(&header?);
    }

    let hosted_path = env::current_dir()
        .unwrap()
        .join(args.path.unwrap_or_default());

    if !hosted_path.exists() {
        bail!("Path [{}] does not exist!", hosted_path.display());
    }

    println!(
        "\nServing [{}] at [ https://{} ] or [ http://{} ]",
        hosted_path.display(),
        args.address,
        args.address
    );

    if !args.noreload {
        println!("Automatic reloading is enabled!");
    }

    println!("Stop with Ctrl+C");

    devserver::run(
        args.address.ip(),
        args.address.port(),
        &hosted_path,
        !args.noreload,
        &headers,
        actions,
    );

    Ok(())
}
