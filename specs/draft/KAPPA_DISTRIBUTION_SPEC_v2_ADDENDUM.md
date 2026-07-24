# κ-Distribution Protocol Specification — Addendum
## v2.0.0-addendum-1
Date: 2026-07-21
Applies to: KAPPA_DISTRIBUTION_SPEC_v2.md (v2.0.0)
Status: Normative where marked, Informative where marked
---
This addendum is read alongside the base specification. It adds
vocabulary, fixes defects, tightens grammar, and appends
informative appendices. It does not add operations, endpoints,
or mechanisms.
---
## A1. Object Model Addition — §3.1a (Normative)
Insert after §3.1 paragraph 3 ("There is exactly one kind of
object in κ-Distribution: a blob."):
**Axiomatic and derived blobs.** A blob is axiomatic if no
rooted derivation record exists for it in the store. A blob
is derived if a rooted chain of "composed-of" edges with
operation-typed metadata connects it to axiomatic inputs
through the content-constructive operations defined in §5.4.
The classification is not stored as a flag. It is discoverable
by traversing the edge graph from the blob's κ-label along
"composed-of" edges to their operands, recursively, until
every leaf is either axiomatic (no further composed-of edges)
or absent (derivation record incomplete).
A derived blob is deterministically reconstructible: replay the
operation chain from available axiomatic inputs, re-hash each
intermediate result, and compare against the κ-label at each
step. Reconstruction succeeds iff every operation on the path
is content-constructive (produces the same output from the same
inputs on every invocation) and every axiomatic input is
available.
The protocol does not mandate reconstruction. It defines the
condition under which reconstruction is possible. Future
versions of this specification MAY define normative
reconstruction behavior for GC and replication.
---
## A2. Edge Relation Type Addition — §3.4a (Normative)
Add to the relation type conventions list in §3.4:
    superseded-by  loser → winner in conflict resolution
                   Created deterministically from the two
                   conflicting κ-labels and the winning κ-label.
                   Content-addressed and idempotent — every
                   registry resolving the same conflict derives
                   the identical edge with zero coordination.
                   No v2.0.0 operation creates this edge. It is
                   reserved for the multi-registry conflict
                   resolution mechanism (future addendum).
---
## A3. Consistency Statement Addition — §7.1a (Normative)
Insert at end of §7.1 (Content Consistency):
Two registries independently computing the same composition
operation (§5.4) over the same operands produce the same
composed κ-label with zero coordination. This follows from the
deterministic canonical forms (Appendix B of the base spec):
same operands, same operation, same canonical form, same hash,
same κ-label. This is the strongest consistency property the
protocol provides — agreement without communication.
---
## A4. Content-Before-Tag Scoping Fix — §7.5a (Normative)
Replace the first sentence of §7.5 with:
Blobs MUST exist before client-initiated tag_set binds to them.
The content-before-tag invariant (§5.7 invariant 3) applies to
operations initiated by a client through the protocol's HTTP
binding (§6.5 PUT /v2/{path}/manifests/{tag}). Replication-apply
paths — where a tag binding arrives from a peer registry via
whatever replication mechanism the deployment uses — are exempt
from this invariant. A registry receiving a replicated tag for
an absent κ-label MUST accept the binding. The content populates
later via continued replication or federation fetch.
Rationale: in multi-registry deployments, tags and content
replicate at different speeds (§8.2 of the informative
multi-registry appendix). Requiring content to arrive before
its tag on the replication path would create a circular
dependency — the replicating registry cannot know in advance
which tags are inbound, and holding tags in a buffer pending
content arrival adds unbounded state. The exemption resolves
this without weakening the client-path invariant.
---
## A5. Section Renumbering Fix (Normative)
The base spec has two sections numbered §5.7:
    §5.7 Conditional Operations (tag_set_if)
    §5.7 Invariants
with §5.8 Cross-Namespace Mount between them.
Corrected numbering:
    §5.7  Conditional Operations
    §5.8  Cross-Namespace Mount
    §5.9  Invariants
All references to "§5.7 invariant 3" in this addendum and the
base spec refer to invariant 3 in the renumbered §5.9.
---
## A6. Grammar Tightening — §4.2a (Normative)
Replace the hex-digest production in §4.2:
    hex-digest = 2*128(HEXDIG)
With per-token constrained productions:
    hex-digest     = sha256-hex / blake3-hex / sha3-256-hex
                   / keccak256-hex / sha512-hex
    sha256-hex     = 64HEXDIG
    blake3-hex     = 64HEXDIG
    sha3-256-hex   = 64HEXDIG
    keccak256-hex  = 64HEXDIG
    sha512-hex     = 128HEXDIG
The previous production admitted hex strings of any length
between 2 and 128 digits. The replacement constrains each
axis token to its exact digest width. An implementation
parsing a κ-label MUST reject a hex digest whose length does
not match the axis token's specified width.
---
## A7. Eviction Loss Statement — §12.5a (Normative)
Replace the last sentence of §12.5:
    "The writer re-pushes the blob (idempotent if still available
    from a peer) and retries."
With:
    For axiomatic content (§3.1a): if the evicted writer held
    the only copy and no peer holds a replica, the content is
    lost. This is an unrecoverable data loss for axiomatic blobs.
    The mitigation is the pin system — pinned content is not
    GC-eligible. Writers that need durability beyond a single
    sweep cycle MUST pin their axiomatic content.
    For derived content (§3.1a): if the derivation record
    (composed-of edges and axiomatic inputs) is intact, the
    content is reconstructible. Future versions of this
    specification MAY define automatic reconstruction on read.
---
## A8. LWW Tie-Break Note — §8.4a (Informative)
Insert at end of §8.4 (Conflict Resolution) in the informative
multi-registry appendix:
Note: the lexicographic tie-break on equal HLC timestamps is
grindable. A writer with knowledge of a competing write can
search content variants whose κ-label sorts higher, winning
the tie deterministically. This is a considered design decision.
The grindability requires the attacker to know the competing
κ-label before the HLC window closes, which in practice
requires the attacker to be one of the two conflicting writers.
The cost of grinding (hashing candidate content until a
lexicographically greater κ-label is found) is 2^127 expected
hashes for SHA-256 — computationally infeasible. For
applications where even theoretical grindability is
unacceptable, signed tags (§11.2) provide an independent
integrity mechanism.
---
## A9. §8 Status Change (Structural)
The base spec's §8 (Replication and Federation) is reclassified
from normative to informative for v2.0.0. The section header
becomes:
    ## 8. Multi-Registry Patterns (Informative)
All RFC 2119 key words (MUST, SHOULD, MAY) within §8 are
reinterpreted as descriptive guidance for implementors of
multi-registry deployments, not as conformance requirements
for single-registry implementations. A conforming v2.0.0
registry is not required to implement any behavior described
in §8.
The section's content is unchanged. The reclassification
reflects that:
- No §8 behavior is testable against a single registry
- The conformance suite's federation tests (L5-50 through
  L5-58) already skip when KAPPA_FEDERATION_PEERS is unset
- The mechanisms required to make §8's multi-registry claims
  normative (HLC wire format, tombstones, replication receipts,
  GC grace quarantine) are deferred to a future Multi-Registry
  Protocol Addendum
When the Multi-Registry Protocol Addendum ships, it will
re-elevate §8 to normative and supply the deferred mechanisms.
Implementations that follow §8's informative patterns today
will be compatible with the normative version — no rework.
---
## A10. Informative Appendix: Agreement Demo
Insert as new appendix after the existing Appendix G
(Long-Duration Disconnection):
### Appendix G2: The Agreement Demo
The base spec's Appendix G (submarine worked example)
demonstrates the custody path: axiomatic content is written,
relayed, and read. This appendix demonstrates the agreement
path: derived content is independently reproduced without any
intermediary holding the value.
**Setup.** Two registries, A and B, sharing no content.
Both hold the same two axiomatic blobs:
    κ_x = sha256:... (some shared reference data)
    κ_y = sha256:... (some shared reference data)
These were synced during a prior connectivity window.
**Step 1.** Registry A computes a G2 composition:
    compose(g2, [κ_x, κ_y]) → κ_c, witness_w
A stores:
    - blob κ_c (the composed value)
    - blob witness_w
    - edge: κ_c composed-of κ_x (metadata: {"operation":"g2"})
    - edge: κ_c composed-of κ_y (metadata: {"operation":"g2"})
    - tag: some/path@result → κ_c
**Step 2.** Both registries go dark. A and B are offline for
an extended period. No data transfers occur. No intermediary
holds κ_c.
**Step 3.** A and B reconnect. Only the derivation metadata
replicates to B:
    - edge: κ_c composed-of κ_x (metadata: {"operation":"g2"})
    - edge: κ_c composed-of κ_y (metadata: {"operation":"g2"})
    - tag: some/path@result → κ_c
The blob κ_c (the composed bytes) has NOT replicated. B does
not hold the value.
**Step 4.** A client queries B:
    GET /v2/some/path/manifests/result
B resolves the tag to κ_c. B does not hold κ_c locally.
**Step 5 (v2.0.0 behavior).** B performs federation fetch if
available. If A is reachable, B fetches κ_c from A, verifies
by re-hash, caches, serves. This is the custody path — it
works but requires A to be reachable.
**Step 5 (future, with §A1 reconstruction).** B discovers the
derivation record for κ_c: two composed-of edges with
operation "g2" pointing to κ_x and κ_y. B holds both
operands. B independently computes:
    compose(g2, [κ_x, κ_y]) → κ_c', witness_w'
B re-hashes the result: κ_c' == κ_c. The content is
identical. B serves it. No data of the value ever crossed the
link — only the derivation record (two edges, ~400 bytes of
metadata).
**Why this matters.** The custody path requires every byte of
the value to traverse the network at least once. The agreement
path requires only the derivation record. For large composed
values (merged datasets, composed model weights, aggregated
telemetry), the bandwidth savings are proportional to the
ratio of value size to derivation record size. For a 100 MB
composed dataset derived from two 50 MB inputs that both
registries already hold, the agreement path transfers ~400
bytes instead of 100 MB.
The agreement path is not a v2.0.0 normative feature. It is
documented here to show that the protocol's existing machinery
(composition operations, composed-of edges, operation-typed
metadata) already provides the foundation. The future
Multi-Registry Protocol Addendum will define normative
reconstruction-on-read behavior.
---
## A11. Informative Appendix: Future Work
Insert as final appendix:
### Appendix P: Future Work — Multi-Registry Protocol
The following mechanisms are identified as necessary for
normative multi-registry behavior. They are deferred from
v2.0.0 and will ship as a Multi-Registry Protocol Addendum.
Implementations MAY implement these mechanisms experimentally.
The patterns described in §8 (informative) are compatible with
these mechanisms — no rework is expected.
**HLC specification.** Hybrid Logical Clock wire format, HLC
assignment rules, receive-merge rules (max of local HLC and
received HLC + 1), persistence across power cycles, and
clock-loss semantics (a device with a dead RTC stamps old
timestamps and loses its writes to LWW — this may be
acceptable for sensors but will be stated explicitly).
Wire format for tag replication records:
    (path, name, κ-label, hlc, previous-κ-label)
The previous-κ-label field enables conflict detection at the
receiving registry.
**Tombstones.** Edge deletion and tag deletion in multi-registry
deployments require tombstone records to prevent resurrection
via stale replication. A tombstone is a blob whose content
records the deleted entity's identity and the deletion HLC.
Tombstone retention policy under unbounded disconnection:
tombstones are permanent (safe, storage cost grows) or
tombstones have a TTL after which resurrection is possible
(bounded storage, correctness risk). The addendum will choose
one and state the consequence.
**Replication receipts.** A per-registry replication journal
exposed as blobs plus a tag snapshot. Convention: a tag at a
well-known path per peer carrying "synced through HLC X with
peer P." This makes durability handoff observable — a writer
can confirm that its content has been replicated before
powering off.
**GC grace quarantine.** Blobs newly arrived via replication
are non-candidates for GC until the next sweep cycle or a
configurable quarantine period. This prevents the tri-color
violation where a blob arrives between the mark phase and the
sweep phase and is evicted because its root (pin or tag) has
not yet replicated. The quarantine rule is an additive change
to the §9.3 sweep algorithm.
**Pin TTL policy for unreplicated axiomatic content.** Pins
protecting locally-originated axiomatic content with no
confirmed replica (per replication receipts) MUST NOT expire
regardless of their wall-clock TTL. This depends on replication
receipts being observable.
**Derived content eviction.** A derived blob whose derivation
record is rooted (all composed-of edges reachable, all
axiomatic inputs available) MAY be evicted by GC regardless of
pins on the bytes. A subsequent get triggers reconstruction:
replay the derivation chain from axiomatic inputs, re-hash
each step, compare against the κ-label, serve. This converts
derived content from "data" to "cache" and collapses the
replication bandwidth requirement for derived content to the
size of the derivation record.
**Superseded-by edge creation.** When conflict resolution
(§8.4) determines a winner, the resolving registry creates a
"superseded-by" edge from the losing κ-label to the winning
κ-label. The edge is deterministic (same conflict → same
edge), content-addressed (same canonical form → same κ-label),
and idempotent (every registry derives the identical edge).
This makes conflict history enumerable.
**Anti-entropy reconciliation.** The mechanism by which a
reconnecting registry discovers what it is missing. Options
under consideration: tag snapshot diffing (compare tag lists
at two points in time), replication journal replay (sequential
cursor through the journal), Merkle-ized label sets (hash tree
over the set of all κ-labels, exchange roots, recurse into
differing subtrees). The addendum will specify one or permit
implementor choice with an interoperability profile.
**Bounded-universe sectors.** Where an application's derived
content ranges over a provably finite operator orbit (a closed
set of canonical states under a fixed operation alphabet), a
registry can carry the entire reachable value universe locally.
Sync of that sector is trivially bounded — the maximum state
is the size of the orbit. The pattern: a scoped path whose
admission schema restricts content to a finite enumerable set,
with the enumeration itself stored as a blob. This is the
extreme point of derived-content eviction and the strongest
offline-first configuration the protocol admits.
**Multi-registry conformance (Level 6).** A conformance level
requiring two registry instances and a controlled network link.
Test categories:
- Partition: write at A, sever link, verify absence at B,
  restore link, verify convergence and re-hash
- Agreement: evict derived blob, read triggers reconstruction,
  verify κ-label matches
- Replicated-tag-to-absent-content: verify §8.2 exemption
- Conflict convergence: the §13.3 vectors as numbered tests
- GC grace: deliver blob without root, sweep, verify survival
- Tombstone non-resurrection: delete at A, replicate stale
  from B, verify deletion holds
- HLC persistence: restart registry, verify HLC monotonicity
---
## A12. Direct Tag Binding Endpoint — §6.5a (Normative)

Insert after §6.5 (Tag Endpoints) in the base spec:

**Bind tag to existing κ-label (no body):**

    PUT /v2/{path}/tags/{name}?kappa={κ-label}
    No request body. Binds the tag to the specified κ-label.

    Response: 201 Created (new tag binding)
    Headers:
        X-Kappa-Label: {κ-label}
    Response: 200 OK (tag binding updated)
    Headers:
        X-Kappa-Label: {κ-label}
    Response: 404 Not Found (κ-label does not exist in store)
    Response: 422 Unprocessable Entity (malformed κ-label)

The registry MUST verify that the κ-label exists in the store
before binding the tag. If the κ-label does not exist, the
registry MUST return 404 with error code TAG_CONTENT_ABSENT.
This is the direct enforcement of the content-before-tag
invariant (§5.9 invariant 3) — the invariant that PUT
/manifests/{tag} enforces implicitly via atomic store+tag, but
that is independently testable only through this endpoint.

The κ-label in the query parameter MUST be a valid, recognized
κ-label (correct axis token, correct hex digit count per axis).
If the κ-label is malformed, the registry MUST return 422 with
error code DIGEST_INVALID.

**Compare-and-swap (conditional tag binding):**

    PUT /v2/{path}/tags/{name}?kappa={κ-label}
    If-Match: {expected-κ-label}
    Response: 200 OK (tag updated, previous matched expected)
    Response: 409 Conflict (current tag value != expected)

    PUT /v2/{path}/tags/{name}?kappa={κ-label}
    If-None-Match: *
    Response: 201 Created (tag did not exist, now bound)
    Response: 409 Conflict (tag already exists)

CAS operations via this endpoint are linearizable within the
tag's consensus domain (§7.4). The tag_set_if operation
described in §5.7 maps directly to this HTTP surface.

**Rationale.** The base spec's §7.4 defines the consensus
interface as `tag_set(path, name, κ-label)` — a pointer update
with no content payload. The base spec's HTTP binding (§6.5 PUT
/manifests/{tag}) conflates content storage with tag binding,
requiring the caller to re-upload content bytes to move a
pointer. This endpoint separates the concerns:

- Content storage: PUT /v2/{path}/blobs/{κ-label} (Level 1)
- Tag binding: PUT /v2/{path}/tags/{name}?kappa={κ-label} (Level 2)

This separation enables:
- Pointer-only tag promotion (staging→production) without
  content re-upload
- Tag-only replication in multi-site deployments (§8.2
  metadata-before-content)
- Lightweight controller reconciliation (tag update without
  holding content in memory)
- Independent conformance testing of the content-before-tag
  invariant
- CAS operations on tag pointers without content re-submission

This endpoint is REQUIRED at Level 2. A conforming Level 2
registry MUST implement both PUT /manifests/{tag} (atomic
store+tag with body) and PUT /tags/{name}?kappa= (pointer-only
binding without body).

**Endpoint summary addition (§A12):**

    Method  Path                                    Cons.  Level
    ------  ----                                    -----  -----
    PUT     /v2/{p}/tags/{name}?kappa={κ}           yes    2
    35 → 36 endpoints. 6 require consensus. 30 do not.

---
## A13. Direct Tag Resolution Endpoint — §6.5b (Normative)

Insert after §A12 (Direct Tag Binding Endpoint):

**Resolve tag to κ-label (no content body):**

    GET /v2/{path}/tags/{name}
    Response: 200 OK
    Headers:
        X-Kappa-Label: {κ-label}
    Body: {"name":"{name}","kappa":"{κ-label}"}
    Response: 404 Not Found (tag does not exist)

The registry MUST return the κ-label the tag is bound to in
both the X-Kappa-Label header and the JSON body. The response
body is the tag binding metadata ONLY — it does NOT include
the blob content. The Content-Length of the response is the
size of the JSON body (typically < 200 bytes), NOT the size
of the blob the tag points to.

This endpoint separates tag resolution from content retrieval:

- GET /v2/{path}/manifests/{tag} → resolves tag, returns
  content body (existing, unchanged)
- GET /v2/{path}/tags/{name} → resolves tag, returns κ-label
  only (new, §A13)

**Rationale.** The base spec's §5.2 defines `tag_get(path, name)
→ κ-label | absent` — a pointer lookup. The base spec's HTTP
binding (§6.5 GET /manifests/{tag}) returns the full content
body, conflating resolution with retrieval. A controller that
needs "what κ-label does latest point to?" must download the
entire blob to read the X-Kappa-Label header.

This separation enables:
- Pointer-only resolution for controllers, CI, and monitoring
  (71 bytes instead of megabytes per query)
- Efficient reconciliation loops (poll tag, compare κ-label,
  fetch content only if changed)
- Tag-only replication protocol (replicate bindings without
  transferring content)
- Separation of the resolution cache from the content cache
  (tags are mutable and short-TTL; content is immutable and
  infinite-TTL)

This endpoint is REQUIRED at Level 2. A conforming Level 2
registry MUST implement GET /manifests/{tag} (resolve + content),
GET /tags/{name} (resolve only), and PUT /tags/{name}?kappa=
(direct bind).

**Endpoint summary addition (§A13):**

    Method  Path                                    Cons.  Level
    ------  ----                                    -----  -----
    GET     /v2/{p}/tags/{name}                     no     2
    36 → 37 endpoints. 6 require consensus. 31 do not.
