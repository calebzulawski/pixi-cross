//! Pixi build protocol over newline-delimited JSON-RPC on stdio.
use std::io::{BufRead, Write};
use std::sync::{Arc, OnceLock};

use anyhow::Result;
use jsonrpc_core::{Error, ErrorCode, IoHandler, Params};
use pixi_build_types::{
    BackendCapabilities,
    procedures::{
        conda_outputs,
        initialize::{self, InitializeParams, InitializeResult},
        negotiate_capabilities::{self, NegotiateCapabilitiesParams, NegotiateCapabilitiesResult},
    },
};
use serde::{Serialize, de::DeserializeOwned};

mod outputs;

pub fn serve_build_protocol() -> Result<()> {
    let rpc = build_protocol_handler();
    let mut stdout = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        if let Some(response) = rpc.handle_request_sync(&line?) {
            writeln!(stdout, "{response}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

fn build_protocol_handler() -> IoHandler {
    let mut rpc = IoHandler::new();
    register_procedure(
        &mut rpc,
        negotiate_capabilities::METHOD_NAME,
        |_: NegotiateCapabilitiesParams| {
            Ok(NegotiateCapabilitiesResult {
                capabilities: BackendCapabilities {
                    provides_conda_outputs: Some(true),
                    ..Default::default()
                },
            })
        },
    );

    let manifest = Arc::new(OnceLock::new());
    let initialized = Arc::clone(&manifest);
    register_procedure(
        &mut rpc,
        initialize::METHOD_NAME,
        move |params: InitializeParams| {
            let path = params
                .source_directory
                .map(|dir| dir.join("pixi.toml"))
                .unwrap_or_else(|| params.manifest_path.with_file_name("pixi.toml"));
            initialized
                .set(path)
                .map_err(|_| Error::invalid_request())?;
            Ok(InitializeResult {})
        },
    );
    register_procedure(&mut rpc, conda_outputs::METHOD_NAME, move |params| {
        let path = manifest.get().ok_or_else(Error::invalid_request)?;
        outputs::describe_conda_outputs(path, params).map_err(|err| Error {
            code: ErrorCode::ServerError(-32000),
            message: format!("{err:#}"),
            data: None,
        })
    });
    rpc
}

/// Decode and encode each procedure with the request and result types from pixi_build_types.
fn register_procedure<P: DeserializeOwned, R: Serialize>(
    rpc: &mut IoHandler,
    method: &str,
    handle: impl Fn(P) -> jsonrpc_core::Result<R> + Send + Sync + 'static,
) {
    rpc.add_sync_method(method, move |params: Params| {
        let result = handle(params.parse()?)?;
        serde_json::to_value(result).map_err(|_| Error::internal_error())
    });
}
