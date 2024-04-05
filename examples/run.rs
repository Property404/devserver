use std::net::Ipv4Addr;

/// Hosts a server at http://localhost:8080 serving whatever folder this is run from.
fn main() {
    devserver::run(
        Ipv4Addr::new(127, 0, 0, 1).into(),
        8080,
        "",
        false,
        "",
        Vec::new(),
    );
}
