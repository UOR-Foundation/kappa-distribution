# κ-Distribution Protocol Specification
## Universal Content-Addressed Blob Store
Status: Draft Specification
Version: 1.1.0
Date: 2026-07-20
---
## 1. Abstract
κ-Distribution is a protocol for storing, retrieving, addressing,
composing, replicating, and garbage collecting content-addressed
blobs. Every object in the store is a blob identified by a κ-label
— a self-describing, algorithm-prefixed content hash. There are no
special object types. Manifests, edges, witnesses, schemas, pins,
and application content are all blobs, all addressed by κ-labels,
all stored and retrieved through the same operations.
Tags are mutable pointers from human-readable names to immutable
κ-labels. Tags are the only mutable state in the protocol.
The addressing convention encodes organizational hierarchy
to the left of a colon separator, resource identity to the right,
and version after an @ delimiter. Organizational depth grows
leftward via DNS-style subdomains. Resource specificity grows
rightward via path segments.
The protocol defines operations, consistency guarantees, garbage
collection semantics, replication behavior, admission control,
failure modes, and a conformance test framework. It does not
prescribe a storage backend, a consensus protocol, a replication
transport, an encryption mechanism, or an authorization framework.
Any system that implements the specified operations and upholds
the specified invariants is a conforming κ-Distribution store.
---
## 2. Terminology and Conventions
### 2.1 Key Words
The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT",
"SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in
this document are to be interpreted as described in RFC 2119.
### 2.2 Glossary
    κ-label         Content address. An ASCII string of the form
                    <algorithm>:<lowercase-hex-digest>. The algorithm
                    names the hash function. The digest is the
                    hexadecimal encoding of the hash output.
                    Lengths: 71 bytes (sha256, blake3), 73 bytes
                    (sha3-256), 74 bytes (keccak256), 135 bytes
                    (sha512). A κ-label is immutable, deterministic,
                    and self-describing.
    σ-axis          The hash algorithm component of a κ-label.
                    One of: sha256, blake3, sha3-256, keccak256,
                    sha512.
    blob            Arbitrary bytes stored in the system. Every blob
                    has exactly one κ-label per σ-axis. The same
                    bytes produce the same κ-label on any node at
                    any time.
    tag             A mutable binding from (path, name) to a κ-label.
                    Tags are the only mutable state in the protocol.
    edge            A typed directed relationship between two
                    κ-labels. An edge is itself a blob with its own
                    κ-label, serialized in canonical form from
                    (source, relation, target, metadata).
    manifest        A blob whose content references other κ-labels.
                    Not a distinct protocol type — a manifest is a
                    blob that happens to describe other blobs.
    witness         A blob containing the derivation trace of a
                    κ-label computation. Enables verification of a
                    κ-label without re-hashing the original content.
    schema          A blob containing validation rules. Content
                    written to paths matching the schema's scope is
                    validated before κ-label computation.
    pin             A blob that marks a κ-label as protected from
                    garbage collection. Contains the protected
                    κ-label and a TTL.
    finalizer       A pin carrying a controller name. Blocks GC
                    eviction until explicitly removed by the
                    named controller.
    consensus       An ordering authority that linearizes tag
                    domain        updates within a scope. The protocol requires
                    consensus domains exist but does not prescribe
                    their implementation or scoping.
    sweep           A garbage collection pass that walks reachability
                    from roots (pins + tags) and evicts unreachable
                    blobs.
    registry        Any endpoint implementing this protocol.
    federation      Cross-registry content resolution. A registry
                    that does not hold a requested blob queries
                    peers, which may relay to their peers.
    verify-on-      Client-side re-hash of received content to
    receipt         confirm κ-label integrity, independent of
                    server trust.
### 2.3 Notation Conventions
Path segments in braces are variable:
    {org-hierarchy}:{resource-path}@{version}
Angle brackets denote formal grammar productions:
    <org-segment> "." <org-segment>
The vertical bar separates alternatives:
    accept | reject
---
## 3. Object Model
### 3.1 Blobs
A blob is the atomic unit of storage. A blob consists of:
    content     arbitrary bytes, 0 to 2^63 - 1 bytes
    κ-label     the content address, computed from the content
A blob is immutable. Once stored, its content never changes. The
κ-label is deterministic: the same content always produces the
same κ-label under the same σ-axis, on any node, at any time.
There is exactly one kind of object in κ-Distribution: a blob.
All higher-level constructs — manifests, edges, witnesses, schemas,
pins, admission filters, tag snapshots — are blobs. They differ
in what their content means, not in how they are stored, addressed,
or retrieved.
A blob MAY have additional κ-labels under different σ-axes. The
same bytes addressed as `sha256:abc...` and as `blake3:def...` are
the same content with two different addresses. Cross-axis κ-labels
MAY be computed eagerly at ingestion time or lazily on demand when
a query arrives for an axis not computed at ingestion.
### 3.2 κ-Labels
A κ-label is an ASCII string:
    <algorithm> ":" <lowercase-hex-digest>
The algorithm is one of the recognized σ-axis tokens:
    Token       Digest bytes   κ-label bytes   Standard
    -----       ------------   -------------   --------
    sha256      32             71              FIPS 180-4
    blake3      32             71              BLAKE3 spec
    sha3-256    32             73              FIPS 202
    keccak256   32             74              Keccak
    sha512      64             135             FIPS 180-4
A κ-label MUST satisfy:
- Total length is between 71 and 135 bytes inclusive
- Every byte is in the ASCII printable range (0x21-0x7E)
- Exactly one colon separator exists
- The substring before the colon is a recognized σ-axis token
- The substring after the colon is lowercase hexadecimal digits
- The hex digit count equals twice the digest byte count for the
  token
Construction is fallible. Implementations MUST reject κ-labels
that violate any constraint.
Properties of the κ-label as a type:
- Copyable: no heap allocation, pass by value
- Comparable: bytewise equality and lexicographic ordering
- Hashable: direct use as a hash map key
- Printable: renders as the ASCII string
- Self-describing: the algorithm is readable from the prefix
### 3.3 Tags
A tag is a mutable binding:
    (path, name) → κ-label
The path is a κ-address (§4) without the version component. The
name is a UTF-8 string of 1 to 256 bytes. Tag names MUST NOT
contain the characters `/` (path separator) or `@` (version
delimiter).
Tags are the only mutable state in the protocol. Creating,
updating, or deleting a tag is a mutation that requires ordering
within a consensus domain (§7).
Tag names MAY be ISO 8601 timestamps with nanosecond precision:
    2026-07-20T14:30:00.123456789
When tags are timestamps, the tag list for a path is a timeline.
Temporal queries are range-filtered tag list operations:
    "Value at time T"       → resolve tag named T
    "Values between T1-T2"  → list tags in range [T1, T2]
    "Most recent value"     → list tags, descending, limit 1
A tag snapshot is the set of all tag bindings for a path,
serialized as a blob with its own κ-label. The tag snapshot
captures the mutable state as immutable content at a point in
time. Implementations MAY produce tag snapshots automatically
at configurable intervals.
### 3.4 Edges
An edge expresses a typed directed relationship between two
κ-labels. An edge is a blob. Its content is the canonical
serialization of:
    source      κ-label of the origin blob
    relation    UTF-8 string naming the relationship type
    target      κ-label of the destination blob
    metadata    CBOR-encoded bytes, may be empty
The canonical serialization is deterministic: the same
(source, relation, target, metadata) always produces the same
bytes, which always produces the same κ-label for the edge blob.
The canonical form is defined in Appendix C.
An edge blob is stored in the same store as any other blob. It
is retrievable by its own κ-label. Additionally, the store MUST
maintain an index that allows querying edges by source κ-label,
by target κ-label, by relation type, or by any combination of
these.
The following relation types are conventions defined by this
specification. Any UTF-8 string is a valid relation type; these
are not exhaustive:
    owns          owner → owned
                  GC semantics: target is reachable if source is
    derives-from  child → parent
                  content lineage, schema evolution, tag snapshots
    composed-of   composed → operand
                  provenance from composition operations
    witness-of    witness → witnessed blob
                  links a witness blob to the blob it attests
    schema-for    schema → path scope (encoded in metadata)
                  links a schema blob to its validation scope
    pins          pin blob → protected blob
                  GC semantics: target is a GC root
    refers-to     referrer → subject
                  general reference (OCI referrers equivalent)
    filter-for    filter blob → path scope (encoded in metadata)
                  admission control filter binding
### 3.5 Manifests
A manifest is a blob whose content describes a set of other blobs.
The protocol does not define a manifest format. JSON, CBOR,
protobuf, custom binary, or any other serialization is valid. The
protocol treats a manifest identically to any other blob — it is
stored by κ-label, retrieved by κ-label, verified by re-hash.
The distinction between a manifest and a non-manifest blob is
semantic: the manifest's content contains references to other
κ-labels. The store does not parse manifest content. Applications
parse manifests to discover referenced blobs and resolve them.
Container images, Helm charts, LLM model weight packages, genomic
dataset bundles, etcd state snapshots, and firmware archives are
all manifests in this sense — blobs that describe sets of other
blobs.
A manifest MAY be linked to its referenced blobs by edges
(relation: "refers-to") for graph traversal. This linkage is
OPTIONAL — applications that know how to parse the manifest format
can discover references without edges. Applications that want
format-agnostic graph traversal SHOULD create edges.
### 3.6 Witnesses
A witness is a blob containing the serialized derivation trace of
a κ-label computation. The witness records the steps the
canonicalization pipeline took to produce the κ-label. Replaying
the witness re-derives the κ-label from the trace's intermediate
states, without re-hashing the original content.
A witness blob is linked to the blob it attests by an edge
(relation: "witness-of"). The witness is retrievable by its own
κ-label or discoverable by querying edges inbound to the attested
blob with relation "witness-of".
Witnesses enable offline audit: a verifier can confirm that a
κ-label was correctly derived by replaying the witness, without
access to the original content, the store, or the network.
### 3.7 Schemas
A schema is a blob whose content defines validation rules for
content written to a specific path scope. The protocol does not
prescribe the schema format — OpenAPI v3, JSON Schema, WASM filter
bytecode, or any format that can express accept/reject decisions
is valid.
A schema blob is linked to its path scope by an edge (relation:
"schema-for") with the scope encoded in the edge metadata. When a
schema edge exists for a path, the store MUST validate content
written to that path against the schema before computing the
κ-label. Content that fails validation is rejected. Rejected
content never enters the store and never receives a κ-label.
Schema evolution: updating a schema produces a new schema blob
with a new κ-label. The new schema blob is linked to the old one
by an edge (relation: "derives-from"). Content validated against
the old schema retains its original κ-label. Content written
after the schema update is validated against the new schema.
Schemas are self-hosting. The schema that defines valid schema
structure is itself a blob stored at a well-known path
(Appendix E). Registering a schema is a write to the store like
any other write — the schema blob is stored, the schema-for edge
is created, and a tag binds the schema to a human-readable name.
### 3.8 Pins and Finalizers
A pin is a blob whose content names a κ-label to protect from
garbage collection plus a TTL:
    protected   κ-label of the blob to protect
    ttl         seconds until expiry, 0 for permanent
    controller  optional controller name (makes it a finalizer)
    session     optional upload session UUID (upload protection)
A pin blob is linked to the blob it protects by an edge (relation:
"pins"). The pin κ-label, the edge κ-label, and the protected
blob κ-label are three distinct κ-labels — all are blobs in the
store.
TTL semantics: a pin with TTL > 0 expires after the specified
seconds. Expired pins are equivalent to absent pins for GC
purposes. A pin with TTL = 0 is permanent until explicitly
superseded.
Finalizer semantics: a pin carrying a controller name blocks GC
eviction until a new pin blob from the same controller explicitly
marks it as released. The controller's release is itself a blob,
linked to the original pin by a derives-from edge.
Upload session semantics: a pin carrying a session UUID protects
partially-uploaded content during chunked upload. The session pin
expires on upload completion or session timeout.
Pin history is auditable. Each pin state change (create, expire,
release) produces a new blob. The history is a chain of blobs
linked by derives-from edges.
---
## 4. Addressing
### 4.1 The κ-Address Convention
Every resource in κ-Distribution is addressable by a structured
string that encodes organizational ownership, resource identity,
and version:
    <org-hierarchy> ":" <resource-path> "@" <version>
**Left of the colon: organizational hierarchy.**
DNS-style subdomain depth. Each level to the left increases the
specificity of organizational ownership:
    cern.ch
    atlas.cern.ch
    atlas.cern.ch/calibration
    atlas.cern.ch/calibration/v1
The left side determines consensus domain scope, authorization
boundaries, and registry routing. Deeper hierarchy enables finer
delegation of ownership. The protocol does not constrain depth
— 4, 5, 6, or more levels are valid.
**Right of the colon: resource path.**
Path segments identifying the specific resource within the
organizational scope:
    calibrations/run3/alignment
    datasets/muon-pairs/filtered
    images/production/node
The right side is a hierarchical key. It encodes resource type,
namespace, and name — analogous to the Kubernetes path
`/{resource}/{namespace}/{name}`.
**After @: version.**
A tag name or a κ-label:
    @latest                                 tag name
    @v2.1.0                                 tag name
    @2026-07-20T14:30:00.123456789          timestamp tag
    @sha256:abcdef0123456789...             κ-label (direct)
When the version contains a colon, it is interpreted as a κ-label
and resolved directly from the store without tag lookup. When it
does not contain a colon, it is interpreted as a tag name and
resolved via tag lookup.
**Complete examples:**
    atlas.cern.ch/calibration/v1:calibrations/run3/alignment@latest
    braincraft.io/images/v1:production/rekindle-node@v2.1.0
    iot.fleet.example.com/sensors/v1:pacific/buoy-7@sha256:abc...
    nasa.gov/jpl/deep-space/v1:voyager2/telemetry@2026-01-15T00:00:00
    home.lab/minecraft/v1:servers/survival@latest
### 4.2 Address Resolution
Resolution proceeds in three steps:
    1. Parse the address into org-hierarchy, resource-path,
       and version.
    2. If version contains ':' (κ-label):
         Resolve directly: store.get(κ-label)
       If version does not contain ':' (tag name):
         Resolve via tag: store.tag_get(path, tag) → κ-label
         Then: store.get(κ-label)
    3. Verify on receipt: re-hash the retrieved content, compare
       against the κ-label, reject on mismatch.
The org-hierarchy determines which registry to query. A deployment
MAY configure registry routing by org-hierarchy prefix (e.g., all
addresses starting with `atlas.cern.ch` route to CERN's registry).
The protocol does not prescribe registry discovery — it is a
deployment concern.
### 4.3 The Three Resolution Contexts
**By κ-label (machine-to-machine):**
The κ-label is sufficient. No parsing, no tag lookup, no registry
routing. The requestor provides the κ-label and the store returns
the content. Used in wire frames, dedup caches, audit chains, and
inter-node replication.
**By tag (human-to-machine):**
The requestor provides a path and tag name. The store resolves the
tag to a κ-label and returns the content. Used in CLI tools,
controller reconciliation, and application queries.
**By address (human-to-human):**
The requestor provides a complete κ-address string. The client
library parses the address, determines the registry, resolves
the tag, retrieves the content, and verifies by re-hash. Used
in documentation, SBOMs, attestations, citations, and
cross-system references.
### 4.4 Registries
A registry is any endpoint implementing this protocol. A single
κ-address MAY be resolvable from multiple registries. Content
retrieved from any registry is verified by re-hash — the registry
is untrusted for content integrity.
Registries MAY be:
- A multi-petabyte distributed store backed by a cluster
- A single-node server backed by a local filesystem
- An in-memory store in a test process
- A CDN edge cache serving immutable blobs
- Any system that can respond to the operations in §5
The protocol does not distinguish between these. The operations
and invariants are the same.
### 4.5 Cross-Axis Enrichment
A blob stored under one σ-axis (e.g., blake3) MAY later be
enriched with κ-labels under other axes (e.g., sha256). Enrichment
proceeds:
    1. Retrieve content by the existing κ-label
    2. Re-hash under the requested axis
    3. Store the additional κ-label as an index entry (the content
       is not duplicated — only the index mapping is added)
Enrichment MAY be performed eagerly at ingestion (compute all
configured axes) or lazily on demand (compute when a query arrives
for an unstored axis). The protocol does not prescribe the
enrichment policy.
When a query arrives for a κ-label whose axis is not in the index,
the store MAY:
    a. Return absent (the store does not know that alias)
    b. Search for the content under other axes, re-hash, verify,
       and return (on-demand enrichment)
Option (b) is RECOMMENDED for stores that support multiple axes.
---
## 5. Operations
### 5.1 Blob Operations
**put(κ-label, content) → result**
    Store content at the given κ-label.
    The store MUST verify that the κ-label matches the hash of
    the content. If it does not match, the store MUST reject the
    write.
    If the κ-label already exists and the stored content matches,
    the operation succeeds (idempotent re-put). If the κ-label
    already exists and the stored content differs, the operation
    fails (hash collision — probability 2^-128 for SHA-256).
    If a schema is registered for the path under which this blob
    is being written, the store MUST validate the content against
    the schema before computing the κ-label. If validation fails,
    the write is rejected and no κ-label is assigned.
**get(κ-label) → content | absent**
    Retrieve content by κ-label.
    If the κ-label exists, return the content bytes. If it does
    not exist, return absent. The store MUST NOT return content
    that does not match the κ-label.
**exists(κ-label) → boolean**
    Check existence without retrieving content.
**remove(κ-label) → result**
    Mark the blob as GC-eligible. This does not immediately
    delete the content. Content with outstanding pins, tags, or
    inbound edges from pinned blobs is retained until it becomes
    unreachable.
**list(prefix) → κ-labels**
    Enumerate κ-labels matching a string prefix. The prefix is
    matched against the full κ-label (including the σ-axis
    prefix). The result is paginated.
### 5.2 Tag Operations
**tag_set(path, name, κ-label) → result**
    Bind a tag name to a κ-label at the given path. The blob
    referenced by the κ-label MUST exist in the store before
    the tag can be bound — tags MUST NOT point to absent content.
    If the tag already exists, the binding is atomically replaced.
    tag_set is linearizable within the tag's consensus domain.
**tag_get(path, name) → κ-label | absent**
    Resolve a tag name to a κ-label. Returns the currently
    bound κ-label, or absent if the tag does not exist.
**tag_list(path) → (name, κ-label) pairs**
    List all tags at a path. The result is paginated. Results
    MUST support ordering (ascending or descending by tag name)
    and range filtering (after/before bounds for timestamp tags).
**tag_delete(path, name) → result**
    Remove a tag binding. The content the tag pointed to is NOT
    deleted. Content becomes GC-eligible only when no tags, pins,
    or reachable edges reference it.
### 5.3 Edge Operations
**edge_put(source, relation, target, metadata) → edge-κ-label**
    Create an edge. The store computes the canonical serialization
    of (source, relation, target, metadata), computes its κ-label,
    stores the edge blob, and indexes it by source and target.
    The source blob MUST exist. The target blob MAY be absent
    (lazy replication — the edge is structurally correct even if
    the target has not yet arrived).
    If the exact edge already exists (same canonical form, same
    κ-label), the operation is idempotent.
    Returns the κ-label of the edge blob.
**edge_get(κ-label, relation, direction) → edges**
    Query edges involving a κ-label.
    direction = outbound: edges where κ-label is the source
    direction = inbound:  edges where κ-label is the target
    direction = both:     edges where κ-label is either
    If relation is specified, filter by relation type.
    If relation is omitted, return all edges.
    Each returned edge includes the edge blob's own κ-label,
    the source, the relation, the target, and the metadata.
**edge_remove(edge-κ-label) → result**
    Remove an edge by its κ-label. The edge blob is unpinned
    and becomes GC-eligible. The source and target blobs are
    not affected.
### 5.4 Composition Operations
**compose(operation, operand-κ-labels) → composed-κ-label, witness**
    Apply a categorical composition operation to one or two
    operand κ-labels. The operations:
    g2:   commutative binary product
          canonical form: lexicographically smaller operand first,
          concatenate, hash
    f4:   involution quotient
          canonical form: lexicographic minimum of operand and its
          bitwise complement
    e6:   degree-partition filtration
          canonical form: classify by first digest byte modulo 9
    e7:   S₄-orbit augmentation
          canonical form: lexicographic minimum across all 24
          permutations of the digest's quarter-width segments
    e8:   direct embedding
          canonical form: identity on bytes, distinguished by
          composition type
    The store MUST:
    1. Compute the canonical form per the operation
    2. Hash the canonical form to produce the composed κ-label
    3. Store the composed blob
    4. Create "composed-of" edges from the composed κ-label to
       each operand
    5. Generate and store a witness blob for the composition
    6. Create a "witness-of" edge from the witness to the
       composed blob
    7. Return the composed κ-label and the witness κ-label
### 5.5 Bulk Operations
**put_batch(entries) → results**
    Atomically store multiple blobs. Each entry is a
    (κ-label, content) pair. All entries succeed or all fail.
    Used for multi-blob ingestion where partial writes would
    leave the store inconsistent (e.g., a manifest and its
    referenced blobs).
**tag_set_batch(bindings) → result**
    Atomically set multiple tag bindings within a single
    consensus domain. All bindings succeed or all fail.
    Used for coordinated multi-resource updates where partial
    tag updates would leave the namespace inconsistent.
### 5.6 Streaming Operations
**put_stream(κ-label, reader) → result**
    Store content from a streaming reader. The content is larger
    than available memory. The store MUST compute the κ-label
    incrementally as bytes arrive and verify it against the
    provided κ-label after the stream completes. If the computed
    κ-label does not match, the partially written content is
    discarded.
**get_stream(κ-label) → reader**
    Retrieve content as a streaming reader. The caller reads
    bytes incrementally without buffering the entire blob in
    memory.
### 5.7 Conditional Operations
**tag_set_if(path, name, κ-label, expected-previous) → result**
    Compare-and-swap for tag updates. Sets the tag to the new
    κ-label only if the current tag value matches
    expected-previous. If the current value does not match,
    the operation fails with a conflict error.
    Used by controllers that need optimistic concurrency control
    — read the current tag value, compute the new desired state,
    write back only if the state has not changed since the read.
### 5.8 Invariants
The following invariants MUST hold across all implementations:
1. **Content immutability.** The same κ-label always returns the
   same content. Once stored, content never changes.
2. **Content-before-tag.** A tag MUST NOT be bound to a κ-label
   that does not exist in the store. The store MUST verify
   existence before binding.
3. **Verify-on-put.** The store MUST re-hash content on put and
   verify the computed κ-label matches the provided κ-label.
4. **Idempotent put.** Storing the same content with the same
   κ-label succeeds (no-op).
5. **Edge source existence.** edge_put MUST verify that the
   source κ-label exists.
6. **Edge target absence tolerance.** edge_put MUST accept a
   target κ-label that does not yet exist. The edge is valid;
   the target populates later via replication.
7. **Everything is a blob.** Edges, witnesses, schemas, pins,
   manifests, and filters are blobs. All blob invariants apply
   to them.
---
## 6. Protocol Binding: HTTP
This section defines an HTTP binding for the operations in §5.
Other bindings (gRPC, RDMA, Unix domain socket) are valid. The
HTTP binding is the reference binding for conformance testing.
### 6.1 Version Check
    GET /v2/
    Response: 200 OK
    Body: {"kappa-distribution": "1.1.0"}
A 404 response indicates the endpoint does not implement
κ-Distribution.
### 6.2 Blob Endpoints
**Check existence:**
    HEAD /v2/{path}/blobs/{κ-label}
    Response: 200 OK
    Headers:
        Content-Length: <byte-count>
        X-Kappa-Axis: <σ-axis>
    Response: 404 Not Found
**Retrieve:**
    GET /v2/{path}/blobs/{κ-label}
    Response: 200 OK
    Headers:
        Content-Length: <byte-count>
        X-Kappa-Axis: <σ-axis>
    Body: content bytes
    Response: 404 Not Found
**Push (monolithic):**
    PUT /v2/{path}/blobs/{κ-label}
    Body: content bytes
    Response: 201 Created (new blob stored)
    Response: 200 OK (blob already exists, idempotent)
    Response: 409 Conflict (κ-label does not match content)
    Response: 422 Unprocessable (schema validation failed)
**Push (chunked upload):**
    POST /v2/{path}/blobs/uploads/
    Response: 202 Accepted
    Headers:
        Location: /v2/{path}/blobs/uploads/{session}
    PATCH /v2/{path}/blobs/uploads/{session}
    Headers:
        Content-Range: <start>-<end>
    Body: chunk bytes
    Response: 202 Accepted
    PUT /v2/{path}/blobs/uploads/{session}?kappa={κ-label}
    Body: final chunk (may be empty)
    Response: 201 Created
    Response: 409 Conflict (κ-label does not match assembled content)
On POST, the store MUST create a temporary upload session pin
(§3.8) protecting the partial upload from garbage collection. On
PUT completion or session timeout, the pin is released.
**Unpin:**
    DELETE /v2/{path}/blobs/{κ-label}
    Response: 202 Accepted (GC-eligible)
    Response: 409 Conflict (outstanding finalizer pins)
### 6.3 Tag Endpoints
**Resolve tag or κ-label:**
    GET /v2/{path}/manifests/{version}
    When {version} contains ':' → direct blob retrieval by κ-label
    When {version} has no ':'  → tag lookup, then blob retrieval
    Response: 200 OK
    Headers:
        X-Kappa-Label: <resolved-κ-label>
    Body: content bytes
    Response: 404 Not Found
**Bind tag:**
    PUT /v2/{path}/manifests/{tag}
    Body: content bytes
    Response: 201 Created
    Headers:
        X-Kappa-Label: <computed-κ-label>
    Response: 422 Unprocessable (schema validation failed)
    Response: 409 Conflict (tag_set_if failed — concurrent update)
The store MUST:
1. Validate against registered schema if one exists for the path
2. Compute the κ-label from the content
3. Store the blob (idempotent if it already exists)
4. Bind the tag via consensus
5. Return the κ-label in the response header
**List tags:**
    GET /v2/{path}/tags/list
    Query parameters:
        n:      maximum results (default 100)
        last:   pagination cursor
        order:  "asc" | "desc" (default "asc")
        after:  ISO 8601 lower bound (timestamp tag range filter)
        before: ISO 8601 upper bound (timestamp tag range filter)
    Response: 200 OK
    Body: {"tags": [{"name": "latest", "kappa": "sha256:..."}]}
**Delete tag:**
    DELETE /v2/{path}/manifests/{tag}
    Response: 202 Accepted
    Response: 404 Not Found
### 6.4 Edge Endpoints
**Query edges:**
    GET /v2/{path}/edges/{κ-label}
    Query parameters:
        relation:   filter by relation type
        direction:  "inbound" | "outbound" | "both" (default "outbound")
        n:          maximum results
        last:       pagination cursor
    Response: 200 OK
    Body:
    {
        "edges": [
            {
                "edge_kappa": "sha256:...",
                "source": "sha256:...",
                "relation": "derives-from",
                "target": "sha256:...",
                "metadata": {}
            }
        ]
    }
**Create edge:**
    PUT /v2/{path}/edges/
    Body:
    {
        "source": "sha256:...",
        "relation": "derives-from",
        "target": "sha256:...",
        "metadata": {}
    }
    Response: 201 Created
    Headers:
        X-Kappa-Label: <edge-blob-κ-label>
    Response: 200 OK (edge already exists, idempotent)
**Delete edge:**
    DELETE /v2/{path}/edges/{edge-κ-label}
    Response: 202 Accepted
    Response: 404 Not Found
### 6.5 Witness Endpoint
    GET /v2/{path}/witnesses/{κ-label}
    Response: 200 OK
    Body: witness blob content
    Response: 404 Not Found
The κ-label parameter is the κ-label of the witnessed blob, not
the witness blob itself. The store resolves the witness by querying
edges inbound to the given κ-label with relation "witness-of".
### 6.6 Composition Endpoint
    POST /v2/{path}/compose/{operation}
    Body: {"operands": ["sha256:...", "sha256:..."]}
    Response: 200 OK
    Body:
    {
        "composed": "sha256:...",
        "witness": "sha256:...",
        "operands": ["sha256:...", "sha256:..."],
        "operation": "g2"
    }
### 6.7 Schema Endpoints
**Register schema:**
    PUT /v2/{path}/schemas/{scope}
    Body: schema content (format determined by content-type)
    Response: 201 Created
    Headers:
        X-Kappa-Label: <schema-blob-κ-label>
    Response: 200 OK (schema exists, κ-label matches)
The store MUST create a "schema-for" edge from the schema κ-label
to the scope, and tag the schema at a well-known path.
**Retrieve schema:**
    GET /v2/{path}/schemas/{scope}
    Response: 200 OK
    Body: schema content
    Headers:
        X-Kappa-Label: <schema-blob-κ-label>
    Response: 404 Not Found
**List schemas:**
    GET /v2/{path}/schemas/
    Response: 200 OK
    Body: {"schemas": [{"scope": "...", "kappa": "sha256:..."}]}
### 6.8 Discovery Endpoint
    GET /v2/_discovery/
    Response: 200 OK
    Body:
    {
        "hierarchies": [
            {
                "org": "atlas.cern.ch/calibration/v1",
                "resources": ["calibrations", "alignments"]
            }
        ]
    }
Returns the set of org-hierarchies and resource paths that have
content. Analogous to `kubectl api-resources`.
### 6.9 Garbage Collection Endpoints
**Pin:**
    POST /v2/{path}/gc/pin
    Body: {"kappa": "sha256:...", "ttl": 3600, "controller": ""}
    Response: 201 Created
    Headers:
        X-Kappa-Label: <pin-blob-κ-label>
**Unpin:**
    POST /v2/{path}/gc/unpin
    Body: {"pin_kappa": "sha256:..."}
    Response: 200 OK
    Response: 409 Conflict (outstanding finalizer)
**Trigger sweep:**
    POST /v2/{path}/gc/sweep
    Response: 202 Accepted
    Body: {"sweep_id": "<uuid>"}
**Sweep status:**
    GET /v2/{path}/gc/status
    Response: 200 OK
    Body:
    {
        "last_sweep": "2026-07-20T14:00:00Z",
        "duration_seconds": 90,
        "objects_scanned": 1000000,
        "objects_evicted": 42000,
        "bytes_reclaimed": 1073741824,
        "pending_finalizers": [
            {"kappa": "sha256:...", "controller": "calibration-ctrl"}
        ]
    }
### 6.10 Health Endpoints
    GET /v2/_health/live      200 if process alive
    GET /v2/_health/ready     200 if backend connected, consensus available
    GET /v2/_health/startup   200 if initialization complete
### 6.11 Admission Filter Endpoints
**Register filter:**
    PUT /v2/{path}/filters/{scope}
    Body: filter content (WASM bytecode, executable, etc.)
    Response: 201 Created
    Headers:
        X-Kappa-Label: <filter-blob-κ-label>
The store MUST create a "filter-for" edge from the filter κ-label
to the scope. The filter activates immediately for all subsequent
writes matching the scope.
**List filters:**
    GET /v2/{path}/filters/
    Response: 200 OK
    Body: {"filters": [{"scope": "...", "kappa": "sha256:..."}]}
**Remove filter:**
    DELETE /v2/{path}/filters/{filter-κ-label}
    Response: 202 Accepted
The filter blob is not deleted — it is unlinked from the scope.
The blob remains in the store for audit.
**Filter interface:**
A filter receives content bytes and returns accept or reject:
    input:   content bytes (the candidate blob)
    output:  { "accept": true } | { "accept": false, "reason": "..." }
Multiple filters MAY be registered for the same scope. All
registered filters MUST accept for the write to proceed. If any
filter rejects, the write is rejected. If a filter execution
fails (crash, timeout), the write is rejected (fail closed).
The filter runtime is not prescribed. WASM, native shared object,
interpreted script, or external HTTP call are all valid. The
protocol defines the interface, not the execution environment.
### 6.12 Endpoint Summary
    Method  Path                                    Operation           Consensus
    ------  ----                                    ---------           ---------
    GET     /v2/                                    version check       no
    HEAD    /v2/{p}/blobs/{κ}                       blob exists         no
    GET     /v2/{p}/blobs/{κ}                       blob retrieve       no
    PUT     /v2/{p}/blobs/{κ}                       blob push           no
    POST    /v2/{p}/blobs/uploads/                  upload start        no
    PATCH   /v2/{p}/blobs/uploads/{s}               upload chunk        no
    PUT     /v2/{p}/blobs/uploads/{s}?kappa={κ}     upload complete     no
    DELETE  /v2/{p}/blobs/{κ}                       blob unpin          no
    GET     /v2/{p}/manifests/{v}                   tag resolve         no
    PUT     /v2/{p}/manifests/{t}                   tag bind            yes
    DELETE  /v2/{p}/manifests/{t}                   tag delete          yes
    GET     /v2/{p}/tags/list                       tag list            no
    GET     /v2/{p}/edges/{κ}                       edge query          no
    PUT     /v2/{p}/edges/                          edge create         no
    DELETE  /v2/{p}/edges/{κ}                       edge delete         no
    GET     /v2/{p}/witnesses/{κ}                   witness get         no
    POST    /v2/{p}/compose/{op}                    compose             no
    PUT     /v2/{p}/schemas/{scope}                 schema register     yes
    GET     /v2/{p}/schemas/{scope}                 schema get          no
    GET     /v2/{p}/schemas/                        schema list         no
    GET     /v2/_discovery/                         discovery           no
    POST    /v2/{p}/gc/pin                          GC pin              no
    POST    /v2/{p}/gc/unpin                        GC unpin            no
    POST    /v2/{p}/gc/sweep                        GC sweep            no
    GET     /v2/{p}/gc/status                       GC status           no
    GET     /v2/_health/live                        liveness            no
    GET     /v2/_health/ready                       readiness           no
    GET     /v2/_health/startup                     startup             no
    PUT     /v2/{p}/filters/{scope}                 filter register     yes
    GET     /v2/{p}/filters/                        filter list         no
    DELETE  /v2/{p}/filters/{κ}                     filter remove       yes
    Total: 31 endpoints. 4 require consensus. 27 do not.
---
## 7. Consistency and Consensus
### 7.1 Content Consistency
Content is eventually consistent across replicas. A blob stored
at one registry becomes available at other registries through
replication (§8). Because content is immutable and addressed by
κ-label, any replica serves any blob. Clients verify by re-hash.
Content is infinitely cacheable — CDN, edge cache, local disk
cache, in-memory cache. A cached blob never goes stale because
its κ-label is deterministic and its content is immutable.
### 7.2 Tag Consistency
Tags are sequentially consistent within a consensus domain. All
clients observing the same consensus domain see tag updates in the
same order. If client A observes tag T₁ before T₂, all clients in
the same domain observe T₁ before T₂ or do not observe T₁.
### 7.3 Consensus Domains
The protocol requires that consensus domains exist. Each tag
belongs to exactly one consensus domain. tag_set and tag_delete
are linearizable within the tag's domain.
The protocol does not prescribe how consensus domains are scoped.
Deployments MAY scope by org-hierarchy, by resource path, by
geographic region, or by any other partition. The scoping decision
determines the contention surface — tags in different domains
update independently without coordination.
### 7.4 Consensus Interface
The protocol requires that the consensus mechanism provides:
    linearizable tag_set(path, name, κ-label)
    linearizable tag_delete(path, name)
    linearizable tag_set_if(path, name, κ-label, expected)
The protocol does not prescribe the consensus implementation.
Raft, CASPaxos, DARE, Mu, a single-node mutex, or an external
coordination service are all valid. The consensus mechanism is a
deployment choice.
### 7.5 Content-Before-Tag Guarantee
Blobs MUST exist before tags can reference them. Blob durability
is confirmed before the tag update is submitted to consensus.
Combined guarantee: once a tag update is acknowledged by consensus,
the referenced content is readable from the local store.
---
## 8. Replication and Federation
### 8.1 Replication Model
Content replicates between registries via whatever mechanism the
deployment provides. The protocol does not define a replication
transport. It defines the invariant: content received via
replication MUST be verified by re-hash against its κ-label. The
replication transport is untrusted.
### 8.2 Metadata-Before-Content
Tags and edges MAY replicate before the blobs they reference.
A registry that receives a tag pointing to an absent κ-label MUST
accept the tag binding. Content requests for the absent κ-label
return absent. The content populates later via continued
replication or on-demand federation fetch.
### 8.3 Federation Fetch
When a registry does not hold a requested κ-label, it MAY query
peer registries. Peers MAY relay the query to their own peers.
This enables multi-hop resolution across networks where the
requesting registry cannot directly reach the registry holding
the content.
Multi-hop resolution:
    A requests κ from A's store → absent
    A queries peer B → B does not have κ
    B queries peer C (A cannot reach C) → C has κ
    C returns content to B
    B verifies by re-hash
    B caches the content locally
    B returns content to A
    A verifies by re-hash
    A caches the content locally
    A returns content to the client
Every hop verifies by re-hash. A corrupted or tampered relay is
detected at the next hop. The final client verifies independently
of all intermediate hops.
### 8.4 Conflict Resolution
When two registries independently update the same tag within the
replication window, both updates propagate to all registries.
A receiving registry detects a conflict when an arriving tag
update's previous value does not match the local current value.
Resolution is deterministic and requires no cross-registry
coordination:
    1. Compare HLC timestamps of the conflicting updates
    2. The update with the strictly higher timestamp wins
    3. If timestamps are equal: the update with the
       lexicographically greater κ-label wins
Properties:
- Deterministic: all registries arrive at the same winner
- Commutative: order of update reception does not matter
- Idempotent: applying the same update twice has no effect
Both the winning and losing κ-labels remain in the store. Only
the tag pointer resolves. Applications MAY observe conflict events
to detect divergent writes.
### 8.5 Long-Duration Disconnection
The protocol makes no assumption about maximum disconnection
duration. A registry offline for seconds, hours, days, weeks, or
months reconnects and synchronizes through the same mechanism:
    1. Tags accumulated during disconnection replicate
    2. Conflicts resolve deterministically (§8.4)
    3. Content backfills on demand via federation fetch (§8.3)
    4. No manual intervention required
An Antarctic sensor station offline for six months and a CDN edge
node offline for six seconds use the same reconnection protocol.
The consistency model (§7) and conflict resolution (§8.4) are
designed for arbitrary replication delay.
---
## 9. Garbage Collection
### 9.1 Roots
The GC roots are:
- Every κ-label that is the target of a pin edge whose pin has
  not expired (permanent pins, unexpired TTL pins, finalizer pins)
- Every κ-label that is bound by a tag
### 9.2 Reachability
A κ-label is reachable if:
- It is a root (pinned or tagged), OR
- It is the target of an "owns" edge from a reachable κ-label, OR
- It is the target of a "composed-of" edge from a reachable
  κ-label
Edge blobs are reachable if either their source or their target
is reachable.
All other κ-labels are GC-eligible.
### 9.3 Sweep Algorithm
    1. Snapshot the set of all roots (pins + tags) at sweep
       start time
    2. Walk edges from roots along "owns" and "composed-of"
       relations (breadth-first)
    3. Mark all visited κ-labels as reachable
    4. For each edge blob: mark as reachable if source or
       target is reachable
    5. For all κ-labels not marked reachable:
       a. Remove associated edges
       b. Remove the blob from the store
       c. Record the eviction in the audit log
### 9.4 Snapshot Isolation
Pins created after the sweep snapshot protect their subgraph from
the NEXT sweep, not the current one. This prevents a race
condition where a newly-pinned subgraph is partially evicted
before the sweep discovers the new pin.
Upload session pins created before the snapshot protect partial
uploads during the current sweep.
### 9.5 Parallelism
Sweeps MAY partition the κ-label space by any sharding dimension:
hash prefix, window index, namespace prefix, or any other
partition. Independent partitions sweep in parallel. A partition's
sweep considers only κ-labels in that partition plus cross-
partition edge traversals (which read but do not modify the
neighbor partition).
### 9.6 Finalizers
A κ-label with outstanding finalizer pins is never evicted, even
if it is otherwise unreachable. Stuck finalizers — finalizers
whose controller has not released them — are reported in the GC
status endpoint. Operators or recovery controllers clear stuck
finalizers manually.

### 9.7 Sweep Scheduling
The protocol does not prescribe sweep scheduling. Deployments
choose sweep frequency based on their churn rate and capacity
constraints. A store with 1% daily churn benefits from daily
sweeps. A store with 10% hourly churn benefits from hourly
sweeps per partition.

Sweep cost is proportional to the number of κ-labels in the
partition being swept, not to the total store size. Partitioning
by hash prefix, namespace, or window index bounds sweep cost to
a fraction of the total object count.

A sweep MUST NOT block content reads or writes. Sweeps operate
on a snapshot of the root set (§9.4) and do not hold locks on
content or tag state during traversal. Eviction of unreachable
blobs MAY be batched and applied asynchronously after the
traversal completes.
---
## 10. Admission Control
### 10.1 Filter Model
An admission filter is a blob. Its content is executable logic
that evaluates candidate content on write. The filter is linked
to a path scope by a "filter-for" edge. The filter receives
content bytes and returns accept or reject with a reason string.
### 10.2 Filter Matching
Filters are scoped by path prefix. A filter registered at scope
`atlas.cern.ch/calibration/v1:calibrations/` applies to all writes
under that prefix. Multiple filters MAY match a single write path.
All matching filters MUST accept for the write to proceed.
### 10.3 Filter Execution
If any filter rejects, the write is rejected. The rejection reason
is returned to the client. The rejected content does not enter
the store and does not receive a κ-label.
If a filter execution fails (crash, timeout, resource exhaustion),
the write is rejected. Fail closed. The failure is logged.
### 10.4 Filter Lifecycle
Filters are blobs. Updating a filter is storing a new filter blob
and updating the filter tag (consensus-protected). The old filter
blob remains in the store for audit. The new filter activates on
tag update.
Filter deployment cost is mitigated by horizontal sharding of
consensus domains. A filter that runs on every write in a
high-throughput namespace adds per-write latency proportional to
filter complexity. Deployments SHOULD shard high-throughput
namespaces into separate consensus domains to bound the blast
radius.
---
## 11. Security Considerations
### 11.1 Verify-on-Receipt
κ-labels provide content integrity independent of transport
security and server trust. A client that retrieves a blob and
re-hashes it detects any corruption, tampering, or substitution
regardless of how the blob arrived. This property is stronger
than transport security: TLS verifies the transport; κ-labels
verify the content.
### 11.2 Tag Integrity
Tags are mutable and consensus-protected. Redirecting a tag
requires compromising a consensus quorum. Deployments requiring
stronger tag integrity SHOULD use signed tags: the tag value is
a κ-label of a signed manifest containing the content κ-label and
the signer's identity. Verifying the signature verifies the tag
binding independently of consensus integrity.
### 11.3 Edge Integrity
Edges are blobs with κ-labels. A tampered edge is detectable by
re-hashing the edge blob and comparing against its κ-label.
Composition edges carry witness blobs that replay the composition
derivation. Injecting a false composition edge requires forging
a witness — the verifier detects the forgery by replaying it.
### 11.4 Pin Manipulation
Pin creation and deletion MUST be authorized. Unauthorized pin
creation enables GC denial-of-service (pin flooding). Unauthorized
pin deletion enables premature eviction. Pin operations SHOULD be
rate-limited per identity.
### 11.5 Authorization
The protocol requires that operations are authorized before
execution. It does not define the authorization framework. It
defines the authorization scope: the org-hierarchy and resource
path. An identity authorized at a given scope can operate within
it. An identity not authorized MUST receive a rejection.
### 11.6 Encryption
Encryption is not defined by this protocol. Content is stored as
provided by the client. If encryption at rest is desired, the
deployment provides it (block-level, filesystem-level, or
application-level). If content-level encryption is desired, the
client MUST encrypt before computing the κ-label — the κ-label
covers the ciphertext, not the plaintext.

### 11.7 Content Provenance
A κ-label is intrinsic provenance. The κ-label proves that the
content existed in its exact form at the time the κ-label was
computed. Combined with a witness (§3.6), the κ-label proves
how the canonical form was derived. Combined with a signed tag
(§11.2), the κ-label proves who asserted its identity. Combined
with a composition witness (§5.4), the κ-label proves which
operands produced the composed value.

Content provenance does not require external attestation services.
The κ-label, the witness, and the edge graph together form a
self-contained provenance chain that any party can verify
independently.

### 11.8 Replay Protection
A client that receives a blob and verifies its κ-label knows the
content is authentic but does not know whether it is current. An
attacker who has access to old blobs can serve them in response
to requests. The client receives a valid blob (correct κ-label)
but an outdated one.

Mitigation: access content through tags, not raw κ-labels, for
any use case where freshness matters. The tag resolution path
(§4.2) returns the currently bound κ-label. The tag binding is
consensus-protected and reflects the most recent state.

For use cases where the κ-label is the only identifier (e.g.,
inter-node replication), replay protection is provided by the
replication protocol and the conflict resolution mechanism (§8.4)
— the tag with the higher timestamp wins, so an old blob served
in place of a new one is overridden when the correct tag update
arrives.

### 11.9 Multi-Hop Trust
In federation fetch (§8.3), content traverses multiple registries
before reaching the requesting client. Each hop verifies by
re-hash. A compromised intermediate registry cannot tamper with
content without the next hop detecting the mismatch. This provides
end-to-end content integrity across untrusted relay chains without
requiring a pre-established trust relationship between the
requesting registry and the content-holding registry.

The relay chain does not provide confidentiality. Each relay sees
the content in cleartext. Deployments requiring confidentiality
across untrusted relays MUST encrypt content before computing the
κ-label (§11.6).
---
## 12. Failure Modes
### 12.1 Node Down
Content uniquely held by a down node is temporarily unavailable.
Federation fetch from peers resolves it if any peer holds a copy.
If no peer holds a copy, the content is unavailable until the node
recovers. Tag reads return the last committed value. Content reads
for available κ-labels continue normally.
### 12.2 Consensus Quorum Lost
Tag updates in the affected consensus domain stall. Content reads
continue — content does not require consensus. Tag reads return
the last committed value. The domain recovers when quorum is
restored.
### 12.3 Corrupted Blob
A blob whose content does not match its κ-label is corrupt.
Detection: any get operation that re-hashes on retrieval detects
corruption. Recovery: federation fetch retrieves an uncorrupted
copy from a peer. The corrupt blob is evicted and replaced.
### 12.4 Split-Brain Partition
Registries on opposite sides of a network partition operate
independently. Tags diverge. On partition heal, deterministic
conflict resolution (§8.4) converges all registries to the same
state. Both values survive in the store. Only the tag pointers
resolve.
### 12.5 GC Race
Snapshot isolation (§9.4) prevents. A blob pinned after the sweep
snapshot is protected on the next sweep. An upload session pinned
before the snapshot is protected during the current sweep.
### 12.6 Stuck Finalizer
A finalizer whose controller has crashed or is permanently
unavailable blocks eviction of its protected blob indefinitely.
Detection: the GC status endpoint reports outstanding finalizers.
Resolution: an operator or recovery controller removes the
finalizer pin.
### 12.7 Federation Fetch Exhaustion
All reachable peers queried, none holds the content. The store
returns absent to the client. No partial content is returned. The
client retries with a different set of peers, waits for
replication to deliver the content, or fails.
### 12.8 Admission Filter Failure
A filter execution that crashes, times out, or exceeds resource
limits causes the write to be rejected. Fail closed. The filter
failure is logged. The content is not stored. The client receives
a rejection with the failure reason.
### 12.9 Cross-Axis Resolution Failure
Content stored under axis A is requested under axis B. On-demand
re-hash requires retrieving the content, re-hashing under axis B,
and verifying the result. If the content is unavailable (node
down, not yet replicated), cross-axis resolution fails. The client
receives absent. Resolution: wait for the content to become
available, then retry.
---
## 13. Conformance
### 13.1 Conformance Levels
    Level 1: Blob operations
             put, get, exists, remove, list
             Sufficient for a minimal content-addressed store.
    Level 2: Level 1 + Tag operations
             tag_set, tag_get, tag_list, tag_delete
             Sufficient for a mutable-tag content store.
    Level 3: Level 2 + Edge operations
             edge_put, edge_get, edge_remove
             Edges are blobs — all Level 1 invariants apply.
             Sufficient for a content store with relationships.
    Level 4: Level 3 + Composition, Witnesses, Schemas
             compose, witness storage, schema registration
             and validation
             Sufficient for a verified content store.
    Level 5: Level 4 + GC, Admission, Federation, Multi-hop
             sweep, pins, finalizers, filter registration
             and execution, federation fetch, multi-hop relay
             Sufficient for a production distributed store.
### 13.2 Conformance Test Harness
The conformance test harness is implemented against the operations
defined in §5. The implementor provides the operation
implementations. The harness exercises each conformance level
with the test vectors in §13.3 and reports pass/fail per test
per level.
The harness is backend-independent. It does not assume a
particular storage engine, filesystem layout, or network topology.
It tests behavior at the operation interface.
### 13.3 Test Vectors
**Blob vectors (Level 1):**
    Input: empty byte array (0 bytes)
    Expected κ-label (sha256):
        sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
    Expected κ-label (blake3):
        blake3:af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262
    Input: UTF-8 "hello" (5 bytes)
    Expected κ-label (sha256):
        sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
    Expected κ-label (blake3):
        blake3:ea8f163db38682925e4491c5e58d4bb3506ef8c14eb78a86e908c5624a67200f
    Test: put(κ, content) → get(κ) → content matches
    Test: put(κ, content) → put(κ, content) → idempotent success
    Test: put(κ, wrong_content) → rejected (verify-on-put)
    Test: put(κ, content) → exists(κ) → true
    Test: exists(absent_κ) → false
    Test: get(absent_κ) → absent
    Test: remove(κ) → GC-eligible (not immediately absent)
    Test: list("sha256:2c") → returns κ-labels with matching prefix
**Tag vectors (Level 2):**
    Test: tag_set(path, "latest", κ) → tag_get(path, "latest") → κ
    Test: tag_set(path, "latest", κ₂) → tag_get returns κ₂
    Test: tag_set(path, "v1", absent_κ) → rejected (content-before-tag)
    Test: tag_delete(path, "latest") → tag_get returns absent
    Test: tag_list(path) → returns all (name, κ) pairs
    Test: tag_set_if(path, "latest", κ₂, κ₁) → succeeds when current is κ₁
    Test: tag_set_if(path, "latest", κ₂, wrong_κ) → fails (conflict)
**Edge vectors (Level 3):**
    Test: edge_put(κ_A, "derives-from", κ_B, {}) → edge-κ returned
    Test: edge_get(κ_A, "derives-from", outbound) → includes edge to κ_B
    Test: edge_get(κ_B, "derives-from", inbound) → includes edge from κ_A
    Test: edge_get(κ_A, "owns", outbound) → empty (wrong relation)
    Test: edge_put(κ_A, "owns", absent_κ) → succeeds (target may be absent)
    Test: edge_put(absent_κ, "owns", κ_B) → rejected (source must exist)
    Test: edge blob is retrievable by its own κ-label via get()
**GC vectors (Level 5):**
    Setup: put(κ_A), put(κ_B), edge_put(κ_A, "owns", κ_B), pin(κ_A)
    Sweep: κ_A and κ_B survive (B reachable via owns)
    Unpin κ_A, sweep: both evicted (neither pinned, no tags)
    Setup: put(κ_C), tag_set(path, "v1", κ_C)
    Sweep: κ_C survives (tagged)
    tag_delete(path, "v1"), sweep: κ_C evicted (unreachable)
    Setup: put(κ_D), pin(κ_D, controller="ctrl")
    Sweep: κ_D survives (finalizer)
    Unpin: rejected (outstanding finalizer)
    Release finalizer: unpin succeeds, next sweep evicts
**Composition vectors (Level 4):**
    Operands: κ_A, κ_B
    compose("g2", [κ_A, κ_B]) → κ_C
    compose("g2", [κ_B, κ_A]) → κ_C (commutative)
    edge_get(κ_C, "composed-of", outbound) → includes κ_A and κ_B
    get(κ_C) → retrievable (composed blob stored)
    edge with relation "witness-of" exists inbound to κ_C
**Conflict vectors (Level 5):**
    Site A: tag_set(path, "latest", κ_1) at timestamp T1
    Site B: tag_set(path, "latest", κ_2) at timestamp T2
    T2 > T1: all sites converge to κ_2
    T1 == T2: all sites converge to max(κ_1, κ_2) lexicographically
    Both κ_1 and κ_2 remain in the store as blobs
---
## Appendix A: Complete HTTP Endpoint Table
    Method  Path                                    Operation           Cons.  Level
    ------  ----                                    ---------           -----  -----
    GET     /v2/                                    version check       no     1
    HEAD    /v2/{p}/blobs/{κ}                       blob exists         no     1
    GET     /v2/{p}/blobs/{κ}                       blob retrieve       no     1
    PUT     /v2/{p}/blobs/{κ}                       blob push           no     1
    POST    /v2/{p}/blobs/uploads/                  upload start        no     1
    PATCH   /v2/{p}/blobs/uploads/{s}               upload chunk        no     1
    PUT     /v2/{p}/blobs/uploads/{s}?kappa={κ}     upload complete     no     1
    DELETE  /v2/{p}/blobs/{κ}                       blob unpin          no     1
    GET     /v2/{p}/manifests/{v}                   tag resolve         no     2
    PUT     /v2/{p}/manifests/{t}                   tag bind            yes    2
    DELETE  /v2/{p}/manifests/{t}                   tag delete          yes    2
    GET     /v2/{p}/tags/list                       tag list            no     2
    GET     /v2/{p}/edges/{κ}                       edge query          no     3
    PUT     /v2/{p}/edges/                          edge create         no     3
    DELETE  /v2/{p}/edges/{κ}                       edge delete         no     3
    GET     /v2/{p}/witnesses/{κ}                   witness get         no     4
    POST    /v2/{p}/compose/{op}                    compose             no     4
    PUT     /v2/{p}/schemas/{scope}                 schema register     yes    4
    GET     /v2/{p}/schemas/{scope}                 schema get          no     4
    GET     /v2/{p}/schemas/                        schema list         no     4
    GET     /v2/_discovery/                         discovery           no     2
    POST    /v2/{p}/gc/pin                          GC pin              no     5
    POST    /v2/{p}/gc/unpin                        GC unpin            no     5
    POST    /v2/{p}/gc/sweep                        GC sweep            no     5
    GET     /v2/{p}/gc/status                       GC status           no     5
    GET     /v2/_health/live                        liveness            no     1
    GET     /v2/_health/ready                       readiness           no     1
    GET     /v2/_health/startup                     startup             no     1
    PUT     /v2/{p}/filters/{scope}                 filter register     yes    5
    GET     /v2/{p}/filters/                        filter list         no     5
    DELETE  /v2/{p}/filters/{κ}                     filter remove       yes    5
    31 endpoints. 5 require consensus. 26 do not.
---
## Appendix B: κ-Address Formal Grammar
    kappa-address   = org-hierarchy ":" resource-path "@" version
    org-hierarchy   = org-segment *("." org-segment) *("/" path-segment)
    org-segment     = 1*(ALPHA / DIGIT / "-")
    path-segment    = 1*(ALPHA / DIGIT / "-" / "_")
    resource-path   = path-segment *("/" path-segment)
    version         = tag-name / kappa-label
    tag-name        = 1*256(UTF8-char) ; excludes "/" and "@"
    kappa-label     = axis-token ":" hex-digest
    axis-token      = "sha256" / "blake3" / "sha3-256" / "keccak256" / "sha512"
    hex-digest      = *(HEXDIG)        ; length determined by axis-token
Examples parsed:
    atlas.cern.ch/calibration/v1:calibrations/run3/alignment@latest
    ├─ org-hierarchy: atlas.cern.ch/calibration/v1
    │    ├─ org-segment: atlas
    │    ├─ org-segment: cern
    │    ├─ org-segment: ch
    │    ├─ path-segment: calibration
    │    └─ path-segment: v1
    ├─ resource-path: calibrations/run3/alignment
    │    ├─ path-segment: calibrations
    │    ├─ path-segment: run3
    │    └─ path-segment: alignment
    └─ version: latest (tag-name)
    iot.fleet.example.com/sensors/v1:pacific/buoy-7@sha256:abc...
    ├─ org-hierarchy: iot.fleet.example.com/sensors/v1
    ├─ resource-path: pacific/buoy-7
    └─ version: sha256:abc... (κ-label, direct resolution)
---
## Appendix C: Edge Blob Canonical Form
The canonical serialization of an edge produces deterministic
bytes for κ-label computation. The serialization is:
    field           encoding
    -----           --------
    source          κ-label as UTF-8 bytes
    separator_1     single byte 0x00
    relation        UTF-8 bytes
    separator_2     single byte 0x00
    target          κ-label as UTF-8 bytes
    separator_3     single byte 0x00
    metadata_len    u32 big-endian (length of metadata bytes)
    metadata        CBOR-encoded bytes (RFC 8949 §4.2 deterministic)
The null byte separators ensure unambiguous parsing (κ-labels and
relation strings do not contain null bytes). The metadata uses
deterministic CBOR encoding (sorted keys, minimal integer width)
to ensure the same metadata always produces the same bytes.
Example:
    source:   sha256:aaaa...   (71 bytes)
    relation: derives-from     (12 bytes)
    target:   sha256:bbbb...   (71 bytes)
    metadata: {}               (1 byte CBOR: 0xA0)
    canonical: [71 bytes][0x00][12 bytes][0x00][71 bytes][0x00][0x00 0x00 0x00 0x01][0xA0]
    total: 71 + 1 + 12 + 1 + 71 + 1 + 4 + 1 = 162 bytes
    edge κ-label: sha256:hash(canonical)
---
## Appendix D: Composition Canonical Forms
**CS-G2 — Commutative Binary Product:**
    input:   κ-label A, κ-label B (equal width N)
    canonical: if A ≤ B lexicographically: A || B
               else: B || A
    output:  hash(canonical) → composed κ-label
    property: g2(A, B) == g2(B, A)
**CS-F4 — Involution Quotient:**
    input:   κ-label A (width N)
    canonical: decode digest, compute bitwise complement,
               take lexicographic minimum of digest and complement,
               re-encode with axis prefix
    output:  hash(canonical) → composed κ-label
    property: f4(A) == f4(A') where A' has complement digest
**CS-E6 — Degree-Partition Filtration:**
    input:   κ-label A (width N)
    canonical: tag byte = 0x05 if first digest byte % 9 ∈ [0,7],
               0x06 if first digest byte % 9 == 8.
               [tag] || A as bytes
    output:  hash(canonical) → composed κ-label
    property: 8:1 population ratio
**CS-E7 — S₄-Orbit Augmentation:**
    input:   κ-label A (digest divisible by 4)
    canonical: divide digest into 4 quarters, enumerate all 24
               permutations of S₄, take lexicographic minimum
               across all permutations, re-encode with axis prefix
    output:  hash(canonical) → composed κ-label
    property: any quarter permutation produces the same result
**CS-E8 — Direct Embedding:**
    input:   κ-label A
    canonical: A as bytes (identity)
    output:  hash(canonical) → composed κ-label
    property: distinguished from operand by composition type marker
---
## Appendix E: Schema Self-Hosting Definition
The schema that validates schema blobs is itself a blob stored at:
    _system/schemas/v1:definition@latest
Its content is a JSON structural schema:
    {
        "type": "object",
        "required": ["scope", "format", "validation"],
        "properties": {
            "scope": {
                "type": "string",
                "description": "Path prefix this schema applies to"
            },
            "format": {
                "type": "string",
                "description": "Schema format identifier",
                "enum": ["openapi-v3", "json-schema", "wasm", "native"]
            },
            "validation": {
                "description": "The validation rules, format-dependent"
            }
        }
    }
This schema has a κ-label. It is linked to its scope
(`_system/schemas/`) by a "schema-for" edge. It validates itself.
---
## Appendix F: OCI Distribution Compatibility
This appendix is informative. It describes how implementors MAY
provide OCI Distribution v1.1.1 compatibility as a frontend to a
κ-Distribution store.
**Mapping:**
    OCI concept              κ-Distribution equivalent
    -----------              -------------------------
    digest (sha256:...)      κ-label (sha256:...) — identical string
    blob                     blob
    manifest                 blob (content references other κ-labels)
    tag                      tag
    referrers API            edge query (relation: "refers-to", inbound)
    catalog                  discovery endpoint
**OCI path translation:**
    OCI: /v2/{name}/blobs/{digest}
    κ:   /v2/_oci/{name}/blobs/{κ-label}
The `_oci` prefix is a reserved org-hierarchy for OCI-native
content not using CRD-style addressing.
**Compatibility constraints:**
- OCI SHA-256 digests are byte-identical to sha256 κ-labels
- OCI manifests are blobs whose content-type is an OCI media type
- OCI tag resolution is tag_get on the `_oci/{name}` path
- OCI referrers response is an OCI Image Index assembled from
  edge query results with relation "refers-to"
OCI clients (Docker, containerd, Podman, ORAS, Helm) interact
with the OCI frontend and see a conformant OCI registry.
κ-Distribution clients interact with the κ-native API and see
the full feature set. Both access the same blobs in the same
store.
---
## Appendix G: CLI Experience Reference
This appendix is informative. It illustrates the user experience
through a hypothetical CLI tool (`kap`).
**Push a blob:**
    $ echo "hello world" | kap push atlas.cern.ch/data/v1:datasets/test
    sha256:b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9
**Tag it:**
    $ kap tag atlas.cern.ch/data/v1:datasets/test@v1.0 sha256:b94d27b...
**Pull by tag:**
    $ kap pull atlas.cern.ch/data/v1:datasets/test@v1.0
    hello world
**Pull by κ-label (no tag resolution):**
    $ kap pull sha256:b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9
    hello world
**List tags:**
    $ kap tags atlas.cern.ch/data/v1:datasets/test
    v1.0    sha256:b94d27b...
    latest  sha256:b94d27b...
**Create an edge:**
    $ kap edge create sha256:aaa... derives-from sha256:bbb...
    edge sha256:ccc...
**Query edges:**
    $ kap edges sha256:aaa... --relation derives-from
    sha256:aaa... --[derives-from]--> sha256:bbb...
**Compose:**
    $ kap compose g2 sha256:aaa... sha256:bbb...
    composed: sha256:ddd...
    witness:  sha256:eee...
**GC sweep:**
    $ kap gc sweep atlas.cern.ch/data/v1:datasets/
    sweep started: 8f3a-...
    $ kap gc status atlas.cern.ch/data/v1:datasets/
    last sweep: 2026-07-20T14:00:00Z, 42000 objects evicted
**Schema registration:**
    $ kap schema register atlas.cern.ch/data/v1:datasets/ schema.json
    schema: sha256:fff...
**Push with schema validation:**
    $ kap push atlas.cern.ch/data/v1:datasets/test invalid.json
    error: schema validation failed: missing required field "run_number"
**Admission filter:**
    $ kap filter register atlas.cern.ch/data/v1:datasets/ filter.wasm
    filter: sha256:ggg...
---
## Appendix H: Multi-Hop Federation Worked Example
Three registries: A, B, C. A can reach B. B can reach C.
A cannot reach C.
    content blob κ = sha256:xyz... stored only on registry C
    1. Client requests GET /v2/org/v1/blobs/sha256:xyz... from A
    2. A checks local store → absent
    3. A queries peer B: GET /v2/org/v1/blobs/sha256:xyz...
    4. B checks local store → absent
    5. B queries peer C: GET /v2/org/v1/blobs/sha256:xyz...
    6. C checks local store → found
    7. C returns content to B (200 OK)
    8. B re-hashes content → sha256 matches xyz... → verified
    9. B stores content locally (put, idempotent)
    10. B returns content to A (200 OK)
    11. A re-hashes content → sha256 matches xyz... → verified
    12. A stores content locally (put, idempotent)
    13. A returns content to client (200 OK)
    14. Client re-hashes content → sha256 matches xyz... → verified
Future requests to A for sha256:xyz... are served locally.
Future requests to B for sha256:xyz... are served locally.
The content propagated from C through B to A via a single
client request, with verification at every hop.
---
## Appendix I: Long-Duration Disconnection Worked Example
A submarine research vessel operates a κ-Distribution registry
locally. It disconnects from shore networks for 90 days.
**During disconnection:**
    Day 1:  Scientist pushes dataset κ₁, tags it experiment/run1@latest
    Day 30: Scientist pushes dataset κ₂, tags it experiment/run1@latest
            (κ₁ is now the previous value)
    Day 60: Scientist pushes dataset κ₃, tags it experiment/run2@latest
Meanwhile, on shore:
    Day 15: Shore team updates experiment/run1@latest → κ₄
    Day 45: Shore team updates experiment/run1@latest → κ₅
**On reconnection (Day 91):**
    1. Replication propagates shore tags to submarine:
       experiment/run1@latest: shore says κ₅, submarine says κ₂
       Conflict detected (both claim different previous values)
    2. Conflict resolution: compare HLC timestamps
       κ₂ was tagged at Day 30 submarine time
       κ₅ was tagged at Day 45 shore time
       κ₅ wins (higher timestamp)
       experiment/run1@latest → κ₅ on both registries
    3. Content backfill: submarine has κ₁, κ₂, κ₃ but not κ₄, κ₅
       Federation fetch: submarine requests κ₄ and κ₅ from shore
       Shore returns content, submarine verifies by re-hash, stores
    4. Replication propagates submarine blobs to shore:
       Shore receives κ₁, κ₂, κ₃ — verifies, stores
    5. Result: both registries have κ₁ through κ₅
       experiment/run1@latest → κ₅ (shore's latest, by timestamp)
       experiment/run2@latest → κ₃ (no conflict, submarine only)
       All content accessible on both registries
       No data loss. No manual intervention.

---

## Appendix J: Error Semantics

Errors are defined by their semantics, not by protocol-binding-
specific status codes. HTTP status code mappings are provided in
the HTTP binding (§6) and are specific to that binding. Other
bindings use their own error representation.

**Content errors:**

    ContentNotFound
        The requested κ-label does not exist in the store.
        The client SHOULD retry with federation fetch or wait for
        replication.

    ContentMismatch
        The provided κ-label does not match the hash of the
        provided content. The store rejected the write. The client
        has a bug in its κ-label computation.

    ContentExists
        A put with a κ-label that already exists but with different
        content. This implies a hash collision (probability 2^-128
        for SHA-256). The store rejects the write. No recovery.

    SchemaValidationFailed
        Content does not pass the schema registered for the write
        path. The write is rejected. The content does not enter
        the store.

    FilterRejected
        An admission filter rejected the content. The write is
        rejected. The rejection reason is returned to the client.

    FilterExecutionFailed
        An admission filter crashed or timed out. The write is
        rejected (fail closed). The failure is logged.

**Tag errors:**

    TagNotFound
        The requested tag does not exist at the path.

    TagContentAbsent
        tag_set was called with a κ-label that does not exist in
        the store. Tags MUST NOT point to absent content.

    TagConflict
        tag_set_if was called with an expected-previous value that
        does not match the current tag value. Optimistic concurrency
        failure. The client SHOULD re-read and retry.

**Edge errors:**

    EdgeNotFound
        The requested edge κ-label does not exist.

    EdgeSourceAbsent
        edge_put was called with a source κ-label that does not
        exist. Source MUST exist.

**Operational errors:**

    ConsensusUnavailable
        The consensus domain for the requested tag operation does
        not have quorum. Tag updates stall. Content operations
        are unaffected.

    FinalizerOutstanding
        An unpin or remove was requested for a κ-label that has
        outstanding finalizer pins. The operation is rejected until
        the controller releases the finalizer.

    UploadSessionExpired
        A chunked upload session timed out. Partial content is
        discarded. The temporary upload pin is released.

    RateLimited
        The client has exceeded the operation rate limit for the
        authenticated identity. Retry after the indicated backoff.

---

## Appendix K: Manifest Patterns

This appendix is informative. It illustrates common manifest
patterns that applications build on top of κ-Distribution. The
protocol does not prescribe manifest formats — these are
application-level conventions.

**Container image manifest:**
A blob whose content is JSON describing a set of layer blobs
(filesystem diffs) and a configuration blob. Each layer is
referenced by its κ-label. The manifest is tagged at a path
corresponding to the image repository and version. Pulling an
image resolves the tag, retrieves the manifest blob, parses it
to discover layer κ-labels, and retrieves each layer.

**LLM model weight manifest:**
A blob whose content lists weight tensor κ-labels, tokenizer
κ-label, configuration κ-label, and quantization metadata. A
model provider pushes the manifest and its referenced blobs. A
consumer pulls by tag, retrieves the manifest, discovers the
weight κ-labels, and retrieves them — potentially from different
registries if federation fetch is configured.

**Helm chart manifest:**
A blob whose content is a tar archive containing templates,
values, and Chart.yaml. The entire archive is a single blob with
a κ-label. Alternatively, each template file is a separate blob
and the chart manifest references them by κ-label — enabling
deduplication across chart versions that share templates.

**etcd state snapshot manifest:**
A blob whose content lists key→κ-label pairs representing the
state of a key-value namespace at a point in time. Each value is
a blob in the store. The snapshot is tagged with a timestamp tag.
Restoring state resolves the snapshot tag, retrieves the manifest,
and rebuilds the key-value namespace from the listed blobs. This
is the D2 state snapshot model.

**Scientific dataset manifest:**
A blob listing data file κ-labels, metadata κ-label (JSON with
run number, detector configuration, calibration version),
provenance κ-label (who produced it, when, with what software).
The manifest is tagged at the experiment's path with both a
version tag and a timestamp tag.

---

## Appendix L: Deployment Topology Patterns

This appendix is informative. It illustrates common deployment
topologies. The protocol is topology-agnostic — these patterns
are deployment choices.

**Single node, filesystem backend:**
One registry, one filesystem, one process. Content stored as
files. Tags stored in a JSON index file. No consensus needed —
a single-node mutex suffices. Suitable for development, testing,
embedded devices, and single-user workstations.

    client ←→ registry ←→ filesystem

**Single cluster, distributed backend:**
Multiple registry instances behind a load balancer, backed by a
distributed storage system. Consensus for tag updates. Content
reads served by any instance. Suitable for organizational
deployments.

    clients ←→ load balancer ←→ registry instances ←→ distributed store
                                       ↕
                                  consensus group

**Multi-site, async replication:**
Multiple independent sites, each running a full stack. Content
replicates asynchronously between sites. Conflict resolution is
deterministic. Federation fetch resolves cross-site content on
demand. Suitable for geographically distributed organizations.

    site A ←→ site B ←→ site C
      ↕           ↕           ↕
    local       local       local
    store       store       store

**Hub and spoke:**
A central hub registry and multiple edge registries. Edges
replicate from the hub. Writes happen at the hub. Reads are
served by the nearest edge. Suitable for CDN-like deployments
where content flows outward from a source of truth.

    edge₁ ←→ hub ←→ edge₂
    edge₃ ←→ hub ←→ edge₄

**Mesh with partial connectivity:**
Every node connects to some but not all peers. Multi-hop
federation fetch resolves content across the mesh. Suitable for
IoT, field networks, military/research deployments where full
mesh is impractical.

    A ←→ B ←→ C
         ↕
         D ←→ E
    A reaches C only through B. D reaches A only through B.

**Hybrid cloud-edge:**
A cloud registry as the durable source of truth. Edge registries
on premises or on devices with intermittent connectivity. Content
syncs when connected. Operates independently when disconnected.
Reconciles on reconnection.

    cloud registry (durable, always-on)
         ↕
    edge registry₁ (intermittent, local storage)
    edge registry₂ (offline for months, local storage)
    edge registry₃ (mobile, cellular, local storage)

---

## Appendix M: Relationship to Prior Art

This appendix is informative. It positions κ-Distribution relative
to existing content-addressed and distribution systems.

**OCI Distribution:**
OCI Distribution is a content-addressed container image registry
protocol. κ-Distribution generalizes OCI's blob+manifest+tag model
to arbitrary content, adds edges as first-class objects, adds
composition operations, adds schema validation, and adds admission
filters. OCI clients can interact with a κ-Distribution store
through the compatibility mapping in Appendix F.

**IPFS / IPLD:**
IPFS is a peer-to-peer content-addressed filesystem. IPLD is its
data model. κ-Distribution shares the content-addressing property
(same content → same address) but differs in its tag model
(mutable pointers with consensus), its edge model (typed
relationships stored as blobs), and its garbage collection model
(pin-based reachability). IPFS uses CIDs (Content Identifiers)
which encode hash algorithm and codec in a binary multiformat.
κ-labels encode the hash algorithm as an ASCII prefix. The two
formats are interconvertible for shared hash algorithms.

**Git:**
Git is a content-addressed object store with four object types
(blob, tree, commit, tag). κ-Distribution has one object type
(blob) and expresses tree/commit/tag semantics through edge
relationships between blobs. Git's packfiles (delta-compressed
object storage) have no equivalent in κ-Distribution — each blob
is stored independently.

**Nix store:**
The Nix store addresses build outputs by the hash of their
inputs (derivation hash). κ-Distribution addresses content by the
hash of the content itself (content hash). The Nix store's
garbage collection (GC roots are store paths referenced by
profiles) parallels κ-Distribution's pin-based reachability
model.

**etcd:**
etcd is a distributed key-value store with linearizable writes
and a watch API. κ-Distribution's tag operations provide the
equivalent of etcd's key-value operations. κ-Distribution's
event model (§7, §9) provides watch-equivalent functionality.
κ-Distribution separates content storage from consensus, enabling
content to scale independently of the consensus throughput limit.

**Package URLs (PURL):**
PURL is a scheme for identifying software packages. The κ-address
convention (§4.1) is informed by PURL's structure
(`scheme:type/namespace/name@version`) but extends it with
CRD-style organizational depth on the left side of the colon
separator, enabling arbitrary subdomain hierarchy for ownership
delegation.

---

## Appendix N: Versioning and Evolution

This specification is versioned. The version is returned by the
version check endpoint (§6.1). Clients and registries negotiate
compatibility via the version string.

**Version format:** `<major>.<minor>.<patch>`

**Compatibility rules:**
- Patch version changes: bug fixes, clarifications. Fully
  compatible. No behavioral changes.
- Minor version changes: additive features. New operations,
  new edge relation types, new conformance levels. Existing
  operations behave identically. Registries at version 1.1 serve
  clients at version 1.0 for all 1.0 operations.
- Major version changes: breaking changes. Operations may change
  semantics. Clients MUST check the major version before
  proceeding.

**Extension mechanism:**
Registries MAY implement operations beyond those defined in this
specification. Extended operations MUST use endpoint paths that
do not conflict with the paths defined in Appendix A. Clients
MUST NOT assume extended operations are available — they are
registry-specific features.

**κ-label stability:**
The κ-label format is fixed across all versions. A κ-label
computed by a version 1.0 implementation is identical to the same
κ-label computed by a version 2.0 implementation. Content
addressing is the invariant that never changes.
