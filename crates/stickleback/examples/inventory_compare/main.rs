// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Finite host policy comparison. No sockets, RF, domain admission or FEC.

mod model;

use serde_json::json;
use std::fs::OpenOptions;
use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 2 {
        return Err("usage: inventory_compare NEW_REPORT.json".into());
    }
    let mut results = Vec::new();
    for scenario in model::SCENARIOS {
        for seed in [1, 2, 3, 5, 8] {
            for mode in model::MODES {
                results.push(model::run(scenario, mode, seed)?);
            }
        }
    }
    let mut source = blake3::Hasher::new();
    for bytes in [
        include_str!("../../src/inventory/mod.rs"),
        include_str!("../../src/inventory/filter.rs"),
        include_str!("../../src/inventory/wire.rs"),
        include_str!("../../src/inventory/peers.rs"),
        include_str!("main.rs"),
        include_str!("model.rs"),
    ] {
        source.update(&(bytes.len() as u64).to_le_bytes());
        source.update(bytes.as_bytes());
    }
    let report = json!({
        "schema": "mere.inventory-policy-comparison/v1",
        "compiled_source_blake3": source.finalize().to_hex().to_string(),
        "compiled_lock_blake3": blake3::hash(include_bytes!("../../../../Cargo.lock")).to_hex().to_string(),
        "assumptions": {
            "nodes": 8, "topology": "bidirectional ring", "payload_latency_ticks": 1,
            "control_exchange": "synchronous bounded substeps of each repair round",
            "repair_interval_ticks": 4, "per_direction_round_bytes": 4096,
            "global_receiver_slots_per_round": 2, "per_link_candidates": 8,
            "fresh_repair_schedule": "eager baseline uses two shared slots; inventory modes permit one fresh slot, reserve remaining capacity for repair, then reuse unused capacity for fresh work",
            "queue_entries_per_direction": 64, "inventory_entries": 64,
            "retained_payload_bytes": 1048576, "max_object_bytes": 16384,
            "control_ceiling_bytes": 1024, "summary_wire": "actual MIV1 encoded bytes",
            "offer_and_request_model": "two control payloads, each 48 + 32 * candidate_count bytes, each with four-byte framing",
            "result_model": "reserve a 48-byte advisory result plus four-byte framing per attempted payload; emit on complete digest-verified reception",
            "payload_wire_model": "four-byte object-stream prefix plus unchanged synthetic bytes",
            "fallback": "every fourth Cuckoo round, plus filter saturation",
            "loss": "25 percent per modeled control/payload reception in fault scenarios",
            "loss_sampling": "BLAKE3(seed,tick,source,target,kind,ordinal), low byte < 64",
            "peer_state": "eight caller-authorized slots, idle expiry at 32 ticks",
            "admission": "two modeled reservations per receiver per round; failed attempts consume slots",
            "owner_boundary": "digest-verified synthetic payload reception; domain admission unexercised",
            "excluded": ["RF", "physical time", "QUIC retransmission", "cryptographic authentication",
                "signed p2panda operations", "MeshStore admission", "RaptorQ", "production ingress wiring"],
        }, "results": results,
    });
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])?;
    serde_json::to_writer_pretty(&mut output, &report)?;
    output.write_all(b"\n")?;
    println!(
        "{} finite runs written to {}",
        report["results"].as_array().unwrap().len(),
        args[1]
    );
    Ok(())
}
