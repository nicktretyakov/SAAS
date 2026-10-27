//! Hyper API with API-key / JWT auth and billing-aware composition root.

use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use uuid::Uuid;

use saas_adapter::{
    authenticate, issue_demo_jwt,
    adapters::{
        BillingBlockchainProxy, InMemoryBilling, InMemoryCache, InMemoryTenantRepository,
        LoggingEventPublisher, SubstrateAdapter,
    },
    domain::ContractCallRequest,
    services::SaasService,
    AdapterError, BillingPort,
};

struct AppState {
    service: Arc<SaasService>,
    billing: Arc<dyn BillingPort>,
    jwt_secret: String,
    /// If false, /tenants and /health stay open; mutating routes require auth
    auth_required: bool,
}

#[derive(Debug, Deserialize)]
struct RegisterTenantDto {
    name: String,
}

#[derive(Debug, Deserialize)]
struct DeployDto {
    wasm_path: String,
    #[serde(default = "default_constructor")]
    constructor: String,
    #[serde(default)]
    args: serde_json::Value,
}

fn default_constructor() -> String {
    "new".to_string()
}

#[derive(Debug, Deserialize)]
struct CallDto {
    contract_address: String,
    method: String,
    #[serde(default)]
    args: serde_json::Value,
    caller: Option<String>,
}

#[derive(Debug, Serialize)]
struct ApiResponse<T: Serialize> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }
}

impl ApiResponse<()> {
    fn err(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(msg.into()),
        }
    }
}

fn json_response<T: Serialize>(status: StatusCode, body: &T) -> Response<Body> {
    let json = serde_json::to_string(body).unwrap_or_else(|_| "{}".to_string());
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(Body::from(json))
        .unwrap()
}

fn map_error(e: AdapterError) -> Response<Body> {
    let status = match &e {
        AdapterError::Validation(_) => StatusCode::BAD_REQUEST,
        AdapterError::TenantNotFound(_) => StatusCode::NOT_FOUND,
        AdapterError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
        AdapterError::QuotaExceeded(_) => StatusCode::TOO_MANY_REQUESTS,
        AdapterError::Connection(_) | AdapterError::Timeout => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    json_response(status, &ApiResponse::<()>::err(e.to_string()))
}

fn header_str<'a>(req: &'a Request<Body>, name: &str) -> Option<&'a str> {
    req.headers().get(name).and_then(|v| v.to_str().ok())
}

async fn handle(req: Request<Body>, state: Arc<AppState>) -> Result<Response<Body>, Infallible> {
    let path = req.uri().path().to_string();
    let method = req.method().clone();

    // Public health
    if method == Method::GET && path == "/health" {
        return Ok(match state.service.node_status().await {
            Ok(status) => json_response(StatusCode::OK, &ApiResponse::ok(status)),
            Err(e) => map_error(e),
        });
    }

    // Auth helper for protected routes
    let auth = if state.auth_required
        && !(method == Method::POST && path == "/tenants")
    {
        match authenticate(
            header_str(&req, "Authorization"),
            state.service.tenants().as_ref(),
            Some(&state.jwt_secret),
        )
        .await
        {
            Ok(ctx) => Some(ctx),
            Err(e) => return Ok(map_error(e)),
        }
    } else {
        None
    };

    // POST /tenants — register (returns api_key once)
    if method == Method::POST && path == "/tenants" {
        let bytes = hyper::body::to_bytes(req.into_body()).await.unwrap_or_default();
        let dto: RegisterTenantDto = match serde_json::from_slice(&bytes) {
            Ok(d) => d,
            Err(e) => {
                return Ok(json_response(
                    StatusCode::BAD_REQUEST,
                    &ApiResponse::<()>::err(format!("invalid json: {}", e)),
                ));
            }
        };
        return Ok(match state.service.register_tenant(dto.name).await {
            Ok(result) => json_response(StatusCode::CREATED, &ApiResponse::ok(result)),
            Err(e) => map_error(e),
        });
    }

    // GET /tenants
    if method == Method::GET && path == "/tenants" {
        return Ok(match state.service.list_tenants().await {
            Ok(list) => json_response(StatusCode::OK, &ApiResponse::ok(list)),
            Err(e) => map_error(e),
        });
    }

    // GET /usage — billing stats for authenticated tenant
    if method == Method::GET && path == "/usage" {
        let ctx = match auth {
            Some(c) => c,
            None => {
                // try auth even if auth_required=false
                match authenticate(
                    header_str(&req, "Authorization"),
                    state.service.tenants().as_ref(),
                    Some(&state.jwt_secret),
                )
                .await
                {
                    Ok(c) => c,
                    Err(e) => return Ok(map_error(e)),
                }
            }
        };
        return Ok(match state.billing.get_usage(ctx.tenant_id).await {
            Ok(u) => json_response(StatusCode::OK, &ApiResponse::ok(u)),
            Err(e) => map_error(e),
        });
    }

    // POST /auth/token — issue demo JWT for a tenant (dev helper)
    if method == Method::POST && path == "/auth/token" {
        let bytes = hyper::body::to_bytes(req.into_body()).await.unwrap_or_default();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
        let tid = body
            .get("tenant_id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());
        let Some(tid) = tid else {
            return Ok(json_response(
                StatusCode::BAD_REQUEST,
                &ApiResponse::<()>::err("tenant_id required"),
            ));
        };
        let token = issue_demo_jwt(tid, "demo", &state.jwt_secret);
        return Ok(json_response(
            StatusCode::OK,
            &ApiResponse::ok(serde_json::json!({ "token": token })),
        ));
    }

    // POST /tenants/{id}/deploy
    if method == Method::POST && path.starts_with("/tenants/") && path.ends_with("/deploy") {
        let id_str = path
            .trim_start_matches("/tenants/")
            .trim_end_matches("/deploy");
        let tenant_id = match Uuid::parse_str(id_str) {
            Ok(id) => id,
            Err(_) => {
                return Ok(json_response(
                    StatusCode::BAD_REQUEST,
                    &ApiResponse::<()>::err("invalid tenant id"),
                ));
            }
        };
        if let Some(ref ctx) = auth {
            if ctx.tenant_id != tenant_id {
                return Ok(map_error(AdapterError::Unauthorized(
                    "tenant_id mismatch".into(),
                )));
            }
        }
        let bytes = hyper::body::to_bytes(req.into_body()).await.unwrap_or_default();
        let dto: DeployDto = match serde_json::from_slice(&bytes) {
            Ok(d) => d,
            Err(e) => {
                return Ok(json_response(
                    StatusCode::BAD_REQUEST,
                    &ApiResponse::<()>::err(format!("invalid json: {}", e)),
                ));
            }
        };
        return Ok(
            match state
                .service
                .deploy_for_tenant(tenant_id, dto.wasm_path, dto.constructor, dto.args)
                .await
            {
                Ok(tenant) => json_response(StatusCode::OK, &ApiResponse::ok(tenant)),
                Err(e) => map_error(e),
            },
        );
    }

    // POST /tenants/{id}/call
    if method == Method::POST && path.starts_with("/tenants/") && path.ends_with("/call") {
        let id_str = path
            .trim_start_matches("/tenants/")
            .trim_end_matches("/call");
        let tenant_id = match Uuid::parse_str(id_str) {
            Ok(id) => id,
            Err(_) => {
                return Ok(json_response(
                    StatusCode::BAD_REQUEST,
                    &ApiResponse::<()>::err("invalid tenant id"),
                ));
            }
        };
        if let Some(ref ctx) = auth {
            if ctx.tenant_id != tenant_id {
                return Ok(map_error(AdapterError::Unauthorized(
                    "tenant_id mismatch".into(),
                )));
            }
        }
        let bytes = hyper::body::to_bytes(req.into_body()).await.unwrap_or_default();
        let dto: CallDto = match serde_json::from_slice(&bytes) {
            Ok(d) => d,
            Err(e) => {
                return Ok(json_response(
                    StatusCode::BAD_REQUEST,
                    &ApiResponse::<()>::err(format!("invalid json: {}", e)),
                ));
            }
        };
        let request = ContractCallRequest {
            tenant_id,
            contract_address: dto.contract_address,
            method: dto.method,
            args: dto.args,
            caller: dto.caller,
        };
        return Ok(match state.service.call(request).await {
            Ok(result) => json_response(StatusCode::OK, &ApiResponse::ok(result)),
            Err(e) => map_error(e),
        });
    }

    Ok(json_response(
        StatusCode::NOT_FOUND,
        &ApiResponse::<()>::err("not found"),
    ))
}

/// Composition root: wire adapters from env without touching SaasService.
async fn build_state() -> Arc<AppState> {
    let tenants = Arc::new(InMemoryTenantRepository::new());
    let cache = Arc::new(InMemoryCache::new());
    let events = Arc::new(LoggingEventPublisher);
    let billing: Arc<dyn BillingPort> = Arc::new(InMemoryBilling::new());

    // Base chain adapter (simulation). With feature substrate-rpc + SUBSTRATE_WS:
    //   let chain = Arc::new(SubstrateRpcAdapter::connect(&ws, "dev", 32).await.unwrap());
    let raw_chain = Arc::new(SubstrateAdapter::new("saas-devnet", 32));

    // Decorator adds billing — SaasService unchanged
    let blockchain = Arc::new(BillingBlockchainProxy::new(
        raw_chain,
        billing.clone(),
        tenants.clone(),
    ));

    let service = Arc::new(SaasService::new(blockchain, tenants, cache, events));

    Arc::new(AppState {
        service,
        billing,
        jwt_secret: std::env::var("JWT_SECRET").unwrap_or_else(|_| "dev-secret-change-me".into()),
        auth_required: std::env::var("AUTH_REQUIRED")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false),
    })
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,saas_api=debug,saas_adapter=debug".into()),
        )
        .init();

    let state = build_state().await;

    let addr: SocketAddr = std::env::var("BIND_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_string())
        .parse()
        .expect("invalid BIND_ADDR");

    let make_svc = make_service_fn(move |_conn| {
        let state = state.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                let state = state.clone();
                handle(req, state)
            }))
        }
    });

    tracing::info!(
        "SaaS API on {} (auth_required={})",
        addr,
        std::env::var("AUTH_REQUIRED").unwrap_or_else(|_| "false".into())
    );
    if let Err(e) = Server::bind(&addr).serve(make_svc).await {
        tracing::error!("server error: {}", e);
    }
}
