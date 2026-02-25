// Copyright 2025 Erst Users
// SPDX-License-Identifier: Apache-2.0

#![allow(unused_imports, unused_variables, clippy::useless_format)]

mod config;
mod gas_optimizer;
mod runner;
mod snapshot;
mod source_map_cache;
mod source_mapper;
mod stack_trace;
mod types;
mod vm;
mod wasm;

use crate::gas_optimizer::{BudgetMetrics, GasOptimizationAdvisor, CPU_LIMIT, MEMORY_LIMIT};
use crate::source_mapper::SourceMapper;
use crate::stack_trace::{decode_error, WasmStackTrace};
use crate::types::*;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use soroban_env_host::xdr::ReadXdr;
use soroban_env_host::{
    xdr::{Operation, OperationBody},
    Host, HostError,
};
use std::collections::HashMap;
use std::env;
use std::io::{self, Read};
use tracing_subscriber::{fmt, EnvFilter};

// Use types::SimulationRequest directly

fn init_logger() {
    let use_json = env::var("ERST_LOG_FORMAT")
        .map(|val| val.to_lowercase() == "json")
        .unwrap_or(false);

    // Honour ERST_LOG_LEVEL first so a single variable controls both Go and
    // Rust log output.  Fall back to RUST_LOG, then default to "info".
    let filter = if let Ok(erst_level) = env::var("ERST_LOG_LEVEL") {
        EnvFilter::new(erst_level)
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };

    let subscriber = fmt::Subscriber::builder()
        .with_env_filter(filter)
        .with_writer(std::io::stderr);

    if use_json {
        subscriber.json().flatten_event(true).init();
    } else {
        subscriber.compact().init();
    }
}

fn send_error(msg: String) {
    let trace = WasmStackTrace::from_host_error(&msg);
    let res = SimulationResponse {
        status: "error".to_string(),
        error: Some(msg),
        events: vec![],
        diagnostic_events: vec![],
        categorized_events: vec![],
        logs: vec![],
        flamegraph: None,
        optimization_report: None,
        budget_usage: None,
        source_location: None,
        stack_trace: Some(trace),
        wasm_offset: None,
    };
    println!("{}", serde_json::to_string(&res).unwrap());
    std::process::exit(1);
}

fn execute_operations(host: &Host, operations: &[Operation]) -> Result<Vec<String>, HostError> {
    let mut logs = Vec::new();
    for op in operations {
        match &op.body {
            OperationBody::InvokeHostFunction(invoke_op) => {
                logs.push("Executing InvokeHostFunction...".to_string());
                let val = host.invoke_function(invoke_op.host_function.clone())?;
                logs.push(format!("Result: {val:?}"));
            }
            _ => {
                logs.push(format!(
                    "Skipping non-Soroban operation: {:?}",
                    op.body.name()
                ));
            }
        }
    }
    Ok(logs)
}

fn transaction_fee_stroops(envelope: &soroban_env_host::xdr::TransactionEnvelope) -> u64 {
    match envelope {
        soroban_env_host::xdr::TransactionEnvelope::Tx(tx_v1) => tx_v1.tx.fee as u64,
        soroban_env_host::xdr::TransactionEnvelope::TxV0(tx_v0) => tx_v0.tx.fee as u64,
        soroban_env_host::xdr::TransactionEnvelope::TxFeeBump(bump) => bump.tx.fee as u64,
    }
}

fn mocked_required_fee_stroops(
    request: &SimulationRequest,
    operations_count: usize,
    cpu_insns: u64,
    mem_bytes: u64,
) -> Option<u64> {
    let mut required_fee = 0u64;
    let mut enabled = false;

    if let Some(base_fee) = request.mock_base_fee {
        enabled = true;
        required_fee =
            required_fee.saturating_add((base_fee as u64).saturating_mul(operations_count as u64));
    }

    if let Some(gas_price) = request.mock_gas_price {
        enabled = true;
        let cpu_units = cpu_insns.saturating_add(9_999) / 10_000;
        let mem_units = mem_bytes.saturating_add(1_023) / 1_024;
        let resource_units = cpu_units.saturating_add(mem_units).max(1);
        required_fee = required_fee.saturating_add(gas_price.saturating_mul(resource_units));
    }

    if enabled {
        Some(required_fee)
    } else {
        None
    }
}

fn extract_wasm_instruction(topics: &[String], data: &str) -> Option<String> {
    if !topics.iter().any(|t| t.contains("budget")) {
        return None;
    }
    if let Some(pos) = data.find("Instruction: ") {
        let rest = &data[pos + "Instruction: ".len()..];
        let end = rest.find('"').unwrap_or(rest.len());
        return Some(rest[..end].to_string());
    }
    None
}

fn extract_wasm_offset(error_msg: &str) -> Option<u64> {
    for line in error_msg.lines() {
        if let Some(pos) = line.find("@ 0x") {
            let hex_part = &line[pos + 4..];
            let end = hex_part
                .find(|c: char| !c.is_ascii_hexdigit())
                .unwrap_or(hex_part.len());
            if let Ok(offset) = u64::from_str_radix(&hex_part[..end], 16) {
                return Some(offset);
            }
        }
    }
    None
}

fn categorize_events(events: &soroban_env_host::events::Events) -> Vec<CategorizedEvent> {
    events
        .0
        .iter()
        .map(|e| {
            let category = match e.event.type_ {
                soroban_env_host::xdr::ContractEventType::Contract => "Contract",
                soroban_env_host::xdr::ContractEventType::System => "System",
                soroban_env_host::xdr::ContractEventType::Diagnostic => "Diagnostic",
            }
            .to_string();

            let contract_id = e.event.contract_id.as_ref().map(|id| format!("{id:?}"));
            let topics: Vec<String> = match &e.event.body {
                soroban_env_host::xdr::ContractEventBody::V0(v0) => {
                    v0.topics.iter().map(|t| format!("{t:?}")).collect()
                }
            };
            let data = match &e.event.body {
                soroban_env_host::xdr::ContractEventBody::V0(v0) => format!("{:?}", v0.data),
            };

            let wasm_instruction = extract_wasm_instruction(&topics, &data);
            CategorizedEvent {
                category,
                event: DiagnosticEvent {
                    event_type: match e.event.type_ {
                        soroban_env_host::xdr::ContractEventType::Contract => {
                            "contract".to_string()
                        }
                        soroban_env_host::xdr::ContractEventType::System => "system".to_string(),
                        soroban_env_host::xdr::ContractEventType::Diagnostic => {
                            "diagnostic".to_string()
                        }
                    },
                    contract_id,
                    topics,
                    data,
                    // failed_call=true means the call that emitted this event
                    // actually failed; so a successful call is the inverse.
                    in_successful_contract_call: !e.failed_call,
                    wasm_instruction,
                },
            }
        })
        .collect()
}

fn build_diagnostic_events(host: &Host) -> (Vec<String>, Vec<DiagnosticEvent>) {
    match host.get_events() {
        Ok(evs) => {
            let raw_events: Vec<String> = evs.0.iter().map(|e| format!("{e:?}")).collect();
            let diag_events: Vec<DiagnosticEvent> = evs
                .0
                .iter()
                .map(|event| {
                    let event_type = match &event.event.type_ {
                        soroban_env_host::xdr::ContractEventType::Contract => {
                            "contract".to_string()
                        }
                        soroban_env_host::xdr::ContractEventType::System => "system".to_string(),
                        soroban_env_host::xdr::ContractEventType::Diagnostic => {
                            "diagnostic".to_string()
                        }
                    };

                    let contract_id = event
                        .event
                        .contract_id
                        .as_ref()
                        .map(|contract_id| format!("{contract_id:?}"));

                    let (topics, data) = match &event.event.body {
                        soroban_env_host::xdr::ContractEventBody::V0(v0) => {
                            let topics: Vec<String> =
                                v0.topics.iter().map(|t| format!("{t:?}")).collect();
                            let data = format!("{:?}", v0.data);
                            (topics, data)
                        }
                    };

                    DiagnosticEvent {
                        event_type,
                        contract_id,
                        topics,
                        data,
                        in_successful_contract_call: !event.failed_call,
                        wasm_instruction: None,
                    }
                })
                .collect();
            (raw_events, diag_events)
        }
        Err(_) => (
            vec!["Failed to retrieve events".to_string()],
            Vec::<DiagnosticEvent>::new(),
        ),
    }
}

/// Main entry point for the erst simulator.
///
/// Reads a JSON `SimulationRequest` from stdin,
/// initializes a Soroban host environment, and outputs a JSON
/// `SimulationResponse` with simulation results or errors.
///
/// # Panics
///
/// May panic if JSON serialization of the response fails (should not happen
/// with valid `SimulationResponse` structures).
fn main() {
    init_logger();

    tracing::info!(event = "simulator_started", "Simulator initializing...");

    // Read JSON from Stdin
    let mut buffer = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut buffer) {
        let res = SimulationResponse {
            status: "error".to_string(),
            error: Some(format!("Failed to read stdin: {e}")),
            events: vec![],
            diagnostic_events: vec![],
            categorized_events: vec![],
            logs: vec![],
            flamegraph: None,
            optimization_report: None,
            budget_usage: None,
            source_location: None,
            stack_trace: None,
            wasm_offset: None,
        };
        println!("{}", serde_json::to_string(&res).unwrap());
        tracing::error!("Failed to read stdin: {e}");
        return;
    }

    // Parse Request
    let request: SimulationRequest = match serde_json::from_str(&buffer) {
        Ok(req) => req,
        Err(e) => {
            let res = SimulationResponse {
                status: "error".to_string(),
                error: Some(format!("Invalid JSON: {e}")),
                events: vec![],
                diagnostic_events: vec![],
                categorized_events: vec![],
                logs: vec![],
                flamegraph: None,
                optimization_report: None,
                budget_usage: None,
                source_location: None,
                stack_trace: None,
                wasm_offset: None,
            };
            println!(
                "{}",
                serde_json::to_string(&res).expect("Failed to serialize error response")
            );
            return;
        }
    };

    // Decode Envelope XDR
    let envelope = match base64::engine::general_purpose::STANDARD.decode(&request.envelope_xdr) {
        Ok(bytes) => match soroban_env_host::xdr::TransactionEnvelope::from_xdr(
            bytes,
            soroban_env_host::xdr::Limits::none(),
        ) {
            Ok(env) => env,
            Err(e) => {
                send_error(format!("Failed to parse Envelope XDR: {}", e));
                return;
            }
        },
        Err(e) => {
            send_error(format!("Failed to decode Envelope Base64: {}", e));
            return;
        }
    };

    // Decode ResultMeta XDR
    tracing::debug!(
        result_meta_xdr_len = request.result_meta_xdr.len(),
        "Received ResultMetaXdr"
    );

    let _result_meta = if request.result_meta_xdr.is_empty() {
        tracing::warn!("ResultMetaXdr is empty, host storage may be incomplete");
        None
    } else {
        match base64::engine::general_purpose::STANDARD.decode(&request.result_meta_xdr) {
            Ok(bytes) => {
                if bytes.is_empty() {
                    tracing::warn!("ResultMetaXdr decoded to 0 bytes");
                    None
                } else {
                    match soroban_env_host::xdr::TransactionResultMeta::from_xdr(
                        &bytes,
                        soroban_env_host::xdr::Limits::none(),
                    ) {
                        Ok(meta) => Some(meta),
                        Err(e) => {
                            tracing::warn!(error = %e, "Failed to parse ResultMeta XDR, proceeding with empty storage");
                            None
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to decode ResultMeta Base64, proceeding with empty storage");
                None
            }
        }
    };

    // Initialize source mapper if WASM is provided
    let source_mapper = if let Some(wasm_base64) = &request.contract_wasm {
        match base64::engine::general_purpose::STANDARD.decode(wasm_base64) {
            Ok(wasm_bytes) => {
                if let Err(e) = vm::enforce_soroban_compatibility(&wasm_bytes) {
                    return send_error(format!("Strict VM enforcement failed: {}", e));
                }
                let mapper = SourceMapper::new(wasm_bytes);
                if mapper.has_debug_symbols() {
                    tracing::debug!("Debug symbols found in WASM");
                    Some(mapper)
                } else {
                    tracing::debug!("No debug symbols found in WASM");
                    None
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to decode WASM base64");
                None
            }
        }
    } else {
        None
    };

    // Initialize Host
    let sim_host = runner::SimHost::new(None, request.resource_calibration.clone());
    let host = sim_host.inner;

    // Local WASM loading (Issue #70)
    // Note: direct upload_contract_wasm is pub(crate) in soroban-env-host v25+.
    // WASM bytes are loaded and injected via snapshot ledger entries instead.
    if let Some(path) = &request.wasm_path {
        match wasm::load_wasm_from_path(path) {
            Ok(wasm_bytes) => {
                tracing::info!(path = %path, len = wasm_bytes.len(), "Loaded local WASM (injected via snapshot)");
            }
            Err(e) => send_error(format!("Local WASM loading failed: {}", e)),
        }
    }

    // Populate Host Storage via snapshot
    let snapshot = if let Some(entries) = &request.ledger_entries {
        let snap = snapshot::LedgerSnapshot::new();
        for (key_xdr, entry_xdr) in entries {
            let _key = match base64::engine::general_purpose::STANDARD.decode(key_xdr) {
                Ok(b) => match soroban_env_host::xdr::LedgerKey::from_xdr(
                    b,
                    soroban_env_host::xdr::Limits::none(),
                ) {
                    Ok(k) => k,
                    Err(e) => {
                        send_error(format!("Failed to parse LedgerKey XDR: {}", e));
                        return;
                    }
                },
                Err(e) => {
                    send_error(format!("Failed to decode LedgerKey Base64: {}", e));
                    return;
                }
            };

            let _entry = match base64::engine::general_purpose::STANDARD.decode(entry_xdr) {
                Ok(b) => match soroban_env_host::xdr::LedgerEntry::from_xdr(
                    b,
                    soroban_env_host::xdr::Limits::none(),
                ) {
                    Ok(e) => e,
                    Err(e) => {
                        send_error(format!("Failed to parse LedgerEntry XDR: {}", e));
                        return;
                    }
                },
                Err(e) => {
                    send_error(format!("Failed to decode LedgerEntry Base64: {}", e));
                    return;
                }
            };

            tracing::debug!(key = ?_key, entry = ?_entry, "Parsed ledger entry");
        }
        snap
    } else {
        snapshot::LedgerSnapshot::new()
    };

    let loaded_entries_count = snapshot.len();

    // Extract Operations and Simulate
    let operations = match &envelope {
        soroban_env_host::xdr::TransactionEnvelope::Tx(tx_v1) => &tx_v1.tx.operations,
        soroban_env_host::xdr::TransactionEnvelope::TxV0(tx_v0) => &tx_v0.tx.operations,
        soroban_env_host::xdr::TransactionEnvelope::TxFeeBump(bump) => match &bump.tx.inner_tx {
            soroban_env_host::xdr::FeeBumpTransactionInnerTx::Tx(tx_v1) => &tx_v1.tx.operations,
        },
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        execute_operations(&host, operations)
    }));

    // Budget and Reporting
    let budget = host.budget_cloned();
    let cpu_insns = budget.get_cpu_insns_consumed().unwrap_or(0);
    let mem_bytes = budget.get_mem_bytes_consumed().unwrap_or(0);

    let cpu_usage_percent = (cpu_insns as f64 / CPU_LIMIT as f64) * 100.0;
    let memory_usage_percent = (mem_bytes as f64 / MEMORY_LIMIT as f64) * 100.0;

    let budget_usage = BudgetUsage {
        cpu_instructions: cpu_insns,
        memory_bytes: mem_bytes,
        operations_count: operations.len(),
        cpu_limit: CPU_LIMIT,
        memory_limit: MEMORY_LIMIT,
        cpu_usage_percent,
        memory_usage_percent,
    };

    let optimization_report = if request.enable_optimization_advisor {
        let advisor = GasOptimizationAdvisor::new();
        let metrics = BudgetMetrics {
            cpu_instructions: budget_usage.cpu_instructions,
            memory_bytes: budget_usage.memory_bytes,
            total_operations: budget_usage.operations_count,
        };
        Some(advisor.analyze(&metrics))
    } else {
        None
    };

    let mut flamegraph_svg = None;
    if request.profile.unwrap_or(false) {
        let folded_data = format!("Total;CPU {}\nTotal;Memory {}\n", cpu_insns, mem_bytes);
        let mut result_vec = Vec::new();
        let mut options = inferno::flamegraph::Options::default();
        options.title = "Soroban Resource Consumption".to_string();

        if let Err(e) =
            inferno::flamegraph::from_reader(&mut options, folded_data.as_bytes(), &mut result_vec)
        {
            tracing::error!(error = %e, "Failed to generate flamegraph");
        } else {
            flamegraph_svg = Some(String::from_utf8_lossy(&result_vec).to_string());
        }
    }

    match result {
        Ok(Ok(exec_logs)) => {
            let (events, diagnostic_events) = build_diagnostic_events(&host);

            let categorized_events = match host.get_events() {
                Ok(evs) => categorize_events(&evs),
                Err(_) => vec![],
            };

            let mut final_logs = vec![
                format!("Host Initialized with Budget: {:?}", budget),
                format!("Loaded {} Ledger Entries", loaded_entries_count),
                format!("Captured {} diagnostic events", diagnostic_events.len()),
                format!("CPU Instructions Used: {}", cpu_insns),
                format!("Memory Bytes Used: {}", mem_bytes),
            ];
            final_logs.extend(exec_logs);

            if let Some(required_fee) = mocked_required_fee_stroops(
                &request,
                operations.as_slice().len(),
                cpu_insns,
                mem_bytes,
            ) {
                let declared_fee = transaction_fee_stroops(&envelope);
                final_logs.push(format!(
                    "Mock fee check: declared={} required={}",
                    declared_fee, required_fee
                ));

                if declared_fee < required_fee {
                    let response = SimulationResponse {
                        status: "error".to_string(),
                        error: Some(format!(
                            "insufficient fee (mocked): declared {} stroops, required {} stroops",
                            declared_fee, required_fee
                        )),
                        events,
                        diagnostic_events,
                        categorized_events,
                        logs: final_logs,
                        flamegraph: flamegraph_svg,
                        optimization_report,
                        budget_usage: Some(budget_usage),
                        source_location: None,
                        stack_trace: None,
                        wasm_offset: None,
                    };

                    println!("{}", serde_json::to_string(&response).unwrap());
                    return;
                }
            }

            let response = SimulationResponse {
                status: "success".to_string(),
                error: None,
                events,
                diagnostic_events,
                categorized_events,
                logs: final_logs,
                flamegraph: flamegraph_svg,
                optimization_report,
                budget_usage: Some(budget_usage),
                source_location: source_mapper
                    .as_ref()
                    .and_then(|m| m.map_wasm_offset_to_source(0))
                    .and_then(|loc| serde_json::to_string(&loc).ok()),
                stack_trace: None,
                wasm_offset: None,
            };

            println!("{}", serde_json::to_string(&response).unwrap());
        }
        Ok(Err(host_error)) => {
            let error_debug = format!("{:?}", host_error);
            let wasm_trace = WasmStackTrace::from_host_error(&error_debug);

            let (events, diagnostic_events) = build_diagnostic_events(&host);

            let categorized_events = match host.get_events() {
                Ok(evs) => categorize_events(&evs),
                Err(_) => vec![],
            };

            // Heuristic to find the actual user panic point
            let mut user_panic_point = None;
            for event in &diagnostic_events {
                let mut combined_text = event.data.clone();
                for topic in &event.topics {
                    combined_text.push(' ');
                    combined_text.push_str(topic);
                }

                if combined_text.contains("panicked")
                    || combined_text.contains("Error")
                    || combined_text.contains("Trap")
                {
                    if combined_text.contains("core/src/panicking.rs")
                        || combined_text.contains("core::panicking")
                        || combined_text.contains("rust_begin_unwind")
                        || combined_text.contains("std::rt::lang_start")
                        || combined_text.contains("compiler_builtins")
                        || combined_text.contains("rustc_std_workspace")
                    {
                        continue;
                    }

                    if combined_text.contains(".rs") && !combined_text.contains("soroban-env-host")
                    {
                        user_panic_point = Some(combined_text.replace('"', ""));
                        break;
                    }
                }
            }

            let details = if let Some(ref point) = user_panic_point {
                format!(
                    "Contract execution failed with host error: {:?}. Panic point: {}",
                    host_error, point
                )
            } else {
                format!(
                    "Contract execution failed with host error: {:?}",
                    host_error
                )
            };

            let structured_error = StructuredError {
                error_type: "HostError".to_string(),
                message: error_debug.clone(),
                details: Some(details),
            };

            let wasm_offset = extract_wasm_offset(&error_debug);

            let response = SimulationResponse {
                status: "error".to_string(),
                error: Some(serde_json::to_string(&structured_error).unwrap()),
                events,
                diagnostic_events,
                categorized_events,
                logs: vec![format!("Stack trace:\n{}", wasm_trace.display())],
                flamegraph: None,
                optimization_report: None,
                budget_usage: None,
                source_location: None,
                stack_trace: Some(wasm_trace),
                wasm_offset,
            };
            println!("{}", serde_json::to_string(&response).unwrap());
        }
        Err(panic_info) => {
            let panic_msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };

            let wasm_trace = WasmStackTrace::from_panic(&panic_msg);

            let response = SimulationResponse {
                status: "error".to_string(),
                error: Some(format!("Simulator panicked: {panic_msg}")),
                events: vec![],
                diagnostic_events: vec![],
                categorized_events: vec![],
                logs: vec![format!("PANIC: {}", panic_msg)],
                flamegraph: None,
                optimization_report: None,
                budget_usage: None,
                source_location: None,
                stack_trace: Some(wasm_trace),
                wasm_offset: None,
            };
            println!("{}", serde_json::to_string(&response).unwrap());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_wasm_instruction() {
        let topics = vec!["budget".to_string(), "tick".to_string()];
        let data = "\"Instruction: i32.add\"".to_string();
        let instr = extract_wasm_instruction(&topics, &data);
        assert_eq!(instr, Some("i32.add".to_string()));

        let data2 = "\"Instruction: call 12\"".to_string();
        let instr2 = extract_wasm_instruction(&topics, &data2);
        assert_eq!(instr2, Some("call 12".to_string()));

        let topics_none = vec!["other".to_string()];
        let instr3 = extract_wasm_instruction(&topics_none, &data);
        assert_eq!(instr3, None);
    }

    #[test]
    fn test_decode_vm_traps() {
        let msg = decode_error("Error: Wasm Trap: out of bounds memory access");
        assert!(msg.contains("Out of bounds"));
    }

    #[test]
    fn test_decode_unreachable() {
        let msg = decode_error("wasm trap: unreachable");
        assert!(msg.contains("Unreachable"));
    }

    #[test]
    fn test_in_successful_contract_call_is_negation_of_failed_call() {
        use soroban_env_host::events::{Events, HostEvent};
        use soroban_env_host::xdr::{
            ContractEvent, ContractEventBody, ContractEventType, ContractEventV0, ExtensionPoint,
            VecM,
        };

        let make_event = |failed: bool| -> HostEvent {
            HostEvent {
                failed_call: failed,
                event: ContractEvent {
                    ext: ExtensionPoint::V0,
                    contract_id: None,
                    type_: ContractEventType::Diagnostic,
                    body: ContractEventBody::V0(ContractEventV0 {
                        topics: VecM::default(),
                        data: soroban_env_host::xdr::ScVal::Void,
                    }),
                },
            }
        };

        let evs_failed = Events(vec![make_event(true)]);
        let categorized = categorize_events(&evs_failed);
        assert_eq!(categorized.len(), 1);
        assert!(
            !categorized[0].event.in_successful_contract_call,
            "a failed call should NOT be marked as a successful contract call"
        );

        let evs_ok = Events(vec![make_event(false)]);
        let categorized = categorize_events(&evs_ok);
        assert_eq!(categorized.len(), 1);
        assert!(
            categorized[0].event.in_successful_contract_call,
            "a successful call MUST be marked as a successful contract call"
        );
    }

    #[test]
    fn test_categorize_events_type_labels() {
        use soroban_env_host::events::{Events, HostEvent};
        use soroban_env_host::xdr::{
            ContractEvent, ContractEventBody, ContractEventType, ContractEventV0, ExtensionPoint,
            VecM,
        };

        let make_typed_event = |t: ContractEventType| HostEvent {
            failed_call: false,
            event: ContractEvent {
                ext: ExtensionPoint::V0,
                contract_id: None,
                type_: t,
                body: ContractEventBody::V0(ContractEventV0 {
                    topics: VecM::default(),
                    data: soroban_env_host::xdr::ScVal::Void,
                }),
            },
        };

        let evs = Events(vec![
            make_typed_event(ContractEventType::Contract),
            make_typed_event(ContractEventType::System),
            make_typed_event(ContractEventType::Diagnostic),
        ]);

        let cats = categorize_events(&evs);
        assert_eq!(cats[0].category, "Contract");
        assert_eq!(cats[1].category, "System");
        assert_eq!(cats[2].category, "Diagnostic");

        assert_eq!(cats[0].event.event_type, "contract");
        assert_eq!(cats[1].event.event_type, "system");
        assert_eq!(cats[2].event.event_type, "diagnostic");
    }

    #[test]
    fn test_source_mapper_no_symbols_gives_no_location() {
        use crate::source_mapper::SourceMapper;

        let wasm_bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        let mapper = SourceMapper::new(wasm_bytes);
        assert!(!mapper.has_debug_symbols());
        assert!(
            mapper.map_wasm_offset_to_source(0).is_none(),
            "WASM without .debug_info should yield no source location"
        );
    }
}
