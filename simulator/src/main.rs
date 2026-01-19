use serde::{Deserialize, Serialize};
use std::io::{self, Read};
use soroban_env_host::xdr::ReadXdr;
use base64::{Engine as _};
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
struct SimulationRequest {
    envelope_xdr: String,
    result_meta_xdr: String,
    // Key XDR -> Entry XDR
    ledger_entries: Option<HashMap<String, String>>,
}

#[derive(Debug, Serialize)]
struct SimulationResponse {
    status: String,
    error: Option<String>,
    events: Vec<String>,
    logs: Vec<String>,
}

fn main() {
    // Read JSON from Stdin
    let mut buffer = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut buffer) {
        eprintln!("Failed to read stdin: {}", e);
        return;
    }

    // Parse Request
    let request: SimulationRequest = match serde_json::from_str(&buffer) {
        Ok(req) => req,
        Err(e) => {
            let res = SimulationResponse {
                status: "error".to_string(),
                error: Some(format!("Invalid JSON: {}", e)),
                events: vec![],
                logs: vec![],
            };
            println!("{}", serde_json::to_string(&res).unwrap());
            return;
        }
    };

    // Decode Envelope XDR
    let envelope = match base64::engine::general_purpose::STANDARD.decode(&request.envelope_xdr) {
        Ok(bytes) => match soroban_env_host::xdr::TransactionEnvelope::from_xdr(bytes, soroban_env_host::xdr::Limits::none()) {
            Ok(env) => env,
            Err(e) => {
                return send_error(format!("Failed to parse Envelope XDR: {}", e));
            }
        },
        Err(e) => {
            return send_error(format!("Failed to decode Envelope Base64: {}", e));
        }
    };

    // Decode ResultMeta XDR
    let _result_meta = match base64::engine::general_purpose::STANDARD.decode(&request.result_meta_xdr) {
        Ok(bytes) => match soroban_env_host::xdr::TransactionResultMeta::from_xdr(bytes, soroban_env_host::xdr::Limits::none()) {
            Ok(meta) => meta,
            Err(e) => {
                return send_error(format!("Failed to parse ResultMeta XDR: {}", e));
            }
        },
        Err(e) => {
            return send_error(format!("Failed to decode ResultMeta Base64: {}", e));
        }
    };

    // Initialize Host
    let host = soroban_env_host::Host::default();
    host.set_diagnostic_level(soroban_env_host::DiagnosticLevel::Debug).unwrap();

    let mut loaded_entries_count = 0;

    // Populate Host Storage
    if let Some(entries) = &request.ledger_entries {
        for (key_xdr, entry_xdr) in entries {
            // Decode Key
            let key = match base64::engine::general_purpose::STANDARD.decode(key_xdr) {
                Ok(b) => match soroban_env_host::xdr::LedgerKey::from_xdr(b, soroban_env_host::xdr::Limits::none()) {
                    Ok(k) => k,
                    Err(e) => return send_error(format!("Failed to parse LedgerKey XDR: {}", e)),
                },
                Err(e) => return send_error(format!("Failed to decode LedgerKey Base64: {}", e)),
            };

            // Decode Entry
            let entry = match base64::engine::general_purpose::STANDARD.decode(entry_xdr) {
                Ok(b) => match soroban_env_host::xdr::LedgerEntry::from_xdr(b, soroban_env_host::xdr::Limits::none()) {
                    Ok(e) => e,
                    Err(e) => return send_error(format!("Failed to parse LedgerEntry XDR: {}", e)),
                },
                Err(e) => return send_error(format!("Failed to decode LedgerEntry Base64: {}", e)),
            };

            // TODO: Inject into host storage. 
            // For MVP, we verify we can parse them.
            eprintln!("Parsed Ledger Entry: Key={:?}, Entry={:?}", key, entry);
            loaded_entries_count += 1;
        }
    }

    // Mock Success Response
    let response = SimulationResponse {
        status: "success".to_string(),
        error: None,
        events: vec![format!("Parsed Envelope: {:?}", envelope)],
        logs: vec![
            format!("Host Initialized with Budget: {:?}", host.budget_cloned()),
            format!("Loaded {} Ledger Entries", loaded_entries_count)
        ],
    };

    println!("{}", serde_json::to_string(&response).unwrap());
}

fn send_error(msg: String) {
    let res = SimulationResponse {
        status: "error".to_string(),
        error: Some(msg),
        events: vec![],
        logs: vec![],
    };
    println!("{}", serde_json::to_string(&res).unwrap());
}
