// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Directed node pairs: strongly connected components, the pairs on a
//! cycle, and the shortest cycle through a pair.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

/// A directed edge between two nodes.
pub type Pair = (String, String);

fn adjacency(pairs: &BTreeSet<Pair>) -> BTreeMap<&str, BTreeSet<&str>> {
    let mut graph: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for (a, b) in pairs {
        graph.entry(a).or_default().insert(b);
        graph.entry(b).or_default();
    }
    graph
}

fn enter<'a>(node: &'a str, index: &mut HashMap<&'a str, usize>, low: &mut HashMap<&'a str, usize>) {
    let n = index.len();
    index.insert(node, n);
    low.insert(node, n);
}

/// Strongly connected components of more than one node (Tarjan, iterative).
pub fn sccs(pairs: &BTreeSet<Pair>) -> Vec<BTreeSet<String>> {
    let graph = adjacency(pairs);
    let mut index: HashMap<&str, usize> = HashMap::new();
    let mut low: HashMap<&str, usize> = HashMap::new();
    let mut on_stack: BTreeSet<&str> = BTreeSet::new();
    let mut stack: Vec<&str> = Vec::new();
    let mut result = Vec::new();
    for &start in graph.keys() {
        if index.contains_key(start) {
            continue;
        }
        // Each frame: a node and the neighbours it has yet to visit.
        let mut frames: Vec<(&str, Vec<&str>)> = Vec::new();
        let next_of = |node: &str| graph[node].iter().rev().copied().collect::<Vec<&str>>();
        enter(start, &mut index, &mut low);
        stack.push(start);
        on_stack.insert(start);
        frames.push((start, next_of(start)));
        while let Some((node, pending)) = frames.last_mut() {
            let node = *node;
            if let Some(nxt) = pending.pop() {
                if !index.contains_key(nxt) {
                    enter(nxt, &mut index, &mut low);
                    stack.push(nxt);
                    on_stack.insert(nxt);
                    frames.push((nxt, next_of(nxt)));
                } else if on_stack.contains(nxt) {
                    let l = low[node].min(index[nxt]);
                    low.insert(node, l);
                }
                continue;
            }
            frames.pop();
            if let Some((parent, _)) = frames.last() {
                let l = low[*parent].min(low[node]);
                low.insert(*parent, l);
            }
            if low[node] == index[node] {
                let mut component = BTreeSet::new();
                while let Some(member) = stack.pop() {
                    on_stack.remove(member);
                    component.insert(member.to_string());
                    if member == node {
                        break;
                    }
                }
                if component.len() > 1 {
                    result.push(component);
                }
            }
        }
    }
    result
}

/// The pairs whose two ends sit in one strongly connected component.
pub fn cyclic_pairs(pairs: &BTreeSet<Pair>) -> BTreeSet<Pair> {
    let mut comp_of: HashMap<String, usize> = HashMap::new();
    for (number, component) in sccs(pairs).into_iter().enumerate() {
        for member in component {
            comp_of.insert(member, number);
        }
    }
    pairs.iter().filter(|(a, b)| comp_of.get(a).is_some_and(|c| comp_of.get(b) == Some(c))).cloned().collect()
}

/// The shortest cycle through each of the `through` pairs (default: all),
/// rotated to start at its least node, deduplicated, shortest first, at most
/// `limit`. Each cycle ends where it starts.
pub fn witnesses(pairs: &BTreeSet<Pair>, through: Option<&BTreeSet<Pair>>, limit: usize) -> Vec<Vec<String>> {
    let graph = adjacency(pairs);
    let mut found: BTreeSet<Vec<String>> = BTreeSet::new();
    for (a, b) in pairs.iter().filter(|p| through.is_none_or(|t| t.contains(*p))) {
        let mut previous: HashMap<&str, Option<&str>> = HashMap::from([(b.as_str(), None)]);
        let mut queue: VecDeque<&str> = VecDeque::from([b.as_str()]);
        while !previous.contains_key(a.as_str()) {
            let Some(node) = queue.pop_front() else { break };
            for &nxt in graph.get(node).into_iter().flatten() {
                if !previous.contains_key(nxt) {
                    previous.insert(nxt, Some(node));
                    queue.push_back(nxt);
                }
            }
        }
        if !previous.contains_key(a.as_str()) {
            continue;
        }
        let mut route = Vec::new();
        let mut node = Some(a.as_str());
        while let Some(n) = node {
            route.push(n);
            node = previous[n];
        }
        route.reverse(); // b ... a
        let mut body: Vec<&str> = vec![a.as_str()];
        body.extend(&route[..route.len() - 1]);
        let turn = body.iter().enumerate().min_by_key(|(_, n)| **n).map_or(0, |(i, _)| i);
        body.rotate_left(turn);
        let mut cycle: Vec<String> = body.iter().map(|n| (*n).to_string()).collect();
        cycle.push(cycle[0].clone());
        found.insert(cycle);
    }
    let mut cycles: Vec<Vec<String>> = found.into_iter().collect();
    cycles.sort_by(|x, y| x.len().cmp(&y.len()).then_with(|| x.cmp(y)));
    cycles.truncate(limit);
    cycles
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(list: &[(&str, &str)]) -> BTreeSet<Pair> {
        list.iter().map(|(a, b)| ((*a).to_string(), (*b).to_string())).collect()
    }

    #[test]
    fn a_known_cycle_is_found() {
        let p = pairs(&[("a", "b"), ("b", "c"), ("c", "a"), ("c", "d")]);
        assert_eq!(cyclic_pairs(&p), pairs(&[("a", "b"), ("b", "c"), ("c", "a")]));
        assert_eq!(witnesses(&p, None, 8), vec![vec!["a", "b", "c", "a"]]);
    }

    #[test]
    fn an_acyclic_graph_has_none() {
        let p = pairs(&[("a", "b"), ("b", "c"), ("a", "c")]);
        assert!(sccs(&p).is_empty());
        assert!(cyclic_pairs(&p).is_empty());
        assert!(witnesses(&p, None, 8).is_empty());
    }

    #[test]
    fn shortest_cycles_come_first_and_through_filters() {
        let p = pairs(&[("a", "b"), ("b", "a"), ("b", "c"), ("c", "d"), ("d", "b")]);
        assert_eq!(witnesses(&p, None, 8), vec![vec!["a", "b", "a"], vec!["b", "c", "d", "b"]]);
        let only = pairs(&[("c", "d")]);
        assert_eq!(witnesses(&p, Some(&only), 8), vec![vec!["b", "c", "d", "b"]]);
        assert_eq!(witnesses(&p, None, 1).len(), 1);
    }

    #[test]
    fn two_components_stay_apart() {
        let p = pairs(&[("a", "b"), ("b", "a"), ("x", "y"), ("y", "x"), ("b", "x")]);
        let cyc = cyclic_pairs(&p);
        assert!(!cyc.contains(&("b".to_string(), "x".to_string())));
        assert_eq!(cyc.len(), 4);
    }
}
