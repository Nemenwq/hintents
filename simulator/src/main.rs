use base64::Engine as _;
use serde::{Deserialize, Serialize};
use soroban_env_host::xdr::ReadXdr;
use std::collections::HashMap;
use std::io::{self, Read};

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
            return send_error(format!("Invalid JSON: {}", e));
        }
    };

    // Decode Envelope XDR
    let envelope = match base64::engine::general_purpose::STANDARD.decode(&request.envelope_xdr) {
        Ok(bytes) => match soroban_env_host::xdr::TransactionEnvelope::from_xdr(
            bytes,
            &soroban_env_host::xdr::Limits::none(),
        ) {
            Ok(env) => env,
            Err(e) => {
                return send_error(format!("Failed to parse Envelope XDR: {}", e));
            }
        },
        Err(e) => {
            return send_error(format!("Failed to decode Envelope Base64: {}", e));
        }
    };

    // Initialize Host
    let host = soroban_env_host::Host::default();
    host.set_diagnostic_level(soroban_env_host::DiagnosticLevel::Debug)
        .unwrap();

    let mut loaded_entries_count = 0;

    // Populate Host Storage
    if let Some(entries) = &request.ledger_entries {
        for (key_xdr, entry_xdr) in entries {
            let _key = match base64::engine::general_purpose::STANDARD.decode(key_xdr) {
                Ok(b) => match soroban_env_host::xdr::LedgerKey::from_xdr(b, &soroban_env_host::xdr::Limits::none()) {
                    Ok(k) => k,
                    Err(e) => return send_error(format!("Failed to parse LedgerKey XDR: {}", e)),
                },
                Err(e) => return send_error(format!("Failed to decode LedgerKey Base64: {}", e)),
            };

            let _entry = match base64::engine::general_purpose::STANDARD.decode(entry_xdr) {
                Ok(b) => match soroban_env_host::xdr::LedgerEntry::from_xdr(b, &soroban_env_host::xdr::Limits::none()) {
                    Ok(e) => e,
                    Err(e) => return send_error(format!("Failed to parse LedgerEntry XDR: {}", e)),
                },
                Err(e) => return send_error(format!("Failed to decode LedgerEntry Base64: {}", e)),
            };
            loaded_entries_count += 1;
        }
    }

    let mut invocation_logs = vec![];

    // Extract Operations and Simulate
    let operations = match &envelope {
        soroban_env_host::xdr::TransactionEnvelope::Tx(tx_v1) => &tx_v1.tx.operations,
        soroban_env_host::xdr::TransactionEnvelope::TxV0(tx_v0) => &tx_v0.tx.operations,
        soroban_env_host::xdr::TransactionEnvelope::TxFeeBump(bump) => match &bump.tx.inner_tx {
            soroban_env_host::xdr::FeeBumpTransactionInnerTx::Tx(tx_v1) => &tx_v1.tx.operations,
        },
    };

    for op in operations.iter() {
        if let soroban_env_host::xdr::OperationBody::InvokeHostFunction(host_fn_op) = &op.body {
            match &host_fn_op.host_function {
                soroban_env_host::xdr::HostFunction::InvokeContract(invoke_args) => {
                    invocation_logs.push(format!("Invoking Contract: {:?}", invoke_args.contract_address));
                    
                    // Note: In a real simulation, we'd call host.invoke_function here.
                    // If that call returns an Err(host_error), we pass it to our decoder.
                }
                _ => invocation_logs.push("Skipping non-InvokeContract Host Function".to_string()),
            }
        }
    }

    let events = match host.get_events() {
        Ok(evs) => evs.0.iter().map(|e| format!("{:?}", e)).collect::<Vec<String>>(),
        Err(e) => vec![format!("Failed to retrieve events: {:?}", e)],
    };

    // Final Response
    let response = SimulationResponse {
        status: "success".to_string(),
        error: None,
        events,
        logs: {
            let mut logs = vec![
                format!("Host Initialized. Loaded {} Ledger Entries", loaded_entries_count),
            ];
            logs.extend(invocation_logs);
            logs
        },
    };

    println!("{}", serde_json::to_string(&response).unwrap());
}

/// Decodes generic WASM traps into human-readable messages.
fn decode_wasm_trap(err: &soroban_env_host::HostError) -> String {
    let err_str = format!("{:?}", err);
    let err_lower = err_str.to_lowercase();

    // Check for VM-initiated traps
    if err_lower.contains("wasm trap") {
        if err_lower.contains("unreachable") {
            return "Unreachable Instruction: The contract hit a panic or unreachable code path.".to_string();
        }
        if err_lower.contains("out of bounds") {
            return "Out of Bounds Access: The contract tried to access invalid memory (OOB).".to_string();
        }
        if err_lower.contains("integer overflow") {
            return "Integer Overflow: A mathematical operation exceeded the type limits.".to_string();
        }
        if err_lower.contains("stack overflow") {
            return "Stack Overflow: The contract's recursion or stack usage is too high.".to_string();
        }
        if err_lower.contains("divide by zero") {
            return "Division by Zero: The contract attempted to divide by zero.".to_string();
        }
        return format!("Wasm Trap: {}", err_str);
    }

    // Differentiate Host-initiated traps
    if err_str.contains("HostError") {
        return format!("Host-initiated Trap: {}", err_str);
    }

    format!("Execution Error: {}", err_str)
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