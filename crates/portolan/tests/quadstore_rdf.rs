// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The RDF projection's terms intern and index in a quadstore. Moved from
//! chartulary's `quads/tests.rs` when quads became its own crate (portolan D7).

use portolan::rdf::{self, Term};
use portolan::{Container, Graph, Recognized, Relation, RelationClass};
use quadstore::{Dictionary, Quad, QuadIndex, QuadMeta, View};

#[test]
fn the_rdf_projection_interns_and_indexes() {
    let mut graph = Graph::<Container, Relation>::new();
    let a = graph.insert(
        Container::new("a")
            .with_address("https://a.example/")
            .with_title("Paper A")
            .with_tag("research"),
    );
    let b = graph.insert(Container::new("b").with_address("https://b.example/"));
    let c = graph.insert(Container::new("c").with_title("C"));
    graph.connect(
        a,
        b,
        Relation::new(RelationClass::recognized(Recognized::Cites)),
    );
    graph.connect(
        c,
        a,
        Relation::new(RelationClass::recognized(Recognized::Cites)),
    );
    graph.connect(a, c, Relation::new(RelationClass::app("woodshed", 0)));

    let projected = rdf::to_quads(&graph);
    let mut dictionary = Dictionary::new();
    let rows: Vec<(Quad, QuadMeta)> = projected
        .iter()
        .map(|quad| {
            let subject = dictionary.intern(Term::Iri(quad.subject.clone()));
            let predicate = dictionary.intern(Term::Iri(quad.predicate.clone()));
            let object = dictionary.intern(quad.object.clone());
            (
                Quad::in_default_graph(subject, predicate, object),
                QuadMeta::HELD,
            )
        })
        .collect();
    let index = QuadIndex::build(rows);
    assert_eq!(index.len(), projected.len());

    let cites = dictionary
        .lookup(&Term::Iri("urn:chart:rel:cites".into()))
        .expect("cites");
    let mut cited: Vec<&Term> = index
        .pattern(None, Some(cites), None, Some(None), View::Live)
        .map(|(quad, _)| dictionary.resolve(quad.object))
        .collect();
    cited.sort_by_key(|term| format!("{term:?}"));
    assert_eq!(
        cited,
        [
            &Term::Iri("https://a.example/".into()),
            &Term::Iri("https://b.example/".into())
        ]
    );

    let paper_a = dictionary
        .lookup(&Term::Iri("https://a.example/".into()))
        .expect("a");
    assert_eq!(
        index
            .pattern(Some(paper_a), None, None, None, View::Live)
            .count(),
        3
    );
    let c_term = Term::Iri("urn:chart:c".into());
    let c_id = dictionary.lookup(&c_term).expect("c cites a");
    assert_eq!(
        index
            .pattern(None, None, Some(c_id), None, View::History)
            .count(),
        0
    );
    assert_eq!(index.graph_names(View::History).count(), 0);
}
