// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! First real-model run of the goal model path (P4 S3): the ESP adapter over
//! the pinned all-MiniLM-L6-v2 checkpoint, and a calibration of
//! `match_similarity_milli` against the frozen corpus in
//! `tests/fixtures/goal_calibration/corpus.txt`.
//!
//! Ignored: the checkpoint is a 90 MB local artifact. Run on the CPU backend:
//!
//! `ESP_MINILM_DIR=/path/to/all-MiniLM-L6-v2 cargo test -p eidetic-search --features bert --test goal_model_minilm -- --ignored --nocapture`
#![cfg(feature = "bert")]

use eidetic::models::recall::{RecallSourceId, RecallSourceKind, RetainedSourceText};
use eidetic_search::goal_model_esp::{
    EspGoalEmbedder, MINILM_L6_V2_MATCH_SIMILARITY_MILLI, recommended_match_similarity_milli,
};
use eidetic_search::{
    EmbedRole, GoalEmbedder, GoalSettings, ModelAssessRule, ModelGoalAssessor, ModelGoalSettings,
};
use esp::embed::EmbeddingProvider;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Instant;

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

const CORPUS: &str = include_str!("fixtures/goal_calibration/corpus.txt");

fn minilm_dir() -> PathBuf {
    std::env::var_os("ESP_MINILM_DIR")
        .map(PathBuf::from)
        .expect("ESP_MINILM_DIR must point at the fixed all-MiniLM-L6-v2 artifact")
}

fn sha256(path: &Path) -> String {
    let mut file = std::fs::File::open(path)
        .unwrap_or_else(|error| panic!("open model artifact {}: {error}", path.display()));
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut buffer)
            .unwrap_or_else(|error| panic!("hash model artifact {}: {error}", path.display()));
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    let mut hex = String::new();
    for byte in hasher.finalize() {
        write!(&mut hex, "{byte:02x}").unwrap();
    }
    hex
}

fn verify_minilm_artifacts(dir: &Path) {
    for (name, expected) in MINILM_ARTIFACTS {
        let path = dir.join(name);
        assert_eq!(
            sha256(&path),
            *expected,
            "model artifact digest drifted: {}",
            path.display()
        );
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Lexical,
    Paraphrase,
    NearMiss,
    Unrelated,
}

struct Case {
    goal: usize,
    id: String,
    relevant: bool,
    kind: Kind,
    source: RetainedSourceText,
}

fn parse_corpus() -> (Vec<String>, Vec<Case>) {
    let (mut goals, mut cases) = (Vec::new(), Vec::new());
    for line in CORPUS
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        if let Some(rest) = line.strip_prefix("@goal ") {
            goals.push(rest.split_once(' ').expect("goal text").1.to_owned());
            continue;
        }
        let f: Vec<&str> = line.splitn(5, '|').collect();
        assert_eq!(f.len(), 5, "bad corpus line: {line}");
        let kind = match f[1] {
            "lexical" => Kind::Lexical,
            "paraphrase" => Kind::Paraphrase,
            "nearmiss" => Kind::NearMiss,
            "unrelated" => Kind::Unrelated,
            other => panic!("unknown kind {other}"),
        };
        cases.push(Case {
            goal: goals.len() - 1,
            id: f[2].into(),
            relevant: f[0] == "R",
            kind,
            source: RetainedSourceText {
                source: RecallSourceId {
                    kind: RecallSourceKind::Fleece,
                    owner: Some("calibration".into()),
                    source: f[2].into(),
                    node: None,
                },
                version: format!("{}@v1", f[2]),
                title: Some(f[3].into()),
                text: f[4].into(),
            },
        });
    }
    (goals, cases)
}

fn load_embedder() -> (EspGoalEmbedder<impl EmbeddingProvider>, u128) {
    let dir = minilm_dir();
    verify_minilm_artifacts(&dir);
    let started = Instant::now();
    let provider = esp::embed::bert::load_cpu(&dir)
        .unwrap_or_else(|error| panic!("load MiniLM from {}: {error}", dir.display()));
    let load_ms = started.elapsed().as_millis();
    (EspGoalEmbedder::new(provider).expect("adapter"), load_ms)
}

#[test]
#[ignore = "needs the local all-MiniLM-L6-v2 checkpoint (ESP_MINILM_DIR)"]
fn adapter_end_to_end() {
    let (mut embedder, load_ms) = load_embedder();
    let envelope = embedder.envelope();
    envelope.validate().expect("envelope valid");
    envelope
        .ensure_compatible(&embedder.envelope())
        .expect("self-compatible");
    let space = envelope.space.clone();
    assert_eq!(space.dimensions, 384);
    println!("artifacts: {:?}", space.producer.artifacts);
    println!(
        "space: producer={} rev={} pooling={} norm={} qprefix={:?} dprefix={:?}",
        space.producer.name,
        space.producer.revision,
        space.pooling,
        space.normalization,
        space.query_prefix,
        space.document_prefix
    );
    let texts = ["tides and the moon", "a recipe for lasagne"];
    let started = Instant::now();
    let q = embedder.embed(&texts, EmbedRole::Query).unwrap();
    let d = embedder.embed(&texts, EmbedRole::Document).unwrap();
    println!(
        "load {load_ms} ms; embed 2+2 texts {} ms",
        started.elapsed().as_millis()
    );
    for v in q.iter().chain(&d) {
        assert_eq!(v.len(), space.dimensions);
        assert!(v.iter().all(|x| x.is_finite()) && v.iter().any(|x| *x != 0.0));
    }
    // Prefixes are applied exactly as the space declares: with an empty
    // prefix both roles agree; otherwise the roles must differ per prefix.
    if space.query_prefix == space.document_prefix {
        assert_eq!(q, d, "same prefix, same vectors");
    } else {
        assert_ne!(q, d, "different prefixes must change the vectors");
    }
    // Determinism of the adapter itself.
    assert_eq!(q, embedder.embed(&texts, EmbedRole::Query).unwrap());
}

/// Lends one loaded embedder to several assessors.
struct Shared<'a, E: GoalEmbedder>(&'a mut E);

impl<E: GoalEmbedder> GoalEmbedder for Shared<'_, E> {
    fn envelope(&self) -> eidetic::models::embedding::EmbeddingEnvelopeV1 {
        self.0.envelope()
    }
    fn embed(&mut self, texts: &[&str], role: EmbedRole) -> Result<Vec<Vec<f32>>, String> {
        self.0.embed(texts, role)
    }
}

/// One scored (goal, source) pair.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Row {
    id: String,
    kind: Kind,
    relevant: bool,
    coverage: u16,
    s1_match: bool,
    sim: i16,
}

fn score_all<E: GoalEmbedder>(embedder: &mut E, goals: &[String], cases: &[Case]) -> Vec<Row> {
    let mut rows = Vec::new();
    for (g, goal) in goals.iter().enumerate() {
        // RecordOnly: S1 decides, the model's similarity is recorded.
        let settings = ModelGoalSettings {
            assess_rule: ModelAssessRule::RecordOnly,
            ..ModelGoalSettings::default()
        };
        let mut a = ModelGoalAssessor::new(
            goal,
            GoalSettings::default(),
            settings,
            Shared(&mut *embedder),
        )
        .unwrap();
        for c in cases.iter().filter(|c| c.goal == g) {
            a.assess(&c.source).unwrap();
        }
        let rec = a.record();
        let mut it = rec.assessments.iter();
        for c in cases.iter().filter(|c| c.goal == g) {
            let m = it.next().unwrap();
            assert_eq!(m.status, eidetic_search::ModelStatus::Used);
            rows.push(Row {
                id: c.id.clone(),
                kind: c.kind,
                relevant: c.relevant,
                coverage: m.s1_coverage_permille,
                s1_match: m.s1_match,
                sim: m.similarity_milli.unwrap(),
            });
        }
    }
    rows
}

#[derive(Clone, Copy, Debug)]
struct Pr {
    tp: usize,
    fp: usize,
    fneg: usize,
}

impl Pr {
    fn precision(self) -> f64 {
        if self.tp + self.fp == 0 {
            1.0
        } else {
            self.tp as f64 / (self.tp + self.fp) as f64
        }
    }
    fn recall(self) -> f64 {
        self.tp as f64 / (self.tp + self.fneg) as f64
    }
    fn f1(self) -> f64 {
        let (p, r) = (self.precision(), self.recall());
        if p + r == 0.0 {
            0.0
        } else {
            2.0 * p * r / (p + r)
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Rule {
    S1Only,
    Or,
    ModelReplaces,
}

fn predict(r: &Row, rule: Rule, t: i16) -> bool {
    match rule {
        Rule::S1Only => r.s1_match,
        Rule::Or => r.s1_match || r.sim >= t,
        Rule::ModelReplaces => r.sim >= t,
    }
}

fn pr(rows: &[&Row], rule: Rule, t: i16) -> Pr {
    let mut p = Pr {
        tp: 0,
        fp: 0,
        fneg: 0,
    };
    for r in rows {
        match (predict(r, rule, t), r.relevant) {
            (true, true) => p.tp += 1,
            (true, false) => p.fp += 1,
            (false, true) => p.fneg += 1,
            _ => {}
        }
    }
    p
}

fn summary(label: &str, mut v: Vec<i16>) {
    v.sort_unstable();
    if v.is_empty() {
        return;
    }
    let q = |f: f64| v[((v.len() - 1) as f64 * f).round() as usize];
    println!(
        "  {label:<22} n={:<3} min={:<4} p25={:<4} med={:<4} p75={:<4} max={}",
        v.len(),
        v[0],
        q(0.25),
        q(0.5),
        q(0.75),
        v[v.len() - 1]
    );
}

/// Calibrated `match_similarity_milli` for all-MiniLM-L6-v2 under the `Or`
/// rule (see the criterion below); asserted against the sweep so a corpus or
/// model change cannot silently invalidate it.
const CALIBRATED_MILLI: i16 = 220;
/// Required distance, in thousandths, between the threshold and the nearest
/// labelled score on the far side that it must separate.
const MARGIN_MILLI: i16 = 75;

#[test]
#[ignore = "needs the local all-MiniLM-L6-v2 checkpoint (ESP_MINILM_DIR)"]
#[allow(clippy::too_many_lines)] // one linear report
fn calibrate_match_similarity() {
    let (goals, cases) = parse_corpus();
    let (mut embedder, load_ms) = load_embedder();
    let started = Instant::now();
    let rows = score_all(&mut embedder, &goals, &cases);
    let embed_ms = started.elapsed().as_millis();
    let again = score_all(&mut embedder, &goals, &cases);
    assert_eq!(rows, again, "scores must be identical across runs");
    println!(
        "\nload {load_ms} ms; {} pairs scored in {embed_ms} ms",
        rows.len()
    );

    println!("\nper source (coverage permille / S1 match / sim milli):");
    for r in &rows {
        println!(
            "  {:<6} {:<10} {} cov={:<4} s1={:<5} sim={}",
            r.id,
            format!("{:?}", r.kind),
            if r.relevant { "R" } else { "I" },
            r.coverage,
            r.s1_match,
            r.sim
        );
    }

    println!("\nsimilarity distributions (milli):");
    for (name, f) in [
        ("relevant/lexical", (true, Kind::Lexical)),
        ("relevant/paraphrase", (true, Kind::Paraphrase)),
        ("irrelevant/nearmiss", (false, Kind::NearMiss)),
        ("irrelevant/unrelated", (false, Kind::Unrelated)),
    ] {
        summary(
            name,
            rows.iter()
                .filter(|r| (r.relevant, r.kind) == f)
                .map(|r| r.sim)
                .collect(),
        );
    }

    let all: Vec<&Row> = rows.iter().collect();
    // Criterion: the threshold maximising F1 of the `Or` rule over a 10-milli
    // sweep; among ties, the middle of the tied thresholds.
    let sweep: Vec<i16> = (0..=900).step_by(10).collect();
    println!("\nsweep (t: Or P/R/F1 | ModelReplaces P/R/F1):");
    for &t in &sweep {
        let (o, m) = (pr(&all, Rule::Or, t), pr(&all, Rule::ModelReplaces, t));
        println!(
            "  {t:>3}: {:.3}/{:.3}/{:.3} | {:.3}/{:.3}/{:.3}",
            o.precision(),
            o.recall(),
            o.f1(),
            m.precision(),
            m.recall(),
            m.f1()
        );
    }
    let best = sweep
        .iter()
        .map(|&t| pr(&all, Rule::Or, t).f1())
        .fold(0.0, f64::max);
    let tied: Vec<i16> = sweep
        .iter()
        .copied()
        .filter(|&t| (pr(&all, Rule::Or, t).f1() - best).abs() < 1e-9)
        .collect();
    let chosen = tied[tied.len() / 2];
    println!("\ncriterion: max F1 of Or; best F1 {best:.3} at {tied:?}; chosen {chosen}");
    let mr_best = sweep
        .iter()
        .copied()
        .max_by(|&a, &b| {
            pr(&all, Rule::ModelReplaces, a)
                .f1()
                .total_cmp(&pr(&all, Rule::ModelReplaces, b).f1())
                .then(b.cmp(&a))
        })
        .unwrap();
    println!("ModelReplaces own best-F1 threshold: {mr_best}");

    println!("\nat t={chosen} (rule: precision / recall / F1, tp fp fn):");
    for (name, rule) in [
        ("S1Only", Rule::S1Only),
        ("Or", Rule::Or),
        ("ModelReplaces", Rule::ModelReplaces),
    ] {
        let p = pr(&all, rule, chosen);
        println!(
            "  {name:<14} {:.3} / {:.3} / {:.3}  {} {} {}",
            p.precision(),
            p.recall(),
            p.f1(),
            p.tp,
            p.fp,
            p.fneg
        );
    }
    let at500 = pr(&all, Rule::Or, 500);
    println!(
        "  (generic default 500, Or: {:.3} / {:.3} / {:.3})",
        at500.precision(),
        at500.recall(),
        at500.f1()
    );
    let para: Vec<&Row> = rows.iter().filter(|r| r.kind == Kind::Paraphrase).collect();
    for (name, rule) in [
        ("S1Only", Rule::S1Only),
        ("Or", Rule::Or),
        ("ModelReplaces", Rule::ModelReplaces),
    ] {
        let p = pr(&para, rule, chosen);
        println!(
            "  paraphrase recall {name:<14} {:.3} ({}/{})",
            p.recall(),
            p.tp,
            p.tp + p.fneg
        );
    }

    // Robust properties.
    let (s1, or) = (pr(&all, Rule::S1Only, chosen), pr(&all, Rule::Or, chosen));
    assert!(or.recall() >= s1.recall(), "Or can only add matches");
    let para_or = pr(&para, Rule::Or, chosen);
    let para_s1 = pr(&para, Rule::S1Only, chosen);
    assert!(
        para_or.recall() > para_s1.recall(),
        "the model finds paraphrases S1 misses"
    );
    assert!(or.f1() >= s1.f1(), "Or does not lose F1 against S1 alone");
    assert_eq!(
        chosen, CALIBRATED_MILLI,
        "calibration drifted; re-read the sweep"
    );
    assert_eq!(CALIBRATED_MILLI, MINILM_L6_V2_MATCH_SIMILARITY_MILLI);
    assert_eq!(
        recommended_match_similarity_milli(&embedder.envelope().space),
        Some(CALIBRATED_MILLI),
        "the pinned weights are recognised"
    );
    // Every unrelated source below the threshold and every paraphrase above
    // it, each by MARGIN_MILLI.
    let max_unrelated = rows
        .iter()
        .filter(|r| r.kind == Kind::Unrelated)
        .map(|r| r.sim)
        .max()
        .unwrap();
    let min_para = para.iter().map(|r| r.sim).min().unwrap();
    assert!(
        max_unrelated + MARGIN_MILLI <= chosen,
        "unrelated {max_unrelated}"
    );
    assert!(chosen + MARGIN_MILLI <= min_para, "paraphrase {min_para}");
    // The generic default (500) misses most paraphrases for this model.
    assert!(pr(&para, Rule::Or, 500).recall() < 0.5);
}
