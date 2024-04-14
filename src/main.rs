use std::env;
use std::fmt::Display;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let mut address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
    let mut path = PathBuf::new();
    let mut headers = "".to_string();
    let mut reload = true;
    let mut actions = Vec::new();

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_ref() {
            "--address" => {
                address = args
                    .next()
                    .expect("Pass an address with a port after the '--address' flag")
                    .to_socket_addrs()
                    .expect("Please give address and port in form of '127.0.0.1:8080'")
                    .next()
                    .unwrap()
            }
            "-b" | "--build" => {
                let cmd = args.next().expect("--build expexted argument");
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
            }
            "--reload" | "--refresh" => reload = true,
            "--noreload" | "--norefresh" => reload = false,
            "--path" => {
                path = args
                    .next()
                    .expect("Pass a path after the '--path' flag")
                    .into()
            }
            "--header" => {
                let mut new_header = args
                    .next()
                    .expect("Pass a header after the '--header' flag");
                if !new_header.contains(':') {
                    if new_header.contains('=') {
                        new_header = new_header.replacen('=', ":", 1);
                    } else {
                        panic!("Pass a ':' or '=' in the '--header' flag");
                    }
                }
                if new_header.contains('\r') || new_header.contains('\n') || !new_header.is_ascii()
                {
                    panic!("Only ASCII without line breaks is allowed in the '--header' flag");
                }
                headers.push_str("\r\n");
                headers.push_str(&new_header);
            }
            "--help" => {
                println!(
                    r#"Run 'devserver' in a folder to host that folder.

--reload                   Automatically refresh pages when a file in the hosted folder changes. Enabled by default.
--noreload                 Do not automatically refresh pages when a file in the hosted folder changes.
--address [address]:[port] Specify an address to use. The default is 'localhost:8080'.
--path [path]              Specify the path of the folder to be hosted.
--header                   Specify an additional header to send in responses. Use multiple --header flags for multiple headers.
--help                     Display the helpful information you're reading right now.

Examples:

devserver --address 127.0.0.1:8080 --path "some_directory/subdirectory" --header Access-Control-Allow-Origin='*'

                "#
                );
                return;
            }
            _ => {
                println!(
                    "Unrecognized flag: `{:?}`.\nSee available options with `devserver --help`",
                    arg
                );
                return;
            }
        }
    }
    let hosted_path = env::current_dir().unwrap().join(path);

    if !hosted_path.exists() {
        println!("Path [{}] does not exist!", hosted_path.display());
        return;
    }

    println!(
        "\nServing [{}] at [ https://{} ] or [ http://{} ]",
        hosted_path.display(),
        address.ip(),
        address.port()
    );

    if reload {
        println!("Automatic reloading is enabled!");
    }

    println!("Stop with Ctrl+C");

    devserver::run(
        address.ip(),
        address.port(),
        &hosted_path,
        reload,
        &headers,
        actions,
    );
}
