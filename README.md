# keenetic-rci

[🇺🇸 English](./README.md) · [🇷🇺 Русский](./README.ru.md)

[![CI](https://github.com/hexqnt/keenetic-rci/actions/workflows/ci.yml/badge.svg)](https://github.com/hexqnt/keenetic-rci/actions/workflows/ci.yml) [![crates.io](https://img.shields.io/crates/v/keenetic-rci.svg)](https://crates.io/crates/keenetic-rci) [![docs.rs](https://img.shields.io/docsrs/keenetic-rci)](https://docs.rs/keenetic-rci)

An unofficial asynchronous Rust client for the local Keenetic/Netcraze RCI HTTP API.

- Typed requests and response models for commonly used RCI endpoints
- LAN challenge-response authentication and automatic session management
- Ping, Ping IPv6, Traceroute, and iPerf3 diagnostics
- Raw RCI requests and non-interactive CLI commands for unsupported operations

The response models target KeeneticOS 5.0 and newer. Compatibility has been tested with Hero 4G+ (KN-2311) and Hopper (KN-3810 and KN-3811). Other devices or firmware versions may return different response shapes; unknown JSON fields are ignored.

This project is not affiliated with or endorsed by Keenetic.

## Getting started

Add the crate to your project:

```bash
cargo add keenetic-rci
```

Create a client and execute a typed request:

```rust
use keenetic_rci::{KeeneticClient, request::ShowVersion};

async fn show_version(password: String) -> Result<(), Box<dyn std::error::Error>> {
    let client = KeeneticClient::builder()
        .base_url("http://192.168.1.1")
        .credentials("monitor", password)
        .build()?;

    let version = client.execute(ShowVersion).await?;
    println!("{} ({})", version.model, version.hw_id);
    Ok(())
}
```

Credentials are optional for routers that allow unauthenticated access. Building a client does not connect to the router; authentication is performed when a request requires it.

Typed requests cover device and firmware information, system state, interfaces and statistics, connected clients, routing and network services, mesh systems, USB storage, and LTE status. See the [`request` module](https://docs.rs/keenetic-rci/latest/keenetic_rci/request/) for the complete list.

## Network diagnostics

Network Connection Test operations use the same diagnostics as the router's web interface. A complete test can be run in one call:

```rust
use keenetic_rci::{KeeneticClient, Ping};

async fn ping(client: &KeeneticClient) -> Result<(), Box<dyn std::error::Error>> {
    let request = Ping::new("example.com")?.count(3)?;
    let output = client.run_network_test(request).await?;
    println!("{output}");
    Ok(())
}
```

Use `start_network_test` when you need incremental output or explicit cancellation. Dropping an active session does not cancel the command on the router. iPerf3 requires the corresponding KeeneticOS component.

## Raw RCI and CLI access

Raw requests use validated endpoint paths and the same authentication and error handling as typed requests:

```rust
use keenetic_rci::{KeeneticClient, RciPath};
use serde_json::Value;

async fn get_interfaces(client: &KeeneticClient) -> Result<Value, Box<dyn std::error::Error>> {
    let path: RciPath = "show/interface".parse()?;
    Ok(client.get_raw(&path).await?)
}
```

`execute_cli` runs one validated, non-interactive command through `/rci/parse`:

```rust
use keenetic_rci::{CliCommand, KeeneticClient};

async fn run_command(client: &KeeneticClient) -> Result<(), Box<dyn std::error::Error>> {
    let command = CliCommand::new("show version")?;
    let reply = client.execute_cli(&command).await?;
    println!("{}", reply.raw());
    Ok(())
}
```

Raw and CLI operations may change the router's running configuration or device state. The library does not save or verify configuration changes, and it does not retry operations after transport failures.

Only local/LAN RCI authentication is supported. KeenDNS remote authentication and HTTP Digest through the port 79 proxy are not supported.

## Live tests

Live tests are ignored by default. Copy `live.example.toml` to `live.toml`, add one or more routers, and run:

```bash
cargo test --test live live_routers_support_typed_api -- --ignored --nocapture
cargo test --test live live_routers_support_network_connection_tests -- --ignored --nocapture
```

The first test is read-only. The second runs short loopback diagnostics and verifies cancellation. Set `KEENETIC_RCI_LIVE_CONFIG` to use a different configuration file.
