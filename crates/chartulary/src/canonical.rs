// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Canonical resource addresses shared by graph and storage consumers.

use uuid::Uuid;

/// Query keys dropped as campaign noise: these name the click, not the page.
const TRACKING_PARAMS: [&str; 9] = [
    "fbclid", "gclid", "dclid", "gbraid", "wbraid", "msclkid", "mc_cid", "mc_eid", "igshid",
];

/// Whole families dropped by prefix (Urchin's `utm_source`, `utm_medium`, …).
const TRACKING_PARAM_PREFIXES: [&str; 1] = ["utm_"];

/// The port a scheme already implies, and so need not carry.
fn default_port(scheme: &str) -> Option<&'static str> {
    match scheme {
        "http" | "ws" => Some("80"),
        "https" | "wss" => Some("443"),
        _ => None,
    }
}

/// Collapse a URL to the page it names: lowercase scheme and host, no
/// fragment, no default port, no trailing slash on an empty path, no tracking
/// parameters. Everything else is kept verbatim — percent-encoding, case in
/// the path and in surviving query values, and parameter order all carry
/// meaning on real sites.
///
/// A string with no `scheme://` (a `data:` or `about:` form, a bare path)
/// keeps its shape; only the fragment comes off. There is no URL crate here
/// on purpose: the WHATWG parser belongs to the engine, and this key must be
/// computable in the storage layer.
pub fn canonical_url(raw: &str) -> String {
    let raw = raw.trim();
    let without_fragment = raw.split_once('#').map_or(raw, |(head, _)| head);
    let Some((scheme, rest)) = without_fragment.split_once("://") else {
        return without_fragment.to_string();
    };
    let scheme = scheme.to_lowercase();
    let (authority, tail) = rest.split_at(rest.find(['/', '?']).unwrap_or(rest.len()));
    let (path, query) = tail.split_once('?').map_or((tail, ""), |(p, q)| (p, q));
    let authority = canonical_authority(authority, &scheme);
    let path = if path == "/" { "" } else { path };
    let query = canonical_query(query);
    let mut canonical = format!("{scheme}://{authority}{path}");
    if !query.is_empty() {
        canonical.push('?');
        canonical.push_str(&query);
    }
    canonical
}

/// Deterministic resource UUIDv5 over its canonical IRI in the URL namespace.
pub fn resource_id(raw_iri: &str) -> Uuid {
    resource_id_from_canonical_iri(&canonical_url(raw_iri))
}

/// Hash an already prepared identity IRI, without changing its bytes.
pub fn resource_id_from_canonical_iri(canonical_iri: &str) -> Uuid {
    Uuid::new_v5(&Uuid::NAMESPACE_URL, canonical_iri.as_bytes())
}

fn canonical_authority(authority: &str, scheme: &str) -> String {
    let (userinfo, host_port) = match authority.rsplit_once('@') {
        Some((user, host)) => (Some(user), host),
        None => (None, authority),
    };
    // An IPv6 literal keeps its brackets; its port is whatever follows them.
    let (host, port) = match host_port.rfind(']') {
        Some(end) => {
            let (host, rest) = host_port.split_at(end + 1);
            (host, rest.strip_prefix(':'))
        },
        None => match host_port.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (host_port, None),
        },
    };
    let mut canonical = String::new();
    if let Some(userinfo) = userinfo {
        canonical.push_str(userinfo);
        canonical.push('@');
    }
    canonical.push_str(&host.to_lowercase());
    if let Some(port) = port.filter(|p| !p.is_empty() && Some(*p) != default_port(scheme)) {
        canonical.push(':');
        canonical.push_str(port);
    }
    canonical
}

fn canonical_query(query: &str) -> String {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .filter(|pair| {
            let key = pair
                .split_once('=')
                .map_or(*pair, |(key, _)| key)
                .to_lowercase();
            !TRACKING_PARAMS.contains(&key.as_str())
                && !TRACKING_PARAM_PREFIXES
                    .iter()
                    .any(|prefix| key.starts_with(prefix))
        })
        .collect::<Vec<_>>()
        .join("&")
}

#[cfg(test)]
mod tests {
    use super::{canonical_url, resource_id};

    #[test]
    fn resource_aliases_share_the_pinned_url_namespace_identity() {
        // Independently computed with Python's standard-library uuid5.
        let expected = "7150c543-9bc5-5b2f-bb82-bc18fd0ea48d";
        let canonical = "https://example.com/notes/?id=7";
        let id = resource_id(canonical);
        assert_eq!(id.to_string(), expected);
        assert_eq!(id.get_version_num(), 5);
        for alias in [
            "https://Example.COM:443/notes/?utm_source=news&id=7#section-2",
            "https://example.com/notes/?id=7&fbclid=abc123",
            " https://example.com/notes/?id=7#another-fragment ",
        ] {
            assert_eq!(resource_id(alias), id);
            assert_eq!(
                resource_id(&canonical_url(alias)),
                id,
                "canonical input is idempotent"
            );
        }
        assert_ne!(resource_id("https://example.com/notes/?id=8"), id);
        assert_ne!(resource_id("https://example.com/other/?id=7"), id);
    }

    #[test]
    fn resource_identity_preserves_meaningful_address_differences() {
        let id = resource_id("https://example.com");
        assert_eq!(id.to_string(), "4fd35a71-71ef-5a55-a9d9-aa75c889a6d0");
        assert_eq!(resource_id("https://EXAMPLE.COM:443/#part"), id);
        assert_ne!(resource_id("http://example.com"), id);
        assert_ne!(resource_id("https://example.com:8443"), id);
        assert_eq!(
            resource_id("https://example.com/a?x=1&x=2&utm_medium=mail"),
            resource_id("https://example.com/a?x=1&x=2")
        );
        assert_ne!(
            resource_id("https://example.com/a?x=1&x=2"),
            resource_id("https://example.com/a?x=2&x=1")
        );
        assert_ne!(
            resource_id("https://example.com/A"),
            resource_id("https://example.com/a")
        );
    }

    #[test]
    fn aliases_share_an_address_but_meaningful_parameters_do_not() {
        let address = "https://example.com/notes/?id=7";
        assert_eq!(
            canonical_url("https://Example.COM:443/notes/?utm_source=news&id=7#section-2"),
            address
        );
        assert_eq!(
            canonical_url("https://example.com/notes/?id=7&fbclid=abc123"),
            address
        );
        assert_eq!(
            canonical_url(address),
            address,
            "canonicalization is idempotent"
        );
        assert_ne!(canonical_url("https://example.com/notes/?id=8"), address);
        assert_ne!(canonical_url("https://example.com/other/?id=7"), address);
    }

    #[test]
    fn implied_ports_and_empty_paths_collapse_but_other_paths_stay() {
        assert_eq!(
            canonical_url("HTTP://Example.com:80/"),
            "http://example.com"
        );
        assert_eq!(
            canonical_url("https://Example.com:443/"),
            "https://example.com"
        );
        assert_eq!(canonical_url("ws://Example.com:80/"), "ws://example.com");
        assert_eq!(canonical_url("wss://Example.com:443/"), "wss://example.com");
        assert_eq!(
            canonical_url("https://example.com/a/"),
            "https://example.com/a/"
        );
        assert_ne!(
            canonical_url("https://example.com:8443/"),
            "https://example.com"
        );
        assert_ne!(
            canonical_url("https://example.com/a/"),
            canonical_url("https://example.com/a")
        );
    }

    #[test]
    fn surviving_query_case_encoding_and_order_carry_meaning() {
        assert_eq!(
            canonical_url("https://example.com/a?Q=Keep&gclid=x"),
            "https://example.com/a?Q=Keep"
        );
        assert_eq!(
            canonical_url("https://example.com/A%2fb?x=2&x=1&UTM_CAMPAIGN=x&keep=%2F"),
            "https://example.com/A%2fb?x=2&x=1&keep=%2F"
        );
        assert_ne!(
            canonical_url("https://example.com/a?x=1&x=2"),
            canonical_url("https://example.com/a?x=2&x=1")
        );
        assert_ne!(
            canonical_url("https://example.com/A"),
            canonical_url("https://example.com/a")
        );
    }

    #[test]
    fn authority_preserves_userinfo_and_ipv6_brackets() {
        assert_eq!(
            canonical_url("https://User@EXAMPLE.COM:443/"),
            "https://User@example.com"
        );
        assert_eq!(
            canonical_url("https://[2001:DB8::1]:443/"),
            "https://[2001:db8::1]"
        );
        assert_eq!(
            canonical_url("https://[2001:DB8::1]:8443/a"),
            "https://[2001:db8::1]:8443/a"
        );
        assert_ne!(
            canonical_url("https://Other@example.com/"),
            canonical_url("https://User@example.com/")
        );
    }

    #[test]
    fn nonhierarchical_addresses_keep_their_shape_except_fragment() {
        assert_eq!(canonical_url(" about:blank#part "), "about:blank");
        assert_eq!(canonical_url("urn:mere:note:ABC#part"), "urn:mere:note:ABC");
        assert_eq!(
            canonical_url("data:text/plain,Hello?Q=Keep#part"),
            "data:text/plain,Hello?Q=Keep"
        );
        assert_ne!(
            canonical_url("urn:mere:note:ABC"),
            canonical_url("urn:mere:note:abc")
        );
    }
}
