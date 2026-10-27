# SaaS Substrate + Rust Solution

Полноценный каркас SaaS-системы на Rust с поддержкой смарт-контрактов Substrate (pallet-contracts + Ink!).

## Архитектура

```
┌─────────────────┐     ┌──────────────────────┐     ┌─────────────────────┐
│   Hyper 0.14     │────▶│  SaaS Adapter Layer  │────▶│  Substrate Node     │
│   API Service   │     │  (SOLID + Tokio)     │     │  + pallet-contracts │
└─────────────────┘     └──────────────────────┘     └─────────────────────┘
                               │
                               ▼
                        ┌─────────────────┐
                        │  Ink! Contract  │
                        │ simple_storage  │
                        └─────────────────┘
```

### Компоненты

| Компонент | Путь | Описание |
|-----------|------|----------|
| **Ink! Smart Contract** | `contracts/simple_storage` | Простой контракт хранения + mint/transfer (SaaS credits) |
| **Adapter Layer** | `adapter/` | SOLID-архитектура, многопоточность (Tokio + DashMap + Semaphore) |
| **API Service** | `api/` | Hyper 0.14 REST API для вызова контрактов |
| **Substrate Node** | (внешний) | Рекомендуется `substrate-contracts-node` |

### SOLID в адаптере

- **S**ingle Responsibility — отдельные `ports` и реализации
- **O**pen/Closed — новые блокчейны/хранилища через трейты
- **L**iskov — любые реализации `BlockchainPort` взаимозаменяемы
- **I**nterface Segregation — узкие трейты (`CachePort`, `EventPublisher`...)
- **D**ependency Inversion — `SaasService` зависит только от трейтов

### Многопоточность

- Tokio async runtime
- Tokio multi-threaded runtime (по умолчанию 4)
- `DashMap` для concurrent state
- `Semaphore` для ограничения параллельных вызовов к ноде
- `parking_lot::RwLock` для быстрого shared state

---

## Быстрый старт (симуляция)

Адаптер содержит **симулированный** `SubstrateAdapter`. Можно сразу поднять API и тестировать без реальной ноды.

```bash
cd api
cargo run
```

API будет на `http://0.0.0.0:8080`.

### Примеры запросов

```bash
# Health / статус ноды
curl http://localhost:8080/health

# Регистрация тенанта
curl -X POST http://localhost:8080/tenants \
  -H "Content-Type: application/json" \
  -d '{"name": "Acme Corp"}'

# Список тенантов
curl http://localhost:8080/tenants

# Деплой контракта для тенанта (замените UUID)
curl -X POST http://localhost:8080/tenants/<TENANT_UUID>/deploy \
  -H "Content-Type: application/json" \
  -d '{
    "wasm_path": "./simple_storage.wasm",
    "constructor": "new",
    "args": {"initial_value": 42}
  }'

# Вызов метода контракта
curl -X POST http://localhost:8080/tenants/<TENANT_UUID>/call \
  -H "Content-Type: application/json" \
  -d '{
    "contract_address": "<ADDRESS_FROM_DEPLOY>",
    "method": "get",
    "args": {}
  }'

curl -X POST http://localhost:8080/tenants/<TENANT_UUID>/call \
  -H "Content-Type: application/json" \
  -d '{
    "contract_address": "<ADDRESS>",
    "method": "set",
    "args": {"new_value": 100}
  }'
```

---

## Реальная Substrate-нода + Ink!

### 1. Установка инструментов

```bash
# Rust (nightly рекомендуется для contracts)
rustup install nightly
rustup component add rust-src --toolchain nightly
rustup target add wasm32-unknown-unknown --toolchain nightly

# cargo-contract
cargo install --force --locked cargo-contract

# substrate-contracts-node (готовый бинарник с pallet-contracts)
cargo install contracts-node --git https://github.com/paritytech/substrate-contracts-node.git --force --locked
```

### 2. Сборка и тестирование смарт-контракта

```bash
cd contracts/simple_storage

# Юнит-тесты
cargo +nightly test

# Сборка в WASM
cargo +nightly contract build --release

# Результат: target/ink/simple_storage.contract и .wasm
```

### 3. Запуск ноды

```bash
substrate-contracts-node --dev --tmp
# RPC: ws://127.0.0.1:9944
```

### 4. Деплой контракта (через cargo-contract)

```bash
cd contracts/simple_storage

cargo +nightly contract instantiate \
  --constructor new \
  --args 42 \
  --suri //Alice \
  --url ws://127.0.0.1:9944 \
  -x
```

Запомните `Contract` address из вывода.

### 5. Вызов методов

```bash
# Чтение
cargo +nightly contract call \
  --contract <ADDRESS> \
  --message get \
  --suri //Alice \
  --url ws://127.0.0.1:9944

# Запись
cargo +nightly contract call \
  --contract <ADDRESS> \
  --message set \
  --args 999 \
  --suri //Alice \
  --url ws://127.0.0.1:9944 \
  -x
```

### 6. Интеграция реального клиента (subxt)

В продакшене замените `SubstrateAdapter` на реализацию через [`subxt`](https://github.com/paritytech/subxt) или `jsonrpsee`:

```rust
// Пример заготовки
use subxt::{OnlineClient, PolkadotConfig};

pub struct RealSubstrateAdapter {
    client: OnlineClient<PolkadotConfig>,
    // ...
}
```

Добавьте в `adapter/Cargo.toml`:

```toml
subxt = { version = "0.34", features = ["substrate-compat"] }
jsonrpsee = { version = "0.22", features = ["async-client", "ws-client"] }
```

---

## Структура проекта

```
saas-substrate/
├── README.md
├── contracts/
│   └── simple_storage/
│       ├── Cargo.toml
│       └── lib.rs              # Ink! контракт
├── adapter/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── domain.rs           # Сущности
│       ├── ports.rs            # Трейты (интерфейсы)
│       ├── adapters.rs         # Реализации
│       ├── services.rs         # Оркестрация
│       └── error.rs
└── api/
    ├── Cargo.toml
    └── src/
        └── main.rs             # Hyper 0.14 сервер
```

---

## Расширение

1. **Реальная нода** — реализуйте `BlockchainPort` через subxt.
2. **Персистентность** — замените `InMemoryTenantRepository` на PostgreSQL/SQLx.
3. **Очереди** — добавьте NATS/Kafka в `EventPublisher`.
4. **Аутентификация** — JWT / API-keys middleware в Actix.
5. **Метрики** — Prometheus + `tracing`.

---

## Лицензия

MIT / Apache-2.0

---

## Production adapters (v0.2)

`SaasService` **не менялся** — подключаются другие реализации портов.

| Порт | Memory (default) | Production feature |
|------|------------------|--------------------|
| BlockchainPort | `SubstrateAdapter` | `substrate-rpc` → `SubstrateRpcAdapter` (jsonrpsee) |
| TenantRepository | `InMemoryTenantRepository` | `postgres` → `PostgresTenantRepository` (SQLx) |
| CachePort | `InMemoryCache` | `redis-cache` → `RedisCache` |
| EventPublisher | `LoggingEventPublisher` | `nats` → `NatsEventPublisher` |
| BillingPort | `InMemoryBilling` | (тот же API; можно вынести в Redis/PG) |

### Биллинг без правок SaasService

```rust
let chain = Arc::new(SubstrateAdapter::new("dev", 32));
let billed = Arc::new(BillingBlockchainProxy::new(chain, billing, tenants));
let service = SaasService::new(billed, tenants, cache, events);
```

Декоратор проверяет `TenantPlan` (daily calls/gas, concurrency) и пишет usage.

### Auth

- При регистрации возвращается **одноразовый** `api_key` (`sk_...`), в БД хранится SHA-256.
- Заголовок: `Authorization: Bearer sk_...` или demo JWT.
- `AUTH_REQUIRED=true` — защита deploy/call.
- `GET /usage` — счётчики за день.
- `POST /auth/token` — demo JWT.

### Сборка с бэкендами

```bash
# только memory (как раньше)
cargo run -p saas-api

# postgres + redis + nats + rpc
docker compose up -d
cargo run -p saas-api --features full
```

Env:

```
DATABASE_URL=postgres://saas:saas@127.0.0.1:5432/saas
REDIS_URL=redis://127.0.0.1:6379
NATS_URL=nats://127.0.0.1:4222
SUBSTRATE_WS=ws://127.0.0.1:9944
JWT_SECRET=change-me
AUTH_REQUIRED=true
BIND_ADDR=0.0.0.0:8080
```

### subxt

Для полностью типизированных Ink!-вызовов замените тело `SubstrateRpcAdapter` на `subxt::OnlineClient` + сгенерированные runtime APIs; трейт `BlockchainPort` останется тем же.
