use std::hint::black_box;

use axum::{
    Router,
    body::Body,
    http::{Response, header},
    routing::get,
};
use bytes::Bytes;
use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use keenetic_rci::{
    Associations, ClientIndex, Identification, InterfaceStat, Interfaces, InternetStatus, IpArp,
    IpDhcpBindings, IpHotspotHosts, IpNameServers, IpRoutes, Ipv6Routes, KeeneticClient,
    MediaInventory, MwsMembers, MwsStatus, NtpStatus, PingCheckProfiles, RciPath,
    ShowInterfaceReply, ShowLteInterfaceReply, System, SystemModeStatus, UsbDevices, Version,
    request::ShowIpHotspotHosts,
};
use serde::de::DeserializeOwned;
use tokio::task::JoinHandle;

const ASSOCIATIONS: &[u8] = include_bytes!("../tests/fixtures/show_associations.json");
const HOTSPOT: &[u8] = include_bytes!("../tests/fixtures/show_ip_hotspot.json");
const ARP: &[u8] = include_bytes!("../tests/fixtures/show_ip_arp.json");
const DHCP: &[u8] = include_bytes!("../tests/fixtures/show_ip_dhcp_bindings.json");
const LTE_INTERFACE: &[u8] = include_bytes!("../tests/fixtures/show_lte_interface.json");

const SNAPSHOT_FIXTURES: &[&[u8]] = &[
    include_bytes!("../tests/fixtures/show_version.json"),
    include_bytes!("../tests/fixtures/show_system.json"),
    include_bytes!("../tests/fixtures/show_internet_status.json"),
    include_bytes!("../tests/fixtures/show_interfaces.json"),
    ASSOCIATIONS,
    HOTSPOT,
    include_bytes!("../tests/fixtures/show_identification.json"),
    include_bytes!("../tests/fixtures/show_system_mode.json"),
    ARP,
    DHCP,
    include_bytes!("../tests/fixtures/show_ip_route.json"),
    include_bytes!("../tests/fixtures/show_ipv6_route.json"),
    include_bytes!("../tests/fixtures/show_ping_check.json"),
    include_bytes!("../tests/fixtures/show_ip_name_server.json"),
    include_bytes!("../tests/fixtures/show_ntp_status.json"),
    include_bytes!("../tests/fixtures/show_mws_status.json"),
    include_bytes!("../tests/fixtures/show_mws_member.json"),
    include_bytes!("../tests/fixtures/show_usb.json"),
    include_bytes!("../tests/fixtures/show_media.json"),
    include_bytes!("../tests/fixtures/show_interface_stat.json"),
    include_bytes!("../tests/fixtures/show_interface.json"),
    include_bytes!("../tests/fixtures/show_lte_interface.json"),
];

fn decode<T>(bytes: &[u8]) -> T
where
    T: DeserializeOwned,
{
    serde_json::from_slice(black_box(bytes)).unwrap()
}

fn decode_typed_snapshot() {
    macro_rules! decode_fixture {
        ($response:ty, $fixture:literal) => {
            black_box(decode::<$response>(include_bytes!(concat!(
                "../tests/fixtures/",
                $fixture
            ))));
        };
    }

    decode_fixture!(Version, "show_version.json");
    decode_fixture!(System, "show_system.json");
    decode_fixture!(InternetStatus, "show_internet_status.json");
    decode_fixture!(Interfaces, "show_interfaces.json");
    decode_fixture!(Associations, "show_associations.json");
    decode_fixture!(IpHotspotHosts, "show_ip_hotspot.json");
    decode_fixture!(Identification, "show_identification.json");
    decode_fixture!(SystemModeStatus, "show_system_mode.json");
    decode_fixture!(IpArp, "show_ip_arp.json");
    decode_fixture!(IpDhcpBindings, "show_ip_dhcp_bindings.json");
    decode_fixture!(IpRoutes, "show_ip_route.json");
    decode_fixture!(Ipv6Routes, "show_ipv6_route.json");
    decode_fixture!(PingCheckProfiles, "show_ping_check.json");
    decode_fixture!(IpNameServers, "show_ip_name_server.json");
    decode_fixture!(NtpStatus, "show_ntp_status.json");
    decode_fixture!(MwsStatus, "show_mws_status.json");
    decode_fixture!(MwsMembers, "show_mws_member.json");
    decode_fixture!(UsbDevices, "show_usb.json");
    decode_fixture!(MediaInventory, "show_media.json");
    decode_fixture!(InterfaceStat, "show_interface_stat.json");
    decode_fixture!(ShowInterfaceReply, "show_interface.json");
    decode_fixture!(ShowLteInterfaceReply, "show_lte_interface.json");
}

fn benchmark_models(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("models");
    let snapshot_bytes = SNAPSHOT_FIXTURES
        .iter()
        .map(|fixture| fixture.len() as u64)
        .sum();
    group.throughput(Throughput::Bytes(snapshot_bytes));
    group.bench_function("typed_snapshot", |bencher| {
        bencher.iter(decode_typed_snapshot);
    });
    group.throughput(Throughput::Bytes(LTE_INTERFACE.len() as u64));
    group.bench_function("lte_interface", |bencher| {
        bencher.iter(|| black_box(decode::<ShowLteInterfaceReply>(LTE_INTERFACE)));
    });
    group.finish();
}

fn benchmark_client_index(criterion: &mut Criterion) {
    let associations: Associations = decode(ASSOCIATIONS);
    let hotspot: IpHotspotHosts = decode(HOTSPOT);
    let arp: IpArp = decode(ARP);
    let dhcp: IpDhcpBindings = decode(DHCP);

    let mut group = criterion.benchmark_group("client_index");
    group.throughput(Throughput::Elements(
        (associations.len() + hotspot.len() + arp.len() + dhcp.len()) as u64,
    ));
    group.bench_function("build", |bencher| {
        bencher.iter(|| {
            black_box(ClientIndex::new(
                black_box(&associations),
                black_box(&hotspot),
                black_box(&arp),
                black_box(&dhcp),
            ))
        });
    });
    group.finish();
}

fn benchmark_client(criterion: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (origin, server) = runtime.block_on(spawn_fixture_server());
    let client = KeeneticClient::builder()
        .base_url(origin.clone())
        .build()
        .unwrap();
    let cookie_client = KeeneticClient::builder().base_url(origin).build().unwrap();
    black_box(
        runtime
            .block_on(client.execute(ShowIpHotspotHosts))
            .unwrap(),
    );
    let seed_path: RciPath = "seed-cookie".parse().unwrap();
    black_box(runtime.block_on(cookie_client.get_raw(&seed_path)).unwrap());

    {
        let mut group = criterion.benchmark_group("client");
        group.throughput(Throughput::Bytes(HOTSPOT.len() as u64));
        group.bench_function("execute_hotspot", |bencher| {
            bencher
                .to_async(&runtime)
                .iter(|| async { black_box(client.execute(ShowIpHotspotHosts).await.unwrap()) });
        });
        group.bench_function("execute_hotspot_with_cookie", |bencher| {
            bencher.to_async(&runtime).iter(|| async {
                black_box(cookie_client.execute(ShowIpHotspotHosts).await.unwrap())
            });
        });
        group.finish();
    }
    server.abort();
}

async fn hotspot_response() -> Response<Body> {
    Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(Bytes::from_static(HOTSPOT)))
        .unwrap()
}

async fn seed_cookie_response() -> Response<Body> {
    Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::SET_COOKIE, "session=benchmark; Path=/")
        .body(Body::from("{}"))
        .unwrap()
}

async fn spawn_fixture_server() -> (String, JoinHandle<()>) {
    let app = Router::new()
        .route("/rci/show/ip/hotspot", get(hotspot_response))
        .route("/rci/seed-cookie", get(seed_cookie_response));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{address}"), task)
}

criterion_group!(
    benches,
    benchmark_models,
    benchmark_client_index,
    benchmark_client
);
criterion_main!(benches);
