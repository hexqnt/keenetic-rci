//! Optional checks against explicitly configured real routers.

use std::{
    collections::BTreeSet,
    env,
    error::Error as StdError,
    fmt::Write as _,
    fs, io, mem,
    path::{Path, PathBuf},
};

use keenetic_rci::{
    ClientIndex, ConfigError, Error, Interface, InterfaceId, Interfaces, KeeneticClient,
    request::{
        NetworkTestRequest, Ping, PingIpv6, RciRequest, ShowAssociations, ShowIdentification,
        ShowInterface, ShowInterfaceStat, ShowInterfaces, ShowInternetStatus, ShowIpArp,
        ShowIpDhcpBindings, ShowIpHotspotHosts, ShowIpNameServers, ShowIpRoute, ShowIpv6Route,
        ShowLteInterface, ShowMedia, ShowMwsMembers, ShowMwsStatus, ShowNtpStatus, ShowPingCheck,
        ShowSystem, ShowSystemMode, ShowUsb, ShowVersion, Traceroute,
    },
};
use serde::Deserialize;
use thiserror::Error as ThisError;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

const CONFIG_ENV: &str = "KEENETIC_RCI_LIVE_CONFIG";
const DEFAULT_CONFIG_PATH: &str = "live.toml";

#[derive(Debug, ThisError)]
enum LoadConfigError {
    #[error("failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("invalid {path}: {source}")]
    Invalid {
        path: PathBuf,
        #[source]
        source: ParseConfigError,
    },
}

#[derive(Debug, ThisError)]
enum ParseConfigError {
    #[error("invalid TOML configuration: {0}")]
    Toml(Box<str>),
    #[error("`routers` must contain at least one router")]
    EmptyRouters,
    #[error("router at index {index} has an empty name")]
    EmptyRouterName { index: usize },
    #[error("duplicate router name `{0}`")]
    DuplicateRouterName(Box<str>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LiveConfig {
    routers: Vec<RouterConfig>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RouterConfig {
    name: String,
    url: String,
    credentials: Option<CredentialsConfig>,
}

impl RouterConfig {
    fn into_client(self) -> Result<(String, KeeneticClient), ConfigError> {
        let Self {
            name,
            url,
            credentials,
        } = self;
        let mut builder = KeeneticClient::builder().base_url(url);
        if let Some(mut credentials) = credentials {
            builder = builder.credentials(credentials.login, credentials.password.take());
        }
        builder.build().map(|client| (name, client))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialsConfig {
    login: String,
    password: Password,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(transparent)]
struct Password(String);

impl Password {
    fn take(&mut self) -> String {
        mem::take(&mut self.0)
    }
}

#[derive(Default)]
struct Report {
    passed: usize,
    skipped: usize,
    failures: Vec<Failure>,
}

impl Report {
    fn pass(&mut self, router: &str, check: &str) {
        self.passed += 1;
        println!("PASS [{router}] {check}");
    }

    fn skip(&mut self, router: &str, check: &str, reason: &str) {
        self.skipped += 1;
        println!("SKIP [{router}] {check}: {reason}");
    }

    fn fail(&mut self, router: &str, check: &str, reason: impl Into<String>) {
        let reason = reason.into();
        println!("FAIL [{router}] {check}: {reason}");
        self.failures.push(Failure {
            router: router.to_owned(),
            check: check.to_owned(),
            reason,
        });
    }

    fn finish(self) {
        println!(
            "Live test summary: {} passed, {} skipped, {} failed",
            self.passed,
            self.skipped,
            self.failures.len()
        );
        if self.failures.is_empty() {
            return;
        }

        let mut summary = String::from("live router checks failed:");
        for failure in self.failures {
            let _ = write!(
                summary,
                "\n- [{}] {}: {}",
                failure.router, failure.check, failure.reason
            );
        }
        panic!("{summary}");
    }
}

struct Failure {
    router: String,
    check: String,
    reason: String,
}

struct RouterRun<'a> {
    name: &'a str,
    client: &'a KeeneticClient,
    report: &'a mut Report,
}

fn config_path() -> PathBuf {
    env::var_os(CONFIG_ENV).map_or_else(|| PathBuf::from(DEFAULT_CONFIG_PATH), PathBuf::from)
}

fn load_config(path: &Path) -> Result<LiveConfig, LoadConfigError> {
    let source = fs::read_to_string(path)
        .map(Zeroizing::new)
        .map_err(|source| LoadConfigError::Read {
            path: path.to_owned(),
            source,
        })?;
    parse_config(source.as_str()).map_err(|source| LoadConfigError::Invalid {
        path: path.to_owned(),
        source,
    })
}

fn parse_config(source: &str) -> Result<LiveConfig, ParseConfigError> {
    let config: LiveConfig = toml::from_str(source).map_err(|mut error| {
        error.set_input(None);
        ParseConfigError::Toml(error.to_string().trim().into())
    })?;
    if config.routers.is_empty() {
        return Err(ParseConfigError::EmptyRouters);
    }

    let mut names = BTreeSet::new();
    for (index, router) in config.routers.iter().enumerate() {
        if router.name.trim().is_empty() {
            return Err(ParseConfigError::EmptyRouterName { index });
        }
        if !names.insert(router.name.as_str()) {
            return Err(ParseConfigError::DuplicateRouterName(
                router.name.clone().into_boxed_str(),
            ));
        }
    }
    Ok(config)
}

macro_rules! required_routes {
    ($run:expr; $($check:literal => $request:expr),+ $(,)?) => {
        $($run.required_only($check, $request).await;)+
    };
}

impl<'a> RouterRun<'a> {
    const fn new(name: &'a str, client: &'a KeeneticClient, report: &'a mut Report) -> Self {
        Self {
            name,
            client,
            report,
        }
    }

    async fn required<R>(&mut self, check: &str, request: R) -> Option<R::Response>
    where
        R: RciRequest,
    {
        match self.client.execute(request).await {
            Ok(response) => {
                self.report.pass(self.name, check);
                Some(response)
            }
            Err(error) => {
                self.report.fail(self.name, check, error_chain(&error));
                None
            }
        }
    }

    async fn required_only<R>(&mut self, check: &str, request: R)
    where
        R: RciRequest,
    {
        let _response = self.required(check, request).await;
    }

    async fn optional_media(&mut self) {
        const CHECK: &str = "GET /rci/show/media";
        match self.client.execute(ShowMedia).await {
            Ok(_) => self.report.pass(self.name, CHECK),
            Err(Error::Http(error)) if error.status() == reqwest::StatusCode::NOT_FOUND => {
                self.report
                    .skip(self.name, CHECK, "the media component is not installed");
            }
            Err(error) => self.report.fail(self.name, CHECK, error_chain(&error)),
        }
    }

    async fn snapshot_checks(&mut self) -> Option<Interfaces> {
        required_routes!(self;
            "GET /rci/show/version" => ShowVersion,
            "GET /rci/show/system" => ShowSystem,
            "GET /rci/show/internet/status" => ShowInternetStatus,
        );

        let interfaces = self
            .required("GET /rci/show/interface", ShowInterfaces)
            .await;

        let associations = self
            .required("GET /rci/show/associations", ShowAssociations)
            .await;
        let hotspot = self
            .required("GET /rci/show/ip/hotspot", ShowIpHotspotHosts)
            .await;
        let arp = self.required("GET /rci/show/ip/arp", ShowIpArp).await;
        let dhcp = self
            .required("GET /rci/show/ip/dhcp/bindings", ShowIpDhcpBindings)
            .await;

        if let (Some(associations), Some(hotspot), Some(arp), Some(dhcp)) =
            (&associations, &hotspot, &arp, &dhcp)
        {
            let clients = ClientIndex::new(associations, hotspot, arp, dhcp);
            let _client_count = clients.iter().count();
            self.report
                .pass(self.name, "ClientIndex joins live snapshots");
        } else {
            self.report.fail(
                self.name,
                "ClientIndex joins live snapshots",
                "one or more prerequisite routes failed",
            );
        }

        interfaces
    }

    async fn endpoint_checks(&mut self) {
        required_routes!(self;
            "GET /rci/show/identification" => ShowIdentification,
            "GET /rci/show/system/mode" => ShowSystemMode,
            "GET /rci/show/ip/route" => ShowIpRoute,
            "GET /rci/show/ipv6/route" => ShowIpv6Route,
            "GET /rci/show/ping-check" => ShowPingCheck,
            "GET /rci/show/ip/name-server" => ShowIpNameServers,
            "GET /rci/show/ntp/status" => ShowNtpStatus,
            "GET /rci/show/mws/status" => ShowMwsStatus,
            "GET /rci/show/mws/member" => ShowMwsMembers,
            "GET /rci/show/usb" => ShowUsb,
        );
        self.optional_media().await;
    }

    fn check_interface_identity(
        &mut self,
        check: &str,
        requested: &InterfaceId,
        interface: &Interface,
    ) {
        if interface.id == *requested || interface.interface_name == *requested {
            self.report.pass(self.name, check);
        } else {
            self.report.fail(
                self.name,
                check,
                format!(
                    "response id is `{}` and interface-name is `{}`",
                    interface.id, interface.interface_name
                ),
            );
        }
    }

    async fn interface_checks(&mut self, interfaces: Option<Interfaces>) {
        let Some(interfaces) = interfaces else {
            self.report.fail(
                self.name,
                "interface-specific checks",
                "GET /rci/show/interface failed",
            );
            return;
        };
        if interfaces.is_empty() {
            self.report.fail(
                self.name,
                "interface-specific checks",
                "the router returned no interfaces",
            );
            return;
        }

        let interface_names: Vec<_> = interfaces.keys().cloned().collect();
        let lte_names: Vec<_> = interfaces
            .keys()
            .filter(|name| {
                interfaces
                    .get(name.as_str())
                    .is_some_and(Interface::is_mobile_broadband)
            })
            .cloned()
            .collect();

        for name in interface_names {
            let detail_check = format!("POST /rci/ show interface `{name}`");
            if let Some(reply) = self
                .required(&detail_check, ShowInterface::from(name.clone()))
                .await
            {
                let identity_check = format!("interface identity `{name}`");
                self.check_interface_identity(&identity_check, &name, reply.interface());
            }

            let stat_check = format!("GET /rci/show/interface/stat?name={name}");
            self.required_only(&stat_check, ShowInterfaceStat::from(name))
                .await;
        }

        self.lte_checks(lte_names).await;
    }

    async fn network_test<R>(&mut self, check: &str, request: R)
    where
        R: NetworkTestRequest,
    {
        match self.client.run_network_test(request).await {
            Ok(output) if output.is_empty() => {
                self.report
                    .fail(self.name, check, "the router returned no output");
            }
            Ok(_) => self.report.pass(self.name, check),
            Err(error) => self.report.fail(self.name, check, error_chain(&error)),
        }
    }

    async fn network_test_checks(&mut self) {
        const CANCELLATION_CHECK: &str = "DELETE /rci/tools/ping cancellation";

        self.network_test(
            "POST/GET /rci/tools/ping loopback",
            Ping::new("127.0.0.1").unwrap().count(1).unwrap(),
        )
        .await;
        self.network_test(
            "POST/GET /rci/tools/ping6 loopback",
            PingIpv6::new("::1").unwrap().count(1).unwrap(),
        )
        .await;
        self.network_test(
            "POST/GET /rci/tools/traceroute loopback",
            Traceroute::new("127.0.0.1")
                .unwrap()
                .count(1)
                .unwrap()
                .wait_time(1)
                .unwrap()
                .max_ttl(1)
                .unwrap(),
        )
        .await;

        let request = Ping::new("127.0.0.1").unwrap().count(100).unwrap();
        match self.client.start_network_test(request).await {
            Ok(session) if session.is_finished() => {
                self.report.fail(
                    self.name,
                    CANCELLATION_CHECK,
                    "the command finished before cancellation",
                );
            }
            Ok(session) => match session.cancel().await {
                Ok(_) => self.report.pass(self.name, CANCELLATION_CHECK),
                Err(error) => {
                    self.report
                        .fail(self.name, CANCELLATION_CHECK, error_chain(&error));
                }
            },
            Err(error) => {
                self.report
                    .fail(self.name, CANCELLATION_CHECK, error_chain(&error));
            }
        }
    }

    async fn lte_checks(&mut self, lte_names: Vec<InterfaceId>) {
        if lte_names.is_empty() {
            self.report.skip(
                self.name,
                "typed LTE interface checks",
                "no LTE interfaces were discovered",
            );
        }
        for name in lte_names {
            let check = format!("POST /rci/ show typed LTE interface `{name}`");
            if let Some(reply) = self
                .required(&check, ShowLteInterface::from(name.clone()))
                .await
            {
                let identity_check = format!("LTE interface identity `{name}`");
                self.check_interface_identity(&identity_check, &name, reply.interface());
            }
        }
    }

    async fn run(mut self) {
        let interfaces = self.snapshot_checks().await;
        self.endpoint_checks().await;
        self.interface_checks(interfaces).await;
    }
}

fn error_chain(error: &Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(error) = source {
        let detail = error.to_string();
        if !message.ends_with(&detail) {
            let _ = write!(message, ": {detail}");
        }
        source = error.source();
    }
    message
}

#[tokio::test]
#[ignore = "requires explicitly configured local Keenetic routers"]
async fn live_routers_support_typed_api() {
    let path = config_path();
    let config = load_config(&path).unwrap_or_else(|error| {
        panic!("{error}; copy live.example.toml to {DEFAULT_CONFIG_PATH} or set {CONFIG_ENV}")
    });
    let mut report = Report::default();

    for router in config.routers {
        let router_name = router.name.clone();
        match router.into_client() {
            Ok((name, client)) => RouterRun::new(&name, &client, &mut report).run().await,
            Err(error) => report.fail(&router_name, "client configuration", error.to_string()),
        }
    }

    report.finish();
}

#[tokio::test]
#[ignore = "runs active diagnostics on explicitly configured local Keenetic routers"]
async fn live_routers_support_network_connection_tests() {
    let path = config_path();
    let config = load_config(&path).unwrap_or_else(|error| {
        panic!("{error}; copy live.example.toml to {DEFAULT_CONFIG_PATH} or set {CONFIG_ENV}")
    });
    let mut report = Report::default();

    for router in config.routers {
        let router_name = router.name.clone();
        match router.into_client() {
            Ok((name, client)) => {
                RouterRun::new(&name, &client, &mut report)
                    .network_test_checks()
                    .await;
            }
            Err(error) => report.fail(&router_name, "client configuration", error.to_string()),
        }
    }

    report.finish();
}

#[cfg(test)]
mod tests {
    use super::{ParseConfigError, parse_config};

    #[test]
    fn parses_one_or_many_routers_with_optional_credentials() {
        let config = parse_config(
            r#"
                [[routers]]
                name = "anonymous"
                url = "http://192.0.2.1"

                [[routers]]
                name = "authenticated"
                url = "https://router.example"
                credentials = { login = "monitor", password = "secret" }
            "#,
        )
        .unwrap();

        assert_eq!(config.routers.len(), 2);
        assert!(config.routers[0].credentials.is_none());
        let credentials = config.routers[1].credentials.as_ref().unwrap();
        assert_eq!(credentials.login, "monitor");
        assert_eq!(credentials.password.0, "secret");
    }

    #[test]
    fn rejects_empty_router_list() {
        assert!(matches!(
            parse_config("routers = []"),
            Err(ParseConfigError::EmptyRouters)
        ));
    }

    #[test]
    fn rejects_empty_router_names() {
        let result = parse_config(
            r#"
                [[routers]]
                name = "  "
                url = "http://192.0.2.1"
            "#,
        );

        assert!(matches!(
            result,
            Err(ParseConfigError::EmptyRouterName { index: 0 })
        ));
    }

    #[test]
    fn rejects_duplicate_router_names() {
        let result = parse_config(
            r#"
                [[routers]]
                name = "duplicate"
                url = "http://192.0.2.1"

                [[routers]]
                name = "duplicate"
                url = "http://192.0.2.2"
            "#,
        );

        assert!(matches!(
            result,
            Err(ParseConfigError::DuplicateRouterName(name)) if name.as_ref() == "duplicate"
        ));
    }

    #[test]
    fn rejects_unknown_top_level_router_and_credentials_fields() {
        for source in [
            r"
                routers = []
                unexpected = true
            ",
            r#"
                [[routers]]
                name = "router"
                url = "http://192.0.2.1"
                unexpected = true
            "#,
            r#"
                [[routers]]
                name = "router"
                url = "http://192.0.2.1"
                credentials = { login = "admin", password = "secret", unexpected = true }
            "#,
        ] {
            assert!(parse_config(source).is_err());
        }
    }

    #[test]
    fn parse_errors_do_not_echo_passwords() {
        let password = "unique-live-password-that-must-stay-secret";
        let source = format!(
            r#"
                [[routers]]
                name = "router"
                url = "http://192.0.2.1"
                credentials = {{ password = "{password}" }}
            "#
        );

        let error = parse_config(&source).err().unwrap().to_string();
        assert!(!error.contains(password));
    }
}
