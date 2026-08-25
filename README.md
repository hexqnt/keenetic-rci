# keenetic-rci

[![CI](https://github.com/hexqnt/keenetic-rci/actions/workflows/ci.yml/badge.svg)](https://github.com/hexqnt/keenetic-rci/actions/workflows/ci.yml)

An unofficial asynchronous Rust client for the local Keenetic/Netcraze RCI HTTP API. It supports typed requests and raw RCI operations, LAN challenge-response authentication, session cookies, one transparent re-authentication attempt after `401`, and RCI errors returned inside successful HTTP responses.

The typed response models target KeeneticOS 5.0 and newer. Compatibility has been tested with:

- [Hero 4G+ (KN-2311)](https://storage.googleapis.com/docs.help.keenetic.com/cli/5.0/en/cli_manual_kn-2311.pdf)
- [Hopper (KN-3810)](https://storage.googleapis.com/docs.help.keenetic.com/cli/5.0/en/cli_manual_kn-3810.pdf)
- [Hopper (KN-3811)](https://storage.googleapis.com/docs.help.keenetic.com/cli/5.0/en/cli_manual_kn-3811.pdf)

Other models and releases may return different response shapes. Unknown JSON fields are ignored.

This project is not affiliated with or endorsed by Keenetic.

## Usage

Building a client does not contact the router. Credentials are optional and authentication is performed only after a request receives `401`.

```rust
use keenetic_rci::{KeeneticClient, request::ShowVersion};

async fn example(password: String) -> Result<(), Box<dyn std::error::Error>> {
    let client = KeeneticClient::builder()
        .base_url("http://192.168.1.1")
        .credentials("monitor", password)
        .build()?;

    let version = client.execute(ShowVersion).await?;
    println!("{} ({})", version.model, version.hw_id);
    Ok(())
}
```

Typed requests cover version, system, operating mode, identification, Internet status, interfaces and statistics, connected clients, ARP and DHCP, IPv4 and IPv6 routes, DNS, NTP, ping checks, Network Connection Test tools, mesh systems, USB devices, storage media, and LTE status. See the [`request` module](https://docs.rs/keenetic-rci/latest/keenetic_rci/request/) for the complete list.

LTE modems are interfaces in KeeneticOS. Fetch them with `ShowInterfaces`, discover LTE entries through `Interfaces::lte`, then query one with `ShowLteInterface`.

`ClientIndex` combines already fetched Wi-Fi association, hotspot, ARP, and DHCP snapshots by MAC address without cloning their records.

## Network Connection Test

Ping, Ping IPv6, Traceroute, and iPerf3 use the same continued operations as the router's Diagnostics page. A complete finite test can be collected in one call:

```rust
use keenetic_rci::{KeeneticClient, Ping};

async fn ping(client: &KeeneticClient) -> Result<(), Box<dyn std::error::Error>> {
    let request = Ping::new("example.com")?.count(3)?;
    let output = client.run_network_test(request).await?;
    println!("{output}");
    Ok(())
}
```

For incremental output or explicit cancellation, keep the session handle:

```rust
use keenetic_rci::{Iperf3, IperfLimit, KeeneticClient};

async fn iperf(client: &KeeneticClient) -> Result<(), Box<dyn std::error::Error>> {
    let request = Iperf3::new("iperf.example.com", IperfLimit::time(10)?)?;
    let mut session = client.start_network_test(request).await?;

    if let Some(first) = session.next_chunk().await? {
        println!("{first}");
    }
    let remaining = session.cancel().await?;
    println!("{remaining}");
    Ok(())
}
```

The request timeout applies to each start, poll, or cancellation HTTP request, not to the whole test. Dropping an active session does not cancel the router command. Literal source addresses are checked against the selected IP version; validated interface identifiers can be supplied through `NetworkTestSource::interface`. iPerf3 also requires the corresponding KeeneticOS component.

## Raw requests

Strict `RciPath` and `CiPath` types are used for raw endpoint paths. Raw requests use the same authentication and error handling as typed requests.

```rust
use keenetic_rci::{KeeneticClient, RciPath};
use serde_json::{Value, json};

async fn example(client: KeeneticClient) -> Result<(), Box<dyn std::error::Error>> {
    let path: RciPath = "show/interface".parse()?;
    let _interfaces: Value = client.get_raw(&path).await?;
    let _reply: Value = client.post_raw(&json!({"show": {"version": {}}})).await?;
    Ok(())
}
```

Raw `POST /rci/` commands can change the running configuration. The library does not save changes, verify their effect, or retry a transport failure.

Only local/LAN RCI authentication is supported. KeenDNS remote authentication and HTTP Digest through the port 79 proxy are not supported.

## Live tests

Read-only live tests are ignored by default. Copy `live.example.toml` to the`live.toml`, configure one or more routers, and run:

```bash
cargo test --test live live_routers_support_typed_api -- --ignored --nocapture
```

Set `KEENETIC_RCI_LIVE_CONFIG` to use a configuration file at another path.

The active Network Connection Test check runs short loopback Ping, Ping IPv6, and Traceroute commands and verifies explicit cancellation separately:

```bash
cargo test --test live live_routers_support_network_connection_tests -- --ignored --nocapture
```
