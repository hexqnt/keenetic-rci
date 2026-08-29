# keenetic-rci

[🇺🇸 English](./README.md) · [🇷🇺 Русский](./README.ru.md)

[![CI](https://github.com/hexqnt/keenetic-rci/actions/workflows/ci.yml/badge.svg)](https://github.com/hexqnt/keenetic-rci/actions/workflows/ci.yml) [![crates.io](https://img.shields.io/crates/v/keenetic-rci.svg)](https://crates.io/crates/keenetic-rci) [![docs.rs](https://img.shields.io/docsrs/keenetic-rci)](https://docs.rs/keenetic-rci)

Неофициальный асинхронный Rust-клиент для локального HTTP API RCI маршрутизаторов Keenetic/Netcraze.

- Типизированные запросы и модели ответов для часто используемых эндпоинтов RCI
- Challenge-response-аутентификация в локальной сети и автоматическое управление сессиями
- Диагностика с помощью Ping, Ping IPv6, Traceroute и iPerf3
- Низкоуровневые запросы RCI и неинтерактивные команды CLI для остальных операций

Модели ответов рассчитаны на KeeneticOS 5.0 и новее. Совместимость проверена с Hero 4G+ (KN-2311) и Hopper (KN-3810 и KN-3811). На других устройствах и версиях прошивки структура ответов может отличаться; неизвестные поля JSON игнорируются.

Проект не связан с Keenetic и не поддерживается компанией.

## Начало работы

Добавьте крейт в проект:

```bash
cargo add keenetic-rci
```

Создайте клиент и выполните типизированный запрос:

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

Учётные данные необязательны, если маршрутизатор разрешает доступ без аутентификации. Создание клиента не устанавливает соединение с маршрутизатором; аутентификация выполняется, когда она требуется для запроса.

Типизированные запросы охватывают сведения об устройстве и прошивке, состояние системы, интерфейсы и статистику, подключённых клиентов, маршрутизацию и сетевые службы, Mesh-системы, USB-накопители и состояние LTE. Полный список приведён в документации [модуля `request`](https://docs.rs/keenetic-rci/latest/keenetic_rci/request/).

## Сетевая диагностика

Операции Network Connection Test используют те же средства диагностики, что и веб-интерфейс маршрутизатора. Полный тест можно выполнить одним вызовом:

```rust
use keenetic_rci::{KeeneticClient, Ping};

async fn ping(client: &KeeneticClient) -> Result<(), Box<dyn std::error::Error>> {
    let request = Ping::new("example.com")?.count(3)?;
    let output = client.run_network_test(request).await?;
    println!("{output}");
    Ok(())
}
```

Используйте `start_network_test`, если нужно получать вывод по частям или явно отменить операцию. Удаление активной сессии не отменяет команду на маршрутизаторе. Для iPerf3 требуется соответствующий компонент KeeneticOS.

## Низкоуровневый доступ к RCI и CLI

Низкоуровневые запросы используют проверяемые пути эндпоинтов, а также общие с типизированными запросами механизмы аутентификации и обработки ошибок:

```rust
use keenetic_rci::{KeeneticClient, RciPath};
use serde_json::Value;

async fn get_interfaces(client: &KeeneticClient) -> Result<Value, Box<dyn std::error::Error>> {
    let path: RciPath = "show/interface".parse()?;
    Ok(client.get_raw(&path).await?)
}
```

`execute_cli` выполняет одну проверенную неинтерактивную команду через `/rci/parse`:

```rust
use keenetic_rci::{CliCommand, KeeneticClient};

async fn run_command(client: &KeeneticClient) -> Result<(), Box<dyn std::error::Error>> {
    let command = CliCommand::new("show version")?;
    let reply = client.execute_cli(&command).await?;
    println!("{}", reply.raw());
    Ok(())
}
```

Низкоуровневые запросы и команды CLI могут изменять текущую конфигурацию или состояние маршрутизатора. Библиотека не сохраняет и не проверяет изменения конфигурации, а также не повторяет операции после транспортных ошибок.

Поддерживается только локальная аутентификация RCI. Удалённая аутентификация через KeenDNS и HTTP Digest через прокси на порте 79 не поддерживаются.

## Интеграционные тесты

По умолчанию тесты на реальном оборудовании отключены. Скопируйте `live.example.toml` в `live.toml`, добавьте один или несколько маршрутизаторов и выполните:

```bash
cargo test --test live live_routers_support_typed_api -- --ignored --nocapture
cargo test --test live live_routers_support_network_connection_tests -- --ignored --nocapture
```

Первый тест выполняет только чтение. Второй запускает короткие диагностические операции через loopback-интерфейс и проверяет их отмену. Чтобы использовать другой файл конфигурации, задайте переменную `KEENETIC_RCI_LIVE_CONFIG`.
