// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[cfg(feature = "bert")]
const MINILM_ARTIFACTS: &[(&str, &str)] = &[
    (
        "config.json",
        "953f9c0d463486b10a6871cc2fd59f223b2c70184f49815e7efbcab5d8908b41",
    ),
    (
        "tokenizer.json",
        "be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037",
    ),
    (
        "model.safetensors",
        "53aa51172d142c89d9012cce15ae4d6cc0ca6895895114379cacb4fab128d9db",
    ),
];

#[cfg(feature = "bert")]
fn minilm_dir() -> PathBuf {
    std::env::var_os("ESP_MINILM_DIR")
        .map(PathBuf::from)
        .expect("ESP_MINILM_DIR must point at the fixed all-MiniLM-L6-v2 artifact")
}

#[cfg(feature = "bert")]
fn sha256(path: &Path) -> String {
    let mut file = std::fs::File::open(path).unwrap_or_else(|error| {
        panic!("open model artifact {}: {error}", path.display());
    });
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let bytes_read = file.read(&mut buffer).unwrap_or_else(|error| {
            panic!("hash model artifact {}: {error}", path.display());
        });
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut hex, "{byte:02x}").unwrap();
    }
    hex
}

#[cfg(feature = "bert")]
fn verify_minilm_artifacts(model_dir: &Path) -> u64 {
    let mut total_bytes = 0;
    for (name, expected_hash) in MINILM_ARTIFACTS {
        let path = model_dir.join(name);
        let metadata = std::fs::metadata(&path).unwrap_or_else(|error| {
            panic!("stat model artifact {}: {error}", path.display());
        });
        let actual_hash = sha256(&path);
        assert_eq!(
            actual_hash,
            *expected_hash,
            "model artifact digest drifted: {}",
            path.display()
        );
        total_bytes += metadata.len();
    }
    total_bytes
}

#[cfg(feature = "bert")]
fn build_minilm_search(model_dir: &Path) -> (DenseSearch, u128, u128) {
    let load_started = Instant::now();
    let provider = esp::embed::bert::load_cpu(model_dir).unwrap_or_else(|error| {
        panic!("load MiniLM from {}: {error}", model_dir.display());
    });
    let load_ms = load_started.elapsed().as_millis();
    assert_eq!(provider.dimensions(), 384);
    let mut search = SemanticSearch::new(provider);
    let ingest_started = Instant::now();
    search.ingest_batch(DOCUMENTS).unwrap();
    let ingest_ms = ingest_started.elapsed().as_millis();
    (search, load_ms, ingest_ms)
}

/// Explicit learned-vector baseline for the lexical forcing fixture.
///
/// This stays ignored because the checkpoint is a 90 MB local artifact rather
/// than a repository or CI fixture. Run on the portable CPU backend:
///
/// `ESP_MINILM_DIR=/path/to/all-MiniLM-L6-v2 python3 /Users/markik/Code/tools/build_storage.py build --lane eidetic-search --cwd . -- test -p eidetic-search --features bert --test lexical_ngram_recall -- learned_minilm_baseline --ignored --nocapture --test-threads=1`
#[cfg(feature = "bert")]
#[test]
#[ignore = "requires the digest-pinned all-MiniLM-L6-v2 artifact"]
fn learned_minilm_baseline() {
    let model_dir = minilm_dir();
    let artifact_bytes = verify_minilm_artifacts(&model_dir);

    let (dense, load_ms, ingest_ms) = build_minilm_search(&model_dir);

    let directory = tempfile::tempdir().unwrap();
    let trace = browsing_trace();
    let trail = TrailIndex::rebuild(directory.path().join("trail"), [&trace]).unwrap();

    let query_started = Instant::now();
    let (dense_tally, dense_ranks) = evaluate_ranker(|case| vector_ranking(&dense, case.query));
    let dense_query_ms = query_started.elapsed().as_millis();
    let (equal_fusion, equal_fusion_ranks) = evaluate_fusion(&trail, &dense, (1.0, 1.0));
    let (vector_heavy_fusion, vector_heavy_fusion_ranks) =
        evaluate_fusion(&trail, &dense, (1.0, 2.0));

    let vector_index_bytes = dense.len() * dense.provider().dimensions() * size_of::<f32>();
    println!(
        "MiniLM receipt: backend=cpu, dimensions={}, documents={}, held_out_cases={}, artifact_bytes={artifact_bytes}, vector_index_bytes={vector_index_bytes}, load_ms={load_ms}, ingest_ms={ingest_ms}, dense_query_sweep_ms={dense_query_ms}",
        dense.provider().dimensions(),
        DOCUMENTS.len(),
        EVAL_CASES.len(),
    );
    println!("dense={dense_tally:?} ranks={dense_ranks:?}");
    println!("bm25+dense (1,1)={equal_fusion:?} ranks={equal_fusion_ranks:?}");
    println!("bm25+dense (1,2)={vector_heavy_fusion:?} ranks={vector_heavy_fusion_ranks:?}");

    // The fixed artifact makes these rankings a reproducible comparison, not
    // a machine-local smoke test.
    let dense_expected = RankingTally {
        at_1: 12,
        at_3: 15,
        phrase_at_1: 7,
        controls_at_1: 5,
    };
    assert_eq!(
        dense_tally, dense_expected,
        "MiniLM ranking drifted: {dense_ranks:?}"
    );
    assert_eq!(
        equal_fusion,
        FusionTally {
            deterministic: dense_expected,
            unique_at_1: 7,
            phrase_unique_at_1: 2,
            controls_unique_at_1: 5,
            expected_top_ties: 5,
        },
        "equal-weight BM25 + MiniLM drifted: {equal_fusion_ranks:?}"
    );
    assert_eq!(
        vector_heavy_fusion,
        FusionTally {
            deterministic: dense_expected,
            unique_at_1: 12,
            phrase_unique_at_1: 7,
            controls_unique_at_1: 5,
            expected_top_ties: 0,
        },
        "dense-heavy BM25 + MiniLM drifted: {vector_heavy_fusion_ranks:?}"
    );
}

fn median(samples: &mut [u128]) -> u128 {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

/// Explicit cost harness. Run optimized and single-threaded:
///
/// `python3 /Users/markik/Code/tools/build_storage.py build --lane eidetic-search --cwd . -- test -p eidetic-search --release --test lexical_ngram_recall -- --ignored --nocapture --test-threads=1`
#[test]
#[ignore = "measurement harness; run explicitly in release mode"]
fn measure_lexical_ngram_costs() {
    const SAMPLE_COUNT: usize = 11;
    const BUILD_REPEATS: usize = 50;
    const QUERY_SWEEPS: usize = 10;

    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    println!(
        "lexical n-gram receipt: profile={profile}, os={}, arch={}, dimensions={DIMENSIONS}, documents={}, held_out_cases={}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        DOCUMENTS.len(),
        EVAL_CASES.len()
    );
    println!(
        "orders,ranking_at_1,phrase_ranking_at_1,controls_ranking_at_1,median_build_ns_per_document,median_query_ns,index_dense_vector_bytes,index_json_bytes,nonzero_slots"
    );

    let configs = [
        ("1", &[1][..]),
        ("1+2", &[1, 2][..]),
        ("1+2+3", &[1, 2, 3][..]),
    ];
    let searches: Vec<Search> = configs
        .iter()
        .map(|(_, orders)| build_search(orders))
        .collect();

    // Warm every configuration before measuring, then rotate the order in each
    // sample so CPU ramp-up cannot systematically favour the later settings.
    for (_, orders) in configs {
        for _ in 0..10 {
            black_box(build_search(black_box(orders)));
        }
    }
    for search in &searches {
        for _ in 0..2 {
            for case in EVAL_CASES {
                black_box(search.search(black_box(case.query), 5).unwrap());
            }
        }
    }

    let mut build_samples: [Vec<u128>; 3] = std::array::from_fn(|_| Vec::new());
    let mut query_samples: [Vec<u128>; 3] = std::array::from_fn(|_| Vec::new());
    for sample in 0..SAMPLE_COUNT {
        for position in 0..configs.len() {
            let index = (sample + position) % configs.len();
            let orders = configs[index].1;
            let start = Instant::now();
            for _ in 0..BUILD_REPEATS {
                black_box(build_search(black_box(orders)));
            }
            build_samples[index]
                .push(start.elapsed().as_nanos() / (BUILD_REPEATS * DOCUMENTS.len()) as u128);
        }
        for position in 0..configs.len() {
            let index = (sample * 2 + position) % configs.len();
            let search = &searches[index];
            let start = Instant::now();
            for _ in 0..QUERY_SWEEPS {
                for case in EVAL_CASES {
                    black_box(search.search(black_box(case.query), 5).unwrap());
                }
            }
            query_samples[index]
                .push(start.elapsed().as_nanos() / (QUERY_SWEEPS * EVAL_CASES.len()) as u128);
        }
    }

    for (index, (label, _)) in configs.into_iter().enumerate() {
        let search = &searches[index];
        let (tally, _) = evaluate(search);
        let dense_vector_bytes =
            search.index().len() * search.provider().dimensions() * size_of::<f32>();
        let index_json_bytes = serde_json::to_vec(search.index()).unwrap().len();
        let nonzero_slots: usize = search
            .index()
            .iter()
            .map(|(_, vector)| vector.iter().filter(|&&value| value != 0.0).count())
            .sum();
        let build_ns = median(&mut build_samples[index]);
        let query_ns = median(&mut query_samples[index]);

        println!(
            "{label},{}/{},{}/{},{}/{},{build_ns},{query_ns},{dense_vector_bytes},{index_json_bytes},{nonzero_slots}",
            tally.at_1,
            EVAL_CASES.len(),
            tally.phrase_at_1,
            EVAL_CASES.len() - 5,
            tally.controls_at_1,
            5,
        );
    }
}
