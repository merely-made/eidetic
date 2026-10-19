# Lighter recall, and a standards-to-features ledger — brainstorm brief

**Date**: 2026-09-07
**Status**: Brainstorm brief with Mark. Research, contradictions, ideas and a
proposed ledger; no code changed. Every ruling is Mark's.
**Occasion**: Mark, 2026-09-07, on the [eidetic review brief](2026-09-04_eidetic_review_brief.md)'s
conclusion: *"tantivy is pretty sizeable and we could get away with a lighter
approach given our web platform skill tree we're gonna grind. Ideas? And it
would be cool to generalize that approach elsewhere... 'develop and adhere to
these standards and we'll unlock these features.'"*
**Method**: the review brief's claims were re-checked against mere `0a8198ba29`,
genet `5af76a0cb8` and turnstone `c6ee31ed92`; the external survey used the
crate registries and Mozilla's ranking documentation, cited inline.

---

## 1. Where the review brief's conclusion stands

The engine argument holds. Four things around it are wrong or under-argued.

### 1.1 Page text was indexed once; the wiring died with meerkat

The brief reports that nothing supplies `text_for`, and that is true at HEAD.
It is not a design fact. The [capture plan](https://github.com/merely-made/mere/blob/main/design_docs/archive_docs/2026-10-06_completed_plans/2026-06-26_capture_provenance_consent_plan.md)'s
C5 built and headed-verified body-text recall on 2026-06-28 (commit
`8b8b039`): meerkat's `>recall` verb pulled each page's `main_text` from the
content cache and re-minted the index with it. Meerkat was obviated by
turnstone on 2026-07-18, and turnstone's recall actor was rebuilt from traces
alone. Today `TraceEvent` carries `url` and `title` only
(`crates/eidetic/eidetic-core/src/browsing/mod.rs:92-93`), and turnstone's
fetch path extracts only the title through `fleece::extract`
(`turnstone/src/browse.rs:517-518`).

So the brief's criterion 2 ("a consumer supplies page bodies") is not a
hypothetical. It was decided yes, shipped, and lost in a host swap. Whatever
engine is chosen, the decision should assume bodies come back.

### 1.2 Two docs disagree about whether text reaches the corpus

The [search surface wiring plan](https://github.com/merely-made/mere/blob/main/design_docs/mere_docs/implementation_strategy/2026-08-12_search_surface_wiring_plan.md)'s
W3 note of 2026-08-26 states that "the live host capture path supplies
extracted page text to the trace corpus." It does not: the trace type has no
text field and no producer extracts body text. That note justified removing
`eidetic-search`'s direct Fleece dependency, which was the right removal for
the wrong reason. The plan is the wiring plan's owner's to correct; recorded
here, not edited.

### 1.3 "Durable index" is the wrong axis

The brief treats the engine choice as turning on whether the index is opened
from disk. The stack's own doctrine already answers that: the corpus is the
authority and every index is a derived projection
(`turnstone/src/trail_memory.rs:157`, "re-minted from the corpus, never
repaired"). Durability of a projection is a cache policy, not an engine
property. The question that actually decides the engine is **what a re-mint
costs at the corpus sizes we will see**, and the brief's own open question 1
says that is unmeasured. Measure before choosing; see §5.

### 1.4 Two lexical rankers over one token stream

turnstone already fuses tantivy's BM25 with `esp`'s hashed n-gram vectors by
reciprocal-rank fusion (`trail_memory.rs:385-391`). Both are lexical; both
tokenize the same titles. Adding an in-tree BM25 beside the hashed embedder,
as the brief proposes, keeps two tokenizers and two rankers for one signal.
One tokenizer feeding one postings map, from which both BM25 scores and the
hashed vector fall out, is the smaller design. That tokenizer is the seed of
the ledger in §4.

---

## 2. Engines surveyed

| Option | Cost | Fits wasm lane | Incremental | Verdict |
|---|---|---|---|---|
| tantivy (today) | 98 crates, C build (`zstd-sys`), mmap | no | yes | keep only if a re-mint proves too slow |
| SQLite FTS5 | second C dep, `links = "sqlite3"` conflict | no | yes | rejected, agreed with the brief |
| in-tree BM25 (brief's pick) | ~300 lines, zero deps | yes | trivial (it is a map) | default candidate |
| [`bm25`](https://crates.io/crates/bm25) crate | MIT; embedder + scorer + engine with upsert/remove; stemming tokenizer is a feature that can be turned off | untested | yes | worth a one-day probe before writing our own |
| [`probly-search`](https://lib.rs/crates/probly-search) | MIT, 1.5k lines, two deps, wasm-compatible, trie inverted index, latent delete + vacuum | yes | yes | credible; last release July 2024, small user base |
| tinysearch-style bloom index | tiny, static | yes | no (rebuild) | a shape, not a fit: our corpus grows per visit |

Two considerations the brief did not weigh:

- **Stemming and Unicode normalization** are where a hand-rolled BM25 gets
  embarrassing. `rust-stemmers` is one of tantivy's 98 crates; both `bm25` and
  tantivy carry it. If we own the tokenizer, we own that decision too. A
  title-and-URL corpus barely needs stemming; a body-text corpus does.
- **Field weighting** (title above headings above body) is what makes a
  history search feel right, and it is a per-field BM25 with weights, not an
  engine feature. Genet already computes the structure (§3.3), so the
  weighting is free once the tokenizer exists.

**Direction, pending Mark:** measure re-mint cost first (§5). If a session's
corpus re-mints in tens of milliseconds, take the in-tree BM25 with one shared
tokenizer, probe `bm25` for a day to steal its tokenizer decisions, and retire
tantivy. If re-mint cost is real, `probly-search` is the incremental option
that still reaches the browser.

---

## 3. Ideas the review did not reach

### 3.1 Frecency: the browser world's answer, and we already hold its inputs

Firefox has ranked history recall since 2008 without full text: **frecency**,
a per-visit weight by transition kind (typed and bookmarked high, link clicks
medium, redirects and reloads near zero) under a 30-day exponential half-life,
plus an *adaptive input history* that remembers which result the user picked
for which typed string ([Mozilla's ranking doc](https://firefox-source-docs.mozilla.org/browser/urlbar/ranking.html)).
Recent versions promote a visit when the page showed "interesting
interaction": view time over a threshold, or moderate view time plus
keypresses.

Every input exists in our trace already. `TraceTransition` has the ten kinds
(`browsing/mod.rs:96-109`); `TraceEvent::at_ms` is the timestamp;
`dwell_ms` is the field, written `None` by the only producer. Nothing computes
a frecency. It is a fold over the corpus, no index, no crate, and it answers
the omnibar's most common case (a URL prefix the user has typed before) better
than BM25 does. It also gives the `esp`/BM25 fusion a third, behavioural
ranking to fuse. **This is the cheapest large win in the lane.**

### 3.2 Page identity by content, not URL

The brief found one visited page materialized five times, deduplicated at read
time by URL string. URLs are a poor identity: tracking parameters, mirrors,
`http`/`https`, trailing slashes. The family already content-addresses bytes
with blake3. Extend it one level: a **page fingerprint** over the extracted
main text (exact: blake3; near: a simhash or minhash, both under 100 lines)
gives "seen this before" across URLs, collapses the five materializations to
one interned `PageRef` plus N visit events, and is the natural key for the
index. This answers the brief's question 4 with a third option: traces stay
an event log, and the page table is a projection keyed by fingerprint.

### 3.3 Structure the engine already computes is the index's schema

Genet produces, per document, a `ContentReport` (title, outline of role and
name, headings, links; `genet-render/src/inspect.rs`), an accessibility tree
(`document-session-api/src/a11y.rs`), and fleece's `extract_text` and
`extract_main_text` (`fleece/src/lib.rs:558, 821`). That is a field-weighted
document (title, headings, body, link text) with no extra parse. The recall
lane should consume that shape rather than a flat string. The a11y tree is
the interesting one: it is the same content model a screen reader walks, so
indexing it means "search finds what a reader would say", which is both a
quality property and a conformance argument (§4).

### 3.4 One tokenizer, four consumers

Genet already depends on `unicode-segmentation` in fleece and
genet-documents, and a Unicode-conformant segmenter is what the Selection,
find-in-page and `Intl.Segmenter` surfaces need. The same word and sentence
segmentation is what a search tokenizer, the `esp` hashed embedder, and a
reading-time or word-count feature need. One segmentation component in genet,
held to UAX #29, is a platform primitive that mere consumes; it should not be
re-implemented in an intel crate.

### 3.5 Index as invalidated projection, not re-minted whole

The stack already has one disciplined answer to "derived state over a
changing authority": netrender's fragment invalidation, and chartulary's
`GraphLog` revisions. A postings map keyed by page fingerprint, invalidated by
corpus revision, is the same pattern. "Incremental index" then stops being an
engine feature and becomes the standard projection contract every derived
structure in the stack follows. Worth stating once, in the projection grammar,
rather than per crate.

---

## 4. The standards-to-features ledger

Mark's generalization: *develop and adhere to these standards, and we unlock
these features.* The 2026-09-06 [WPT census](../../../../genet/design_docs/2026-09-06_web_platform_wpt_census.md)
gives every row a measured baseline. The ledger's use is direction: when a
genet lane picks its next directory, the right column says what mere and the
products get for it, so the grind is ordered by payoff rather than by
alphabetical WPT order.

Seed rows. Counts are the census's subtests passed over total; "harness" marks
a count depressed by a runner gap the census itself names.

| Standard | Census | Adhering means | Unlocks in the stack |
|---|---|---|---|
| Unicode segmentation, UAX #29 | not a WPT dir; `Intl.Segmenter` under `intl` | one conformant word/sentence/grapheme segmenter in genet | search tokenizer (§3.4), find-in-page word mode, `esp` lexical features, reading time, selection by word |
| Selection API | 0 / 280 | `getSelection`, ranges over the layout DOM | web clip as a real gesture (capture plan C3's parked half), quote-with-provenance, find-in-page highlighting |
| Accessibility tree, ARIA / AccName | not measured (no testharness lane) | roles and names computed per spec | field-weighted index from the reader's model (§3.3), agent-driven pages, the inspector as oracle |
| Intersection Observer | 0 / 104 | viewport intersection callbacks | dwell and "interesting interaction" for frecency (§3.1), lazy media, attention receipts for the trail pane |
| High Resolution Time, Performance Timeline | 0 / 14, 0 / 73 | monotonic clocks, entries | `dwell_ms` filled honestly, page-load receipts, the timing half of the capture record |
| Mutation Observer | 9 files fail on the missing global | DOM change notifications | re-extract body text on SPA navigation, so the index tracks what the user actually read |
| URL | 351 / 519 | WHATWG parsing and canonicalization | page identity (§3.2) starts from canonical URLs; import from other browsers matches ours |
| Encoding | 7,109 / 1.33M (legacy slices) | labels and decoders | history and bookmark import from every browser's export, not just UTF-8 |
| IndexedDB, Storage | 5 / 880, 0 / 75 | the storage APIs, quota | muniment's OPFS lane hosted by our own engine; browsing memory in the browser (brief's question 3) |
| Web Crypto | 3 / 199 (86 files miss `crypto`) | SubtleCrypto over our primitives | pack signing and sealing in the browser lane, personae in a web host |
| Workers | 17 / 574 | dedicated workers, message passing | indexing and embedding off the document thread; `esp` in the browser |
| Web Messaging, BroadcastChannel | 49 / 209 | channels across contexts | the graphshell remote-projection wire planes hosted in-page |
| Microdata / JSON-LD in `<script type="application/ld+json">` | not a WPT dir | parse and expose structured data | `mere-linked-data` ingest straight from visited pages; the semantic ring fills itself |
| Custom Elements, Shadow DOM | 2,041 / 3,674, 18 / 8,654 | the component model | Cambium widgets hosted inside web documents; the reverse of embedding the web in Cambium |

Rows a lane should add as it opens: `editing` (contenteditable is what a
writing product needs), `streams` (fetch bodies as they arrive, so extraction
starts before the page finishes), `service-workers` (offline products).

**Proposed home:** this ledger is genet's to own, since the left three columns
are genet facts and the census is genet's baseline. It would sit beside the
census as `genet/design_docs/<date>_standards_to_features_ledger.md`, with
mere and the products adding rows to the right column as their plans name a
dependency. Founding it there is Mark's call, and outside this brief's repo.

---

## 5. Questions for Mark

1. **Measure before choosing.** Instrument turnstone's `RecallIndex::mint`
   to report event count, corpus bytes, and re-mint latency, then decide the
   engine on numbers. Yes, and bring the numbers back first?
2. **Frecency first?** It needs no engine and lands in the existing fusion.
   Should it precede the engine change, so the engine decision is made
   against a lane that already ranks well?
3. **Restore body text as a stated slice.** C5 is lost, not decided against.
   Re-queue it in the wiring plan as "host extracts `main_text` through
   fleece and attaches it to the trace or to a fingerprint-keyed page
   record", and correct the 2026-08-26 W3 note. Which doc owner does that?
4. **One tokenizer in genet.** Found the UAX #29 segmentation component as a
   genet primitive that the search lane, find-in-page and Selection share?
   This is the first ledger row and the one this brief's engine choice
   depends on.
5. **Found the ledger in genet?** Table above as the seed, beside the census.
6. **Page fingerprint.** Interning `PageRef` by content fingerprint answers
   the brief's question 4 with "event log plus projected page table". Take
   that reading?

**Ruled 2026-09-07, all six yes.** Numbers first; frecency before the
engine; C5 re-queued as W6c of the
[search surface wiring plan](https://github.com/merely-made/mere/blob/main/design_docs/mere_docs/implementation_strategy/2026-08-12_search_surface_wiring_plan.md)
and the W3 note corrected there; the UAX #29 segmenter is genet's to found;
the ledger is founded in genet as
`genet/design_docs/2026-09-07_standards_to_features_ledger.md`; traces are an
event log with a fingerprint-keyed page table projected over them (W6d).
