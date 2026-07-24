# κ-Distribution Protocol Specification
## Content-Addressed Distributed Storage with CRD-Typed Namespaces
Status: Draft Specification
Version: 0.1.0
Date: 2026-07-19
Companion to: CROSSPOLLINATION.md (transport), CROSSPOLLINATION-PRIMER.md (primer)
---
## 1. Abstract
This document specifies the κ-Distribution protocol: a
content-addressed distributed storage protocol built on the
κ-label primitive defined in the κ-Label Unified Content Identity
Protocol (CROSSPOLLINATION.md). The protocol defines how κ-labeled
content is stored, retrieved, addressed, composed, replicated,
garbage collected, and schema-validated across single-node and
multi-node deployments.
The protocol defines:
- An object model where content is immutable, tags are mutable
  pointers to immutable content, and graph edges encode typed
  relationships between content
- A three-layer storage interface (KappaStore, KappaIndex,
  KappaGraph) totaling fourteen methods, implementable against
  any storage backend
- A CRD-typed namespace hierarchy that inherits Kubernetes Custom
  Resource Definition semantics for schema validation, spec/status
  separation, owner references, and finalizer-based lifecycle
  management
- A PURL-based external addressing convention (`pkg:kappa`) that
  serializes the CRD namespace hierarchy into a portable,
  machine-parseable identifier compatible with SBOM, attestation,
  and supply chain security tooling
- An HTTP protocol surface registered as a native protocol
  alongside S3 and Swift in RADOS Gateway (RGW), with OCI
  Distribution v1.1.1 and S3 as compatibility projections onto
  the same underlying storage
- Consistency, replication, garbage collection, and burst ingestion
  behavior at multi-hundred-petabyte scale
The protocol is backend-agnostic. Reference backend layouts are
provided for RADOS/BlueStore, filesystem, S3-compatible object
stores, and SQLite/SQLCipher. Any backend that implements the
fourteen-method interface is compliant.
---
## 2. Terminology and Conventions
### 2.1 Key Words
The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT",
"SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in
this document are to be interpreted as described in RFC 2119.
### 2.2 Glossary
    κ-label        71-byte ASCII content address: <algorithm>:<64-hex>
    tag            mutable pointer: a name that resolves to a κ-label
    witness        AddressWitness that replays κ-label derivation
                   without re-hashing the original content
    edge           typed relationship between two κ-labels
    pin            GC protection marker on a κ-label
    sweep          reachability walk from pinned κ-labels to
                   determine GC-eligible content
    GVR            Group/Version/Resource — the CRD type coordinate
    CRD            Custom Resource Definition — a schema that
                   validates content written to a typed path
    PURL           Package URL (pkg:type/namespace/name@version)
    consensus      ordering authority for tag updates (Raft,
                   CASPaxos, DARE, or equivalent)
    backend        storage implementation behind the fourteen-method
                   interface (RADOS, filesystem, S3, SQLite, etc.)
    projection     protocol compatibility layer that presents the
                   same underlying storage through a different API
                   (OCI Distribution, S3)
    federation     cross-site content resolution via κ-label query
    verify-on-     client-side re-hash of received content to confirm
    receipt        κ-label integrity, independent of server trust
### 2.3 Notation Conventions
Path segments in angle brackets are variable:
    /v2/{group}/{version}/{resources}/{namespace}/{name}
Wire format fields use the same notation as CROSSPOLLINATION.md.
PURL examples use the `pkg:kappa` type prefix registered by this
specification.
---
## 3. Object Model
### 3.1 The κ-Labeled Object
A κ-labeled object consists of four components:
    content     arbitrary bytes in a known format
    κ-label     deterministic content address (KappaLabel<71>)
    witness     AddressWitness for the κ-label derivation
    metadata    key-value attributes stored alongside the object
The κ-label is computed from the canonical form of the content
bytes using the ψ-pipeline defined in CROSSPOLLINATION.md §3.1.
The witness is produced as a side effect of κ-label computation.
The metadata is not covered by the κ-label — it is mutable,
advisory, and backend-specific.
A κ-labeled object is immutable by definition. Changing the content
changes the κ-label, which produces a different object. There is no
"update in place." Mutation is expressed as creating a new object
and updating a tag to point to the new κ-label.
### 3.2 Tags
A tag is a mutable binding from a human-readable name to an
immutable κ-label:
    tag: "latest" → sha256:e3b0c44298fc1c14...
    tag: "v2.1.0" → sha256:7f83b1657ff1fc53...
    tag: "2026-07-19T14:30:00.123456789" → blake3:af1349b9f5f9a1a6...
Tags are scoped to a resource path (§4). A tag name is a UTF-8
string of 1 to 256 bytes. Tag names MUST NOT contain the `/`
character (path separator) or the `@` character (version delimiter
in PURL).
Tags are the only mutable state in the protocol. All tag operations
(create, update, delete) are linearizable within a consensus domain
(§10.1). Content operations are idempotent and eventually consistent.
Tag names MAY be ISO 8601 timestamps. When used as timestamps, tags
encode D2's timestamp-as-tag model: time IS the version number.
Temporal queries are tag list operations with range filters.
### 3.3 Graph Edges
A graph edge is a typed, directed relationship between two κ-labels:
    source_κ  ——[relation]——→  target_κ
Edges carry:
    source      KappaLabel<71> — the origin
    target      KappaLabel<71> — the destination
    relation    UTF-8 string — the edge type
    metadata    CBOR-encoded bytes — optional attributes
Edges are append-only. An edge is created or deleted; it is never
updated. Edges are not covered by a κ-label (they are structural,
not content).
The following relation types are defined by this specification:
    owns            owner → owned (cascade delete semantics)
    derives-from    child → parent (content lineage)
    composed-of     composed → operand (composition provenance)
    schema          content → schema (CRD validation binding)
    spec-of         spec → status (spec/status linkage)
    attests         attestation → subject (signed claim)
    pins            pin-holder → pinned (GC protection)
    refers-to       referrer → subject (OCI referrers equivalent)
Implementations MAY define additional relation types. Relation
types are not validated against a registry — any UTF-8 string is
accepted. The defined types are conventions, not constraints.
### 3.4 CRD Schemas
A CRD schema is a κ-labeled object whose content is an OpenAPI v3
structural schema definition. The schema defines validation rules
for content written to a specific GVR path.
A CRD schema contains:
    group       DNS-style group name (e.g., "atlas.cern.ch")
    version     API version (e.g., "v1")
    resource    plural resource name (e.g., "calibrations")
    kind        singular kind name (e.g., "Calibration")
    schema      OpenAPI v3 structural schema (JSON)
CRD schemas are stored in the system namespace `_crds`:
    κ-label of schema = uor_addr::json::address(schema_bytes)
    tag path = _crds/{group}/{version}/{resource}
When a CRD schema is registered, all subsequent content written to
paths matching `/{group}/{version}/{resources}/...` is validated
against the schema before κ-label computation. Content that fails
validation is rejected with `SchemaValidationFailed`. Content that
predates the schema registration is not retroactively validated.
CRD schema evolution follows the content-addressed mutation model:
changing the schema produces a new κ-label. The old schema and the
new schema coexist. A graph edge (`derives-from`) links the new
schema to the old one. Content validated against the old schema
retains its original κ-label.
### 3.5 Spec/Status Separation
For resources with controller-managed status, the spec (desired
state) and status (observed state) are separate κ-labeled objects.
    spec:   content bytes validated against the CRD schema
            κ-label computed from spec content
            tagged at /{gvr}/{namespace}/{name}@{spec-tag}
    status: content bytes representing observed state
            κ-label computed from status content
            tagged at /{gvr}/{namespace}/{name}/status@{status-tag}
The spec and status are linked by a graph edge:
    edge_put(spec_κ, "spec-of", status_κ)
The spec κ-label is immutable — updating the desired state creates
a new spec with a new κ-label and updates the spec tag. The status
κ-label is updated by the controller as observed state changes.
Both are independently addressable, independently verifiable, and
independently replicable.
### 3.6 Pins and Finalizers
A pin is a GC protection marker on a κ-label:
    pin:{κ-label} → TTL (seconds, 0 = permanent)
Pinned κ-labels and their reachability closure are protected from
garbage collection. Unpinned κ-labels with no inbound edges from
pinned objects are GC-eligible.
A finalizer is a controller-owned pin:
    pin:{κ-label}:finalizer/{controller-name} → 0
Finalizer pins are permanent (TTL = 0) until the controller
explicitly removes them. An object with outstanding finalizer pins
cannot be garbage collected, even if its tag is deleted. The
controller removes the finalizer after cleanup is complete.
Upload sessions register temporary pins before writing blobs:
    pin:{κ-label}:upload/{session-uuid} → session TTL
The temporary pin protects partially-uploaded content from GC.
When the upload completes, the temporary pin is replaced by the
tag binding (which implies a pin via reachability). When the upload
is abandoned, the temporary pin expires and the partial content
becomes GC-eligible.
---
## 4. Addressing
### 4.1 The CRD Namespace Hierarchy
Every resource in κ-Distribution is addressed by a path that
encodes its type and location:
    /{group}/{version}/{resources}/{namespace}/{name}
This path directly inherits Kubernetes CRD addressing semantics:
    group       DNS-style API group (e.g., "atlas.cern.ch")
    version     API version (e.g., "v1", "v1beta1")
    resources   plural resource name (e.g., "calibrations")
    namespace   organizational scope (e.g., "run3")
    name        resource name within the namespace (e.g., "alignment")
Cluster-scoped resources (no namespace) use the path:
    /{group}/{version}/{resources}/{name}
The path hierarchy maps to Kubernetes API paths:
    Kubernetes:     /apis/atlas.cern.ch/v1/namespaces/run3/calibrations/alignment
    κ-Distribution: /v2/atlas.cern.ch/v1/calibrations/run3/alignment
The `/v2/` prefix is the protocol version marker, following OCI
Distribution convention.
### 4.2 The Three Address Forms
The same object is addressable through three representations:
**κ-label (machine address):**
    sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
Sufficient to retrieve and verify the object from any store.
No resolution needed. Used in wire frames, dedup caches, audit
chains, RADOS object names, and RocksDB keys.
**Tag path (API address):**
    /v2/atlas.cern.ch/v1/calibrations/run3/alignment@latest
Resolves through KappaIndex to a κ-label. Used in HTTP requests,
controller watches, and operational tooling.
**PURL (portable address):**
    pkg:kappa/atlas.cern.ch/v1/calibrations/run3/alignment@latest
Serializes the tag path into a URI-safe string. Used in SBOMs,
attestations, SCITT receipts, paper citations, and cross-system
references.
### 4.3 PURL Registration: pkg:kappa
This specification registers the `kappa` type in the Package URL
scheme (https://github.com/package-url/purl-spec).
The canonical PURL form is:
    pkg:kappa/{group}/{version}/{resources}/{namespace}/{name}@{tag}
    ?{qualifiers}#{subpath}
Components:
    type        "kappa" (fixed)
    namespace   "{group}/{version}/{resources}/{namespace}"
                (the GVR + namespace as a hierarchical namespace)
    name        resource name
    version     tag name or κ-label (the @-delimited version field)
    qualifiers  key=value pairs acting as label selectors
    subpath     path within composite artifacts
Examples:
    pkg:kappa/atlas.cern.ch/v1/calibrations/run3/alignment@latest
    pkg:kappa/atlas.cern.ch/v1/calibrations/run3/alignment@sha256:abcdef...
    pkg:kappa/atlas.cern.ch/v1/datasets/run3/muon-pairs@2026-07-19T14:30:00
    pkg:kappa/braincraft.io/v1/images/production/rekindle-node@v2.1.0
    pkg:kappa/cms.cern.ch/v1/simulations/pythia8/default-tune@latest?format=root&tier=raw
When the version field is a κ-label (contains `:`), the PURL
resolves directly via KappaStore without tag lookup. When the
version field is a tag name (no `:`), the PURL resolves via
KappaIndex tag lookup first.
Qualifiers are label selectors. The protocol handler evaluates
qualifiers against the object's metadata labels after tag
resolution and before returning content. If the resolved object's
labels do not match all qualifiers, the response is 404.
### 4.4 Resolution Path
    PURL
        |
        v
    parse: extract group, version, resource, namespace, name, tag
        |
        v
    if tag contains ':' (κ-label):
        → KappaStore::get(κ-label) → content bytes
    else (tag name):
        → KappaIndex::tag_get(path, tag) → κ-label
        → KappaStore::get(κ-label) → content bytes
        |
        v
    if qualifiers present:
        → KappaStore::meta_get(κ, "labels") → label map
        → evaluate qualifiers against labels
        → reject if mismatch
        |
        v
    verify-on-receipt (client-side):
        → compute κ-label from received content
        → compare against requested/returned κ-label
        → reject if mismatch
### 4.5 Timestamp-as-Tag
Tag names MAY be ISO 8601 timestamps with nanosecond precision:
    2026-07-19T14:30:00.123456789
When used as timestamps, the tag list for a resource path is a
timeline. Temporal queries are tag list operations:
    "What was the value at time T?"
        → tag_get(path, "2026-07-19T14:30:00.123456789")
    "What values existed between T1 and T2?"
        → tag_list(path, after=T1, before=T2)
    "What is the most recent value?"
        → tag_list(path, limit=1, order=desc)
Timestamp tags and named tags coexist. A resource can have both
`latest` and `2026-07-19T14:30:00.123456789` pointing to the same
κ-label. The named tag is a human convenience; the timestamp tag
is an audit trail entry.
Compaction is the deletion of old timestamp tags:
    tag_delete(path, tag) for all tags where tag < threshold
The content the tags pointed to becomes GC-eligible if no remaining
tags or pins reference it.
---
## 5. Storage Interface
### 5.1 KappaStore (Content)
The content storage interface. Seven methods. All content is
immutable once stored — `put` fails if the κ-label already exists.
    put(κ: &KappaLabel, content: &[u8]) → Result<()>
        Store content by κ-label.
        MUST fail with ContentExists if κ-label already exists.
        MUST verify that the provided κ-label matches the hash
        of the provided content (defense-in-depth; the caller
        is expected to have computed the κ-label correctly).
        MUST store the content bytes and make them retrievable
        by the provided κ-label.
    get(κ: &KappaLabel) → Result<Option<Vec<u8>>>
        Retrieve content by κ-label.
        MUST return Some(content) if the κ-label exists.
        MUST return None if the κ-label does not exist.
        MUST NOT return content that does not match the κ-label.
    exists(κ: &KappaLabel) → Result<bool>
        Check existence without retrieving content.
        MUST return true if the κ-label exists.
        MUST return false otherwise.
    remove(κ: &KappaLabel) → Result<()>
        Mark content as GC-eligible.
        MUST NOT immediately delete content if pins exist.
        MUST remove the content from the reachability set
        so that the next GC sweep can evaluate eligibility.
        Implementations MAY defer actual deletion to the GC sweep.
    list(prefix: &str) → Result<Vec<KappaLabel>>
        Enumerate κ-labels matching a prefix.
        The prefix MUST be matched against the full κ-label string
        (including the σ-axis prefix).
        Implementations SHOULD support pagination via a cursor
        parameter (not shown in this interface for clarity).
    meta_get(κ: &KappaLabel, key: &str) → Result<Option<Vec<u8>>>
        Read a metadata attribute on a κ-labeled object.
        Metadata is not covered by the κ-label. It is mutable
        and advisory.
        MUST return None if the κ-label does not exist or if the
        key does not exist on the object.
    meta_set(κ: &KappaLabel, key: &str, value: &[u8]) → Result<()>
        Write a metadata attribute on a κ-labeled object.
        MUST fail if the κ-label does not exist.
        MUST overwrite the previous value if the key exists.
### 5.2 KappaIndex (Tags)
The mutable pointer interface. Four methods. Tag operations are
the only operations that require consensus in a multi-node
deployment.
    tag_set(path: &str, tag: &str, κ: &KappaLabel) → Result<()>
        Bind a tag name to a κ-label at the given resource path.
        MUST verify that the κ-label exists in KappaStore before
        binding (content must exist before tags can reference it).
        MUST be linearizable within the consensus domain for the
        path's CRD group (§10.1).
        If the tag already exists, MUST atomically replace the
        old binding with the new one.
    tag_get(path: &str, tag: &str) → Result<Option<KappaLabel>>
        Resolve a tag name to a κ-label.
        MUST return Some(κ) if the tag exists at the path.
        MUST return None if the tag does not exist.
    tag_list(path: &str) → Result<Vec<(String, KappaLabel)>>
        List all tags at a resource path.
        MUST return tag name and κ-label pairs.
        Implementations SHOULD support pagination, ordering
        (ascending/descending by tag name), and range filtering
        (after/before for timestamp tags).
    tag_delete(path: &str, tag: &str) → Result<()>
        Remove a tag binding.
        MUST NOT delete the content the tag pointed to. Content
        deletion is handled by GC based on reachability from
        remaining pins and tags.
### 5.3 KappaGraph (Edges)
The relationship interface. Three methods. Edges are append-only
(created or deleted, never updated).
    edge_put(source: &KappaLabel, relation: &str,
             target: &KappaLabel, meta: &[u8]) → Result<()>
        Create a typed edge from source to target.
        MUST verify that source exists in KappaStore.
        Target MAY be absent (lazy population for cross-site
        replication — the edge is structurally correct even if
        the target blob has not yet replicated).
        If the exact edge (source, relation, target) already
        exists, MUST update the metadata without creating a
        duplicate.
    edge_get(κ: &KappaLabel, relation: Option<&str>,
             direction: Direction) → Result<Vec<Edge>>
        Query edges involving a κ-label.
        direction = Outbound: return edges where κ is the source
        direction = Inbound:  return edges where κ is the target
        direction = Both:     return edges where κ is either
        If relation is Some, filter by relation type.
        If relation is None, return all edges.
    edge_remove(source: &KappaLabel, relation: &str,
                target: &KappaLabel) → Result<()>
        Delete a specific edge.
        MUST NOT delete the source or target objects.
### 5.4 Interface Invariants
The following invariants MUST hold across all backend
implementations:
1. **Content immutability.** `put(κ, bytes_A)` followed by
   `put(κ, bytes_B)` where `bytes_A != bytes_B` MUST fail on the
   second call. κ-labels are deterministic — different content
   produces different κ-labels. A collision (different content,
   same κ-label) has probability 2^-128 for SHA-256.
2. **Tag-before-content prohibition.** `tag_set(path, tag, κ)` MUST
   fail if `exists(κ)` returns false. Tags MUST NOT point to
   absent content.
3. **Verify-on-put.** `put(κ, bytes)` MUST verify that `κ` is the
   correct κ-label for `bytes` by re-computing the hash. This is
   defense-in-depth — the caller is expected to have computed the
   κ-label correctly, but the store verifies independently.
4. **Edge source existence.** `edge_put(source, relation, target)`
   MUST verify that `source` exists. Target MAY be absent (§3.3).
5. **Idempotent put.** `put(κ, bytes)` where `κ` already exists
   and the stored content matches `bytes` MUST succeed (no-op).
   This enables safe retries and concurrent puts of the same
   content from multiple writers.
---
## 6. HTTP Protocol Surface
### 6.1 Protocol Registration
κ-Distribution is registered as a native RGW protocol alongside
S3 and Swift:
    rgw_enable_apis = s3, kappa
The protocol handler dispatches all requests matching `/v2/*` to
the κ-Distribution handler. The `/v2/` prefix follows OCI
Distribution convention for version negotiation.
### 6.2 Version Check
    GET /v2/
    Response: 200 OK
    Body: {"kappa-distribution": "1.0.0"}
Clients MUST check this endpoint before issuing other requests.
A 404 response indicates the server does not support
κ-Distribution.
### 6.3 Content Endpoints
**Retrieve content by κ-label:**
    HEAD /v2/{path}/blobs/{κ-label}
    Response: 200 OK
    Headers:
        Content-Length: <size>
        X-Kappa-Axis: <σ-axis>
        X-Kappa-Window: <window-index>
    (no body)
    GET /v2/{path}/blobs/{κ-label}
    Response: 200 OK
    Headers:
        Content-Length: <size>
        X-Kappa-Axis: <σ-axis>
        X-Kappa-Window: <window-index>
    Body: content bytes
    Response: 404 Not Found (κ-label does not exist)
**Push content (monolithic):**
    PUT /v2/{path}/blobs/{κ-label}
    Body: content bytes
    Response: 201 Created (new content stored)
    Response: 200 OK (content already exists, idempotent)
    Response: 409 Conflict (κ-label does not match content hash)
The server MUST verify the κ-label against the received content
before storing. If the κ-label does not match, the server MUST
return 409 and discard the content.
**Push content (chunked upload):**
    POST /v2/{path}/blobs/uploads/
    Response: 202 Accepted
    Headers:
        Location: /v2/{path}/blobs/uploads/{session-uuid}
        X-Kappa-Upload-Session: {session-uuid}
    PATCH /v2/{path}/blobs/uploads/{session-uuid}
    Headers:
        Content-Range: {start}-{end}
    Body: chunk bytes
    Response: 202 Accepted
    Headers:
        X-Kappa-Upload-Offset: {next-expected-offset}
    PUT /v2/{path}/blobs/uploads/{session-uuid}?kappa={κ-label}
    Body: final chunk (may be empty)
    Response: 201 Created
    Response: 409 Conflict (κ-label does not match assembled content)
The server MUST register a temporary pin for the upload session
on POST (§3.6). The server MUST remove the temporary pin on PUT
(success) or on session timeout (abandonment).
**Remove content:**
    DELETE /v2/{path}/blobs/{κ-label}
    Response: 202 Accepted (unpinned, GC-eligible)
    Response: 409 Conflict (outstanding pins or finalizers)
DELETE does not immediately delete content. It removes the
caller's pin. Content with no remaining pins becomes eligible
for the next GC sweep.
### 6.4 Tag Endpoints
**Resolve tag:**
    GET /v2/{path}/manifests/{tag}
    Response: 200 OK
    Headers:
        X-Kappa-Label: {κ-label}
        X-Kappa-Axis: {σ-axis}
    Body: content bytes
    Response: 404 Not Found (tag does not exist)
When `{tag}` contains `:` (indicating a κ-label rather than a tag
name), the server MUST resolve directly via KappaStore without tag
lookup.
**Bind tag:**
    PUT /v2/{path}/manifests/{tag}
    Body: content bytes
    Response: 201 Created
    Headers:
        X-Kappa-Label: {κ-label}
The server MUST:
1. If a CRD schema is registered for the path's GVR, validate
   the content against the schema. Return 422 if validation fails.
2. Compute the κ-label from the content.
3. Store the content via KappaStore::put (idempotent if exists).
4. Bind the tag via KappaIndex::tag_set (consensus-protected).
5. Create a `schema` edge from the content κ-label to the CRD
   schema κ-label (if a schema is registered).
6. Return the κ-label in the X-Kappa-Label response header.
**List tags:**
    GET /v2/{path}/tags/list
    Query parameters:
        n:      max results (default 100)
        last:   pagination cursor (last tag name from prior page)
        order:  "asc" or "desc" (default "asc")
        after:  ISO 8601 timestamp (range filter for timestamp tags)
        before: ISO 8601 timestamp (range filter for timestamp tags)
    Response: 200 OK
    Body: {"tags": ["latest", "v2.1.0", "2026-07-19T14:30:00"]}
**Delete tag:**
    DELETE /v2/{path}/manifests/{tag}
    Response: 202 Accepted
    Response: 404 Not Found
### 6.5 Graph Endpoints
**Query edges:**
    GET /v2/{path}/graph/{κ-label}
    Query parameters:
        relation:  filter by relation type (optional)
        direction: "inbound", "outbound", or "both" (default "outbound")
        n:         max results (default 100)
        last:      pagination cursor
    Response: 200 OK
    Body:
    {
        "edges": [
            {
                "source": "sha256:aaa...",
                "relation": "derives-from",
                "target": "sha256:bbb...",
                "metadata": { ... }
            }
        ]
    }
**Create edge:**
    PUT /v2/{path}/graph/{source-κ}/{relation}/{target-κ}
    Body: CBOR-encoded metadata (optional, may be empty)
    Response: 201 Created
    Response: 200 OK (edge already exists, metadata updated)
**Delete edge:**
    DELETE /v2/{path}/graph/{source-κ}/{relation}/{target-κ}
    Response: 202 Accepted
    Response: 404 Not Found
### 6.6 Witness Endpoint
    GET /v2/{path}/witnesses/{κ-label}
    Response: 200 OK
    Headers:
        Content-Type: application/octet-stream
    Body: postcard-serialized AddressWitness
    Response: 404 Not Found (κ-label or witness does not exist)
Witnesses are stored as metadata on the κ-labeled object
(`meta_get(κ, "witness")`). The witness endpoint is a convenience
accessor for the metadata value.
### 6.7 Composition Endpoint
    POST /v2/{path}/compose/{operation}
    Body: JSON array of operand κ-labels
    Response: 200 OK
    Body:
    {
        "composed": "sha256:ccc...",
        "witness": "<base64-encoded postcard AddressWitness>",
        "operands": ["sha256:aaa...", "sha256:bbb..."],
        "operation": "g2"
    }
The server MUST compute the composition using the algorithms
defined in CROSSPOLLINATION-ADDENDUM.md §4. The composed κ-label
MUST be stored in KappaStore. A `composed-of` edge MUST be created
from the composed κ-label to each operand.
Valid operation values: `g2`, `f4`, `e6`, `e7`, `e8`.
### 6.8 Placement Endpoint
    GET /v2/{path}/placement/{κ-label}
    Response: 200 OK
    Body:
    {
        "window_index": 7,
        "node": "sha256:nodeidentity...",
        "shard": "kappa-blobs-07"
    }
The placement endpoint is informational. Clients MAY use it for
locality optimization. The server MUST compute placement using
`uor_placement::window_index()` (CROSSPOLLINATION-ADDENDUM.md §7)
and the consistent hash ring (if deployed in multi-node mode).
### 6.9 GC Endpoints
**Pin content:**
    POST /v2/{path}/gc/pin
    Body: {"kappa": "sha256:aaa...", "ttl": 3600}
    Response: 201 Created
    Response: 200 OK (pin already exists, TTL updated)
A TTL of 0 creates a permanent pin. A positive TTL creates a
time-limited pin that expires after the specified seconds.
**Unpin content:**
    POST /v2/{path}/gc/unpin
    Body: {"kappa": "sha256:aaa..."}
    Response: 200 OK
    Response: 409 Conflict (outstanding finalizer pins)
**Trigger GC sweep:**
    POST /v2/{path}/gc/sweep
    Response: 202 Accepted
    Body: {"sweep_id": "uuid", "estimated_objects": 1000000}
The sweep runs asynchronously. The server snapshots the pin set
at sweep start time. Any pin added after the snapshot protects its
subgraph from the NEXT sweep, not the current one.
**Query GC status:**
    GET /v2/{path}/gc/status
    Response: 200 OK
    Body:
    {
        "last_sweep": "2026-07-19T14:00:00Z",
        "duration_seconds": 90,
        "objects_scanned": 1000000,
        "objects_evicted": 42000,
        "bytes_reclaimed": 1073741824,
        "active_pins": 958000,
        "pending_finalizers": [
            {"kappa": "sha256:...", "controller": "calibration-controller"}
        ]
    }
### 6.10 CRD Registration Endpoints
**Register CRD schema:**
    PUT /v2/_crds/{group}/{version}/{resource}
    Body: OpenAPI v3 structural schema (JSON)
    Response: 201 Created
    Headers:
        X-Kappa-Label: {κ-label of the schema}
    Response: 200 OK (schema already registered, κ-label matches)
    Response: 409 Conflict (schema already registered with
                            different content)
To update a CRD schema, the new schema MUST be a valid evolution
of the existing schema (additive fields only, no removed required
fields). The server MUST create a `derives-from` edge from the
new schema κ-label to the old schema κ-label.
**Retrieve CRD schema:**
    GET /v2/_crds/{group}/{version}/{resource}
    Response: 200 OK
    Body: OpenAPI v3 structural schema (JSON)
    Headers:
        X-Kappa-Label: {κ-label of the schema}
    Response: 404 Not Found
**List registered CRDs:**
    GET /v2/_crds/
    Response: 200 OK
    Body:
    {
        "crds": [
            {
                "group": "atlas.cern.ch",
                "version": "v1",
                "resource": "calibrations",
                "kind": "Calibration",
                "schema_kappa": "sha256:..."
            }
        ]
    }
### 6.11 Discovery Endpoint
    GET /v2/_groups/
    Response: 200 OK
    Body:
    {
        "groups": [
            {
                "group": "atlas.cern.ch",
                "versions": ["v1"],
                "resources": {
                    "v1": ["calibrations", "datasets", "simulations"]
                }
            }
        ]
    }
Analogous to `kubectl api-resources`. Returns the set of GVR
paths that have registered CRDs or existing content.
---
## 7. Backend Reference Layouts
### 7.1 RADOS / BlueStore Backend
The production distributed backend. Each pool is a RADOS pool with
device-class-specific CRUSH rules.
**Pool layout:**
    Pool              Redundancy   Media          Purpose
    ----              ----------   -----          -------
    kappa-blobs       EC 4+2       HDD            content-addressed values; bulk
    kappa-manifests   Replica 3    NVMe           manifest/realization objects
    kappa-index       Replica 3    NVMe           tag→κ-label mappings; omap
    kappa-graph       Replica 3    NVMe           typed edges; omap
    kappa-uploads     Replica 2    NVMe           upload sessions; TTL eviction
**Object naming:**
    Content blobs:
        kappa:blob:{axis}:{first-two-hex}:{full-κ-label}
        Example: kappa:blob:blake3:ed:blake3:eda736ef...
    Manifests:
        kappa:manifest:{axis}:{first-two-hex}:{full-κ-label}
    Namespace index:
        kappa:repo:{group}/{version}/{resource}/{namespace}:index
        omap keys:
            tag:{name}                     → κ-label (71 bytes ASCII)
            pin:{κ-label}                  → TTL (u64 LE, 0 = permanent)
            pin:{κ-label}:finalizer/{name} → 0 (permanent until removed)
            pin:{κ-label}:upload/{uuid}    → TTL (u64 LE)
            window:{κ-label}               → window_index (u8)
            labels:{κ-label}               → CBOR label map
    Graph edges:
        kappa:edge:{source-κ}:{relation}:{target-κ}
        omap keys:
            type     → relation type string
            created  → timestamp (u64 LE, seconds since epoch)
            meta     → CBOR-encoded metadata
            witness  → postcard-serialized composition witness (if composed)
    Upload sessions:
        kappa:upload:{uuid}:data           → partial blob data
        kappa:upload:{uuid}:state          → streaming hash checkpoint
**Two-hex prefix sharding:** The `{first-two-hex}` segment of blob
and manifest object names distributes objects across 256 RADOS
placement groups evenly. This prevents hot spots from sequential
digest generation.
**Multi-algorithm coexistence:** The `{axis}` segment in the object
name ensures that `sha256:...` and `blake3:...` κ-labels for the
same content are different RADOS objects in different sharding
namespaces. No cross-axis equivalence index is maintained (§5.1
invariant 1).
**Metadata storage:** Metadata attributes (witness, labels,
content-type) are stored as RADOS extended attributes (xattrs) on
the blob object:
    rados_setxattr(object, "witness", witness_bytes)
    rados_setxattr(object, "labels", cbor_labels)
    rados_setxattr(object, "content-type", mime_bytes)
For attributes exceeding the xattr size limit (default 64 KiB),
omap keys are used instead. The accessor falls back from xattr
to omap transparently.
### 7.2 Filesystem Backend
The development and single-node production backend.
    {root}/{group}/{version}/{resources}/{namespace}/blobs/{axis}/{first-two}/{κ-label}
    {root}/{group}/{version}/{resources}/{namespace}/manifests/{axis}/{first-two}/{κ-label}
    {root}/{group}/{version}/{resources}/{namespace}/index.json
    {root}/{group}/{version}/{resources}/{namespace}/graph/{source-κ}/{relation}/{target-κ}.cbor
    {root}/{group}/{version}/{resources}/{namespace}/uploads/{uuid}/data
    {root}/{group}/{version}/{resources}/{namespace}/uploads/{uuid}/state
    {root}/_crds/{group}/{version}/{resource}.json
Metadata is stored as extended attributes (xattr) on blob files.
On filesystems without xattr support, a sidecar `{κ-label}.meta`
JSON file is used.
The index file (`index.json`) contains tag→κ-label mappings as a
JSON object. Tag operations are atomic via rename-over-temp-file
(write to `index.json.tmp`, fsync, rename).
This backend requires no dependencies beyond POSIX filesystem
operations. It works on ext4, XFS, ZFS, tmpfs, 9p, NFS, and
CephFS.
### 7.3 S3-Compatible Backend
The cloud backend. Any S3-compatible object store (AWS S3, GCS,
R2, MinIO) serves as the storage substrate.
    Bucket: kappa-{deployment-id}
    Key prefix layout:
        blobs/{axis}/{first-two}/{κ-label}
        manifests/{axis}/{first-two}/{κ-label}
        index/{group}/{version}/{resource}/{namespace}/tags.json
        graph/{source-κ}/{relation}/{target-κ}.cbor
        uploads/{uuid}/data
        uploads/{uuid}/state
        _crds/{group}/{version}/{resource}.json
Metadata is stored as S3 object metadata headers (up to 2 KiB) or
as sidecar `{κ-label}.meta` objects for larger metadata.
Tag operations use S3's conditional PUT with `If-None-Match` for
create and versioned overwrites for update. S3's eventual
consistency model means tag reads may lag tag writes by a bounded
window (typically < 1 second for AWS S3 strong consistency mode).
### 7.4 SQLite / SQLCipher Backend
The local vault backend for single-device encrypted storage. This
is the backend described in CROSSPOLLINATION.md §9.
    CREATE TABLE kappa_store (
        kappa_label TEXT PRIMARY KEY NOT NULL
            CHECK(length(kappa_label) BETWEEN 71 AND 135),
        content BLOB NOT NULL,
        witness BLOB,
        created_at INTEGER NOT NULL
    );
    CREATE TABLE kappa_meta (
        kappa_label TEXT NOT NULL
            REFERENCES kappa_store(kappa_label),
        key TEXT NOT NULL,
        value BLOB NOT NULL,
        PRIMARY KEY (kappa_label, key)
    );
    CREATE TABLE kappa_index (
        path TEXT NOT NULL,
        tag TEXT NOT NULL,
        kappa_label TEXT NOT NULL
            REFERENCES kappa_store(kappa_label),
        PRIMARY KEY (path, tag)
    );
    CREATE TABLE kappa_graph (
        source TEXT NOT NULL,
        relation TEXT NOT NULL,
        target TEXT NOT NULL,
        metadata BLOB,
        PRIMARY KEY (source, relation, target)
    );
    CREATE INDEX idx_graph_target
        ON kappa_graph(target, relation);
For SQLCipher deployments, per-entry AES-256-GCM encryption is
applied to the `content` and `witness` columns on top of
SQLCipher's page-level encryption (double encryption as defined
in CROSSPOLLINATION-PRIMER.md §3.5.2).
### 7.5 In-Memory Backend
The test backend. Zero IO, zero persistence.
    struct MemKappaStore {
        objects: HashMap<KappaLabel, (Vec<u8>, HashMap<String, Vec<u8>>)>,
        tags: HashMap<(String, String), KappaLabel>,
        edges: Vec<Edge>,
    }
All operations are HashMap lookups. The entire store is lost on
process exit. Used for unit tests, integration tests, and
benchmarks that do not require persistence.
### 7.6 Backend Conformance
A backend is compliant if it passes the conformance test suite.
The test suite verifies:
1. Content immutability (put, put-duplicate, put-conflict)
2. Tag-before-content prohibition
3. Verify-on-put (incorrect κ-label rejected)
4. Idempotent put (same content, same κ-label, no error)
5. Edge source existence check
6. Edge target absence tolerance
7. Tag resolution round-trip
8. Metadata read/write round-trip
9. Graph edge query by direction and relation
10. List by prefix
11. GC reachability (pinned content survives sweep)
12. GC eviction (unpinned unreachable content removed after sweep)
The conformance test suite is implemented against the
fourteen-method interface and is backend-independent. Running the
suite against a new backend requires only implementing the
interface and passing the test harness the implementation.
---
## 8. Compatibility Projections
### 8.1 OCI Distribution v1.1.1 Projection
OCI Distribution clients (Docker, containerd, Podman, ORAS, Helm)
interact with the κ-store through an OCI-compatible projection.
The projection maps OCI operations to κ-Distribution operations:
    OCI Operation                    κ-Distribution Mapping
    -------------                    ----------------------
    GET /v2/                         GET /v2/ (version check)
    HEAD /v2/{name}/blobs/{digest}   HEAD /v2/{path}/blobs/{κ-label}
    GET /v2/{name}/blobs/{digest}    GET /v2/{path}/blobs/{κ-label}
    POST /v2/{name}/blobs/uploads/   POST /v2/{path}/blobs/uploads/
    PATCH upload                     PATCH upload (unchanged)
    PUT upload?digest=               PUT upload?kappa= (digest→κ-label)
    PUT /v2/{name}/manifests/{ref}   PUT /v2/{path}/manifests/{tag}
    GET /v2/{name}/manifests/{ref}   GET /v2/{path}/manifests/{tag}
    DELETE manifest                  DELETE manifest (unchanged)
    GET /v2/{name}/tags/list         GET /v2/{path}/tags/list
    GET /v2/{name}/referrers/{d}     GET /v2/{path}/graph/{κ}?relation=refers-to
The OCI `{name}` field maps to the κ-Distribution `{path}` field.
For OCI clients that do not use CRD-typed paths, the name is
treated as a flat namespace:
    docker pull registry.example.com/library/nginx:1.27
    → GET /v2/library/nginx/manifests/1.27
    → path = "_oci/library/nginx", tag = "1.27"
The `_oci` prefix is a reserved group for OCI-native content that
is not CRD-typed.
For SHA-256 digests, the OCI digest (`sha256:abcdef...`) is
byte-identical to the κ-label. No translation is needed. For
other algorithms, the OCI digest algorithm name and the κ-label
σ-axis name MUST match (both use lowercase algorithm names per
OCI Distribution specification §4).
The OCI Referrers API (`GET /v2/{name}/referrers/{digest}`) is
mapped to a graph query with `relation=refers-to` and
`direction=inbound`. The response format is the OCI Image Index
containing referrer manifests, as required by the OCI spec.
### 8.2 S3 Projection
S3 clients interact with the κ-store through the existing RGW S3
API. The same RADOS objects are accessible via S3 key paths:
    S3 key:  blobs/{axis}/{first-two}/{κ-label}
    S3 bucket: kappa-{namespace}
S3 operations map to RADOS operations that read/write the same
objects as κ-Distribution. A blob pushed via κ-Distribution is
readable via S3 `GetObject` using the blob's S3 key path. A blob
pushed via S3 `PutObject` is readable via κ-Distribution if the
S3 key follows the naming convention and the content hashes to the
κ-label encoded in the key.
The S3 projection does not provide tag resolution, graph queries,
witness retrieval, or composition operations. It provides raw
blob access for data pipeline tools that speak S3 but not
κ-Distribution.
---
## 9. Controller Integration
### 9.1 Watch Mechanism (OTEL Spans)
Tag updates emit OpenTelemetry spans for controller notification:
    span.name: "kappa.tag.update"
    span.attributes:
        kappa.group:     "{group}"
        kappa.version:   "{version}"
        kappa.resource:  "{resource}"
        kappa.namespace: "{namespace}"
        kappa.name:      "{name}"
        kappa.tag:       "{tag}"
        kappa.label:     "{κ-label}"
        kappa.previous:  "{previous-κ-label}" (empty if new tag)
        kappa.operation: "set" | "delete"
Controllers subscribe to OTEL spans via the OTEL collector's
routing configuration. The collector maintains a bounded event
buffer (configurable, default 1 hour). Disconnected controllers
reconnect and catch up from the buffer.
### 9.2 Controller Reconciliation Loop
    controller registers interest in GVR:
        subscribe to OTEL spans where
            kappa.group = "{group}" AND
            kappa.version = "{version}" AND
            kappa.resource = "{resource}"
    on span received:
        κ = span.attributes["kappa.label"]
        content = KappaStore::get(κ)
        desired_state = deserialize(content)
        observed_state = controller.observe()
        if desired_state != observed_state:
            controller.reconcile(desired_state)
            new_status = controller.observe()
            status_κ = uor_addr::json::address(serialize(new_status))
            KappaStore::put(status_κ, serialize(new_status))
            KappaIndex::tag_set(
                "{gvr}/{namespace}/{name}/status",
                "latest",
                status_κ
            )
            KappaGraph::edge_put(spec_κ, "spec-of", status_κ)
### 9.3 Informer Compatibility
The `d2-controller-runtime` library provides an informer
abstraction that translates κ-Distribution's OTEL-based watch
model to the `client-go` informer interface. Controllers written
against standard Kubernetes controller patterns work with import
path changes only:
    // Kubernetes client-go
    import "k8s.io/client-go/informers"
    // D2 / κ-Distribution
    import "braincraft.io/d2-controller-runtime/informers"
The informer internally subscribes to OTEL spans, maintains a
local cache (backed by the in-memory KappaStore backend), and
delivers events through the standard `AddFunc`, `UpdateFunc`,
`DeleteFunc` handler interface.
### 9.4 Admission Control
Admission webhooks receive the content bytes and the CRD schema
before the κ-label is computed:
    1. Client submits PUT /v2/{path}/manifests/{tag}
    2. Server validates content against CRD schema
    3. Server calls admission webhooks:
        POST {webhook-url}
        Body:
        {
            "path": "{path}",
            "tag": "{tag}",
            "content": "<base64>",
            "schema_kappa": "sha256:...",
            "user": "{authenticated-identity}"
        }
    4. Webhook returns: {"allowed": true} or {"allowed": false, "reason": "..."}
    5. If allowed: compute κ-label, store, tag, respond 201
    6. If denied: respond 403 with webhook reason
The webhook's decision is itself κ-labeled and stored as an
attestation:
    decision_κ = uor_addr::json::address(webhook_response)
    KappaStore::put(decision_κ, webhook_response)
    KappaGraph::edge_put(decision_κ, "attests", content_κ)
---
## 10. Distributed Behavior
### 10.1 Consistency Model
**Content (KappaStore):** Eventually consistent with
verify-on-receipt. Any replica serves any κ-label. The client
verifies integrity by re-hashing received content and comparing
against the requested κ-label. The server does not need to be
trusted. Reads scale horizontally through caching proxies and CDN
infrastructure — immutable content is infinitely cacheable.
**Tags (KappaIndex):** Sequentially consistent within a consensus
domain. All clients observing the same consensus domain see tag
updates in the same order. If client A observes tag T₁ before T₂,
all clients in the same domain observe T₁ before T₂ or do not
observe T₁.
**Consensus domains:** Scoped by CRD group. Tag updates within
`atlas.cern.ch/v1/calibrations` do not contend with tag updates
within `cms.cern.ch/v1/simulations`. CASPaxos (leaderless, parallel
writes to different keys) operates within each consensus domain
independently.
**Combined guarantee:** Once a tag update is acknowledged, the
referenced content is guaranteed to be readable from the local
store. This follows from the tag-before-content invariant (§5.4):
content must exist before tags can reference it, and content
durability is confirmed before tag update is submitted to consensus.
### 10.2 Consensus Integration
Consensus operates exclusively on KappaIndex::tag_set and
KappaIndex::tag_delete. No other operations require consensus.
Content operations (KappaStore) are idempotent — concurrent puts
of the same content from multiple writers are safe without
coordination. Graph operations (KappaGraph) are append-only —
concurrent edge creation is safe without coordination (duplicate
edges are idempotent per §5.3).
The consensus protocol is pluggable:
    Protocol     Latency    Leader    Suitable for
    --------     -------    ------    ------------
    Raft         ~1ms       yes       general purpose
    CASPaxos     ~1ms       no        independent key updates
    DARE         ~5µs       yes       RDMA-capable deployments
    Mu           ~1-2µs     no        microsecond requirements
The consensus log entry for a tag update is:
    {
        "path": "{gvr}/{namespace}/{name}",
        "tag": "{tag}",
        "kappa": "{κ-label}",
        "timestamp": "{HLC timestamp}",
        "previous": "{previous-κ-label}"
    }
The consensus log entry is small (< 500 bytes). Consensus never
stores content — it stores ordering decisions for tag pointers.
### 10.3 Replication
Content replicates via the backend's native replication mechanism:
    Backend        Replication mechanism
    -------        ---------------------
    RADOS          RGW multisite async replication
    S3             S3 Cross-Region Replication
    Filesystem     rsync, rclone, or filesystem-level replication
    SQLite         Application-level replication (backup/restore)
κ-labels are self-authenticating. The replication transport is
untrusted. Content received via replication MUST be verified by
re-hashing and comparing against the κ-label encoded in the
object name.
**Replication ordering.** Metadata (tags, edges) may replicate
faster than content (blobs). A remote site may receive a tag
pointing to a κ-label whose blob has not yet replicated. The
remote site MUST accept the tag binding. The remote site MUST
return 404 for content requests to the absent κ-label. The
remote site MAY initiate a federation fetch to resolve the absent
κ-label from the source site on demand.
**Federation fetch.**
    1. Client requests GET /v2/{path}/blobs/{κ-label}
    2. Local store returns 404 (blob absent)
    3. Server queries peer sites: GET /v2/{path}/blobs/{κ-label}
    4. Peer returns 200 with content
    5. Server verifies κ-label against received content
    6. Server stores content locally (KappaStore::put)
    7. Server returns content to client
Federation fetch is transparent to the client. The client sees a
single 200 response. The latency includes the cross-site round
trip.
### 10.4 Garbage Collection
GC operates by reachability from pin sets.
**Roots:** All pinned κ-labels (permanent pins, TTL pins that have
not expired, finalizer pins, upload session pins).
**Reachability:** A κ-label is reachable if:
- It is a root (pinned), OR
- It is the target of an `owns` edge from a reachable κ-label, OR
- It is the κ-label bound by a tag (tags imply reachability), OR
- It is the target of a `composed-of` edge from a reachable κ-label
All other κ-labels are GC-eligible.
**Sweep algorithm:**
    1. Snapshot the pin set (all pins at sweep start time)
    2. Snapshot the tag set (all tag→κ-label bindings)
    3. Compute root set = pinned ∪ tagged
    4. Walk KappaGraph from roots along `owns` and `composed-of`
       edges (breadth-first)
    5. Mark all visited κ-labels as reachable
    6. For all κ-labels in KappaStore not marked reachable:
       a. Delete associated edges from KappaGraph
       b. Delete content from KappaStore
       c. Record eviction in audit log
**Snapshot isolation:** Pins added after the snapshot protect their
subgraph from the NEXT sweep, not the current one. This prevents
the race condition where a newly-pinned subgraph is partially
evicted before the GC sees the new pin.
**Window parallelism:** In sharded deployments (§7.1), each of the
16 windows can be swept independently. The sweep for window W
considers only κ-labels whose `window_index()` equals W. This
reduces per-sweep wall time by up to 16x.
**Finalizer enforcement:** A κ-label with outstanding finalizer
pins is never evicted, even if it is otherwise unreachable. The
GC sweep reports outstanding finalizers in the status endpoint
(§6.9) so operators can identify stuck finalizers.
### 10.5 Burst Ingestion
Burst ingestion (e.g., detector fill cycles producing terabytes
per second) exercises three bottleneck surfaces:
**κ-label computation:** BLAKE3 at ~1 GiB/s per core. At 36 cores
per T640-class node, hash throughput is ~36 GiB/s per node.
Content hashing parallelizes trivially across cores.
**Content storage:** Bounded by backend write throughput. For
RADOS with DCPMM WAL, measured at ~83K writes/sec per consensus
group. For filesystem, bounded by fsync throughput.
**Tag consensus:** Bounded by consensus protocol throughput. For
Raft, ~10K tag updates/sec (single leader). For CASPaxos,
proportional to key space width (independent keys update in
parallel). For DARE/Mu, ~200K tag updates/sec per group.
The burst mitigation strategy:
1. Content is written to KappaStore without tags (idempotent,
   no consensus required). This scales with backend write
   throughput.
2. Tags are batched and committed via consensus in bulk
   (one consensus round per batch, not per tag). Batch size is
   tunable (default: 100 tag updates per consensus round).
3. Upload sessions with temporary pins protect content from GC
   during the window between content write and tag binding.
### 10.6 Multi-Site Topology
A multi-site deployment consists of N sites, each running the full
κ-Distribution stack (protocol handler + backend + consensus).
Sites are connected by the backend's replication mechanism (RGW
multisite, S3 CRR, etc.).
    Site A (Geneva)          Site B (Prévessin)       Site C (Fermilab)
    +-----------------+      +-----------------+      +-----------------+
    | κ-Distribution  |      | κ-Distribution  |      | κ-Distribution  |
    | protocol handler|      | protocol handler|      | protocol handler|
    +--------+--------+      +--------+--------+      +--------+--------+
             |                         |                         |
    +--------+--------+      +--------+--------+      +--------+--------+
    | Backend (RADOS) |      | Backend (RADOS) |      | Backend (RADOS) |
    +--------+--------+      +--------+--------+      +--------+--------+
             |                         |                         |
             +-----async replication---+-----async replication---+
Each site has independent consensus domains for its local tag
updates. Cross-site tag synchronization is via async replication
with conflict resolution:
    Conflict: Site A and Site B both update tag T at time t₁
    Resolution: the tag update with the higher HLC timestamp wins.
    If HLC timestamps are equal: the tag update with the
    lexicographically greater κ-label wins (deterministic tie-break).
Content is eventually consistent across sites via replication.
Federation fetch resolves cross-site content on demand.
---
## 11. Security Considerations
### 11.1 Verify-on-Receipt
Clients MUST re-hash received content and compare against the
requested κ-label. The server is not trusted for content integrity.
A compromised or buggy server that returns incorrect content is
detected by the client-side verification.
This property is stronger than TLS: TLS verifies the transport,
not the content. A correctly-TLS'd connection to a compromised
server delivers authenticated garbage. κ-label verification detects
the garbage regardless of transport security.
### 11.2 Tag Integrity
Tags are mutable and consensus-protected. A compromised consensus
node can redirect tags to point to arbitrary κ-labels. Mitigation:
consensus requires a quorum (majority of nodes must agree). An
attacker must compromise a majority of consensus nodes to redirect
a tag.
Applications requiring stronger tag integrity SHOULD use signed
tags: the tag value is a κ-label of a signed manifest that
includes the content κ-label and the signer's identity. Verifying
the manifest signature verifies the tag binding independently of
consensus integrity.
### 11.3 Graph Integrity
Graph edges are not consensus-protected. A compromised storage
backend can inject, modify, or delete edges. Mitigation: edges
created by composition operations carry witnesses that can be
replayed to verify the edge's derivation. Edges created by
application code (e.g., `owns`, `derives-from`) are not
witness-protected and must be verified by the application's own
integrity mechanisms.
### 11.4 GC Pin Manipulation
An attacker who can create pins can prevent garbage collection
(denial-of-service via pin flooding). An attacker who can delete
pins can cause premature eviction of pinned content.
Mitigation: pin operations MUST be authorized. The authorization
framework (§11.5) controls who can create and delete pins for
each namespace. Pin creation SHOULD be rate-limited per identity.
### 11.5 Authorization
κ-Distribution delegates authorization to the RGW auth framework.
The protocol handler calls `verify_permission()` before executing
each operation. The auth framework evaluates policies against the
authenticated identity and the requested operation.
Authorization policies are scoped by GVR path:
    {group}/{version}/{resources}/{namespace}
An identity authorized for `atlas.cern.ch/v1/calibrations/run3`
can read and write calibration resources in the `run3` namespace.
The identity cannot access `cms.cern.ch/v1/simulations/pythia8`
unless separately authorized.
### 11.6 Encryption at Rest
κ-Distribution delegates encryption at rest to the backend:
    Backend     Encryption mechanism
    -------     --------------------
    RADOS       BlueStore block-level encryption
    Filesystem  dm-crypt / LUKS / fscrypt
    S3          S3 SSE-S3 / SSE-KMS / SSE-C
    SQLCipher   Page-level AES-256-CBC + entry-level AES-256-GCM
The protocol does not define content-level encryption. Content
is stored as cleartext in the backend and encrypted by the
backend's native mechanism. Applications requiring content-level
encryption MUST encrypt before computing the κ-label — the κ-label
then covers the ciphertext, not the plaintext.
---
## 12. Performance Considerations
### 12.1 Operation Cost by Backend
    Operation       RADOS/NVMe       Filesystem      S3          SQLite
    ---------       ----------       ----------      --          ------
    put (1 KiB)     ~125µs           ~50µs           ~50ms       ~1ms
    get (1 KiB)     ~100µs           ~10µs           ~20ms       ~0.5ms
    exists          ~50µs            ~5µs (stat)     ~10ms       ~0.2ms
    tag_set         ~125µs + cons.   ~50µs + fsync   ~50ms       ~1ms
    tag_get         ~100µs           ~10µs           ~20ms       ~0.2ms
    edge_put        ~125µs           ~50µs           ~50ms       ~1ms
    edge_get        ~100µs           ~10µs           ~20ms       ~0.5ms
    meta_get        ~80µs (xattr)    ~5µs (xattr)    ~20ms       ~0.2ms
    GC sweep/obj    ~125µs           ~15µs           ~30ms       ~0.5ms
Consensus overhead (for tag_set only) adds:
    Raft:      ~1ms
    CASPaxos:  ~1ms (but parallel for different keys)
    DARE:      ~5µs (RDMA)
    Mu:        ~1-2µs (RDMA)
### 12.2 κ-Label Computation Cost
    Algorithm   Throughput/core   1 KiB    1 MiB    16 MiB
    ---------   ---------------   -----    -----    ------
    BLAKE3      ~1 GiB/s          ~1µs     ~1ms     ~16ms
    SHA-256     ~500 MiB/s        ~2µs     ~2ms     ~32ms
    SHA3-256    ~300 MiB/s        ~3µs     ~3ms     ~48ms
κ-label computation is the dominant cost for small objects and
negligible for large objects (dominated by backend write time).
### 12.3 Scalability Limits
    Dimension               Limit                  Bounded by
    ---------               -----                  ----------
    Objects per store        unbounded              backend capacity
    Tags per namespace      ~10M (omap)            RocksDB omap performance
    Edges per κ-label       ~100K (omap)           RocksDB omap performance
    Namespaces per store    ~10M                   RADOS object count
    Concurrent readers      unbounded              CDN/cache layer
    Concurrent writers      ~83K/s (DARE+DCPMM)    consensus throughput
    GC sweep rate           ~640K obj/s (16 win)   omap scan throughput
    Replication lag          ~0.5ms (same DC)       network RTT
                            ~50-150ms (cross DC)
    Federation fetch         ~50-150ms (cross DC)  network RTT + get
### 12.4 PG Distribution at Scale
At 247 PB with EC 4+2 on HDD, the `kappa-blobs` pool requires
sufficient PGs to distribute load across OSDs. The two-hex prefix
sharding ensures that RADOS object names distribute uniformly
across the PG hash space — the first two hex digits of the κ-label
effectively pre-shard the namespace into 256 buckets.
Recommended PG counts:
    Pool              PG count (3 nodes)   PG count (100+ nodes)
    ----              ------------------   ---------------------
    kappa-blobs       32                   4096
    kappa-manifests   32                   512
    kappa-index       32                   512
    kappa-graph       32                   512
    kappa-uploads     16                   128
---
## 13. Relationship to D2
D2 (Content-Addressed Distributed State Management) is the first
consumer of κ-Distribution. D2 implements the Kubernetes-compatible
control plane backed by κ-Distribution instead of etcd.
The relationship:
    D2 component              κ-Distribution layer
    ------------              --------------------
    Value storage             KappaStore (content, immutable)
    Ordering authority        KappaIndex consensus (tags, mutable)
    Event propagation         OTEL spans on tag_set (§9.1)
    Watch replacement         OTEL collector + informer (§9.3)
    Temporal queries          Timestamp-as-tag (§4.5)
    kine adapter              KappaIndex (tag_set → kine Create/Update)
    kcp integration           CRD registration (§6.10) + d2-controller-runtime
The `d2-kine` adapter translates kine's KV interface to
κ-Distribution's fourteen-method interface:
    kine Create(key, value)  → KappaStore::put(κ, value)
                             + KappaIndex::tag_set(key, "latest", κ)
    kine Get(key)            → KappaIndex::tag_get(key, "latest")
                             → KappaStore::get(κ)
    kine List(prefix)        → KappaIndex::tag_list(prefix)
    kine Delete(key)         → KappaIndex::tag_delete(key, "latest")
    kine Watch(prefix)       → OTEL subscription on kappa.path prefix
    kine Count(prefix)       → KappaIndex::tag_list(prefix).len()
---
## 14. Relationship to CROSSPOLLINATION
The CROSSPOLLINATION specification defines content in motion.
This specification defines content at rest. The κ-label is the
value that crosses the boundary.
    Application computes κ-label (uor-addr)
        |
        v
    CROSSPOLLINATION: transport layer
        carry κ-label in wire frames
        dedup via κ-label cache
        audit chain via κ-label links
        |
        v
    κ-Distribution: storage layer
        store content by κ-label (KappaStore)
        bind tags to κ-labels (KappaIndex)
        record relationships (KappaGraph)
        GC by reachability from pins
The transport's dedup cache (CROSSPOLLINATION §8.3) and the
storage's replay detection table (§7.4 SQLite backend) serve
the same function at different layers. The session-open
prepopulate and session-close flush (CROSSPOLLINATION-ADDENDUM §13)
bridge them.
Content received via the transport layer is stored via the
κ-Distribution storage interface. Content retrieved via the
κ-Distribution HTTP API may be transmitted via the transport layer.
Both paths use the same `KappaLabel<71>` type without conversion.
---
## 15. References
### 15.1 Normative References
    [RFC2119]       Bradner, S., "Key words for use in RFCs to Indicate
                    Requirement Levels", RFC 2119, March 1997.
    [OCI-DIST]      Open Container Initiative, "OCI Distribution
                    Specification v1.1.1", 2024.
    [PURL-SPEC]     Package URL Project, "Package URL (purl)
                    Specification", https://github.com/package-url/purl-spec.
    [RFC8949]       Bormann, C., Hoffman, P., "Concise Binary Object
                    Representation (CBOR)", RFC 8949, December 2020.
    [OPENAPI]       OpenAPI Initiative, "OpenAPI Specification v3.1.0",
                    https://spec.openapis.org/oas/v3.1.0.
    [CROSSPOLL]     Morgan, K. (2026). "κ-Label Unified Content Identity
                    Protocol", CROSSPOLLINATION.md.
    [CROSSADD]      Morgan, K. (2026). "κ-Label Unified Content Identity
                    Protocol — Addendum", CROSSPOLLINATION-ADDENDUM.md.
### 15.2 Informative References
    [D2-PAPER]      Morgan, K. (2026). "D2: Content-Addressed Distributed
                    State Management with Native OCI Persistence",
                    BrainCraft.io Research.
    [CEPH]          Weil, S. et al. (2006). "Ceph: A Scalable,
                    High-Performance Distributed File System", OSDI 2006.
    [DARE]          Poke, M., Hoefler, T. (2015). "DARE: High-Performance
                    State Machine Replication on RDMA Networks", HPDC 2015.
    [CASPAXOS]      Rystsov, D. (2018). "CASPaxos: Replicated State
                    Machines without Logs", arXiv:1802.07000.
    [KINE]          Rancher Labs, "kine: Kubernetes backend storage
                    shim", https://github.com/k3s-io/kine.
    [KCP]           kcp-dev, "kcp: Kubernetes-like control planes",
                    https://github.com/kcp-dev/kcp.
    [IPFS]          Benet, J. (2014). "IPFS - Content Addressed,
                    Versioned, P2P File System", arXiv:1407.3561.
    [ZOT]           Project Zot, "zot: OCI-native container image
                    registry", https://github.com/project-zot/zot.
    [SCITT]         IETF, "Supply Chain Integrity, Transparency, and
                    Trust (SCITT)", draft-ietf-scitt-architecture.
---
## Appendix A: Complete HTTP Endpoint Table
    Method  Path                                              Operation
    ------  ----                                              ---------
    GET     /v2/                                              version check
    HEAD    /v2/{path}/blobs/{κ}                              blob exists
    GET     /v2/{path}/blobs/{κ}                              blob retrieve
    PUT     /v2/{path}/blobs/{κ}                              blob push (monolithic)
    POST    /v2/{path}/blobs/uploads/                         upload start
    PATCH   /v2/{path}/blobs/uploads/{uuid}                   upload chunk
    PUT     /v2/{path}/blobs/uploads/{uuid}?kappa={κ}         upload complete
    DELETE  /v2/{path}/blobs/{κ}                              blob unpin
    GET     /v2/{path}/manifests/{tag-or-κ}                   tag resolve / content get
    PUT     /v2/{path}/manifests/{tag}                        tag bind
    DELETE  /v2/{path}/manifests/{tag-or-κ}                   tag delete
    GET     /v2/{path}/tags/list                              tag list
    GET     /v2/{path}/graph/{κ}                              edge query
    PUT     /v2/{path}/graph/{src-κ}/{rel}/{tgt-κ}            edge create
    DELETE  /v2/{path}/graph/{src-κ}/{rel}/{tgt-κ}            edge delete
    GET     /v2/{path}/witnesses/{κ}                          witness retrieve
    POST    /v2/{path}/compose/{op}                           composition
    GET     /v2/{path}/placement/{κ}                          placement query
    POST    /v2/{path}/gc/pin                                 GC pin
    POST    /v2/{path}/gc/unpin                               GC unpin
    POST    /v2/{path}/gc/sweep                               GC sweep trigger
    GET     /v2/{path}/gc/status                              GC status
    PUT     /v2/_crds/{group}/{version}/{resource}            CRD register
    GET     /v2/_crds/{group}/{version}/{resource}            CRD retrieve
    GET     /v2/_crds/                                        CRD list
    GET     /v2/_groups/                                      group discovery
    Total: 26 endpoints
---
## Appendix B: PURL Type Registration
    Type:         kappa
    Scheme:       pkg
    Monotonic:    no (tags are mutable)
    Default repo: none (namespace is the organizational scope)
    Description:  Content-addressed objects in a κ-Distribution store,
                  addressed by CRD-typed namespace hierarchy.
    Canonical format:
        pkg:kappa/{group}/{version}/{resources}/{namespace}/{name}@{tag}
    Examples:
        pkg:kappa/atlas.cern.ch/v1/calibrations/run3/alignment@latest
        pkg:kappa/atlas.cern.ch/v1/calibrations/run3/alignment@sha256:abc...
        pkg:kappa/braincraft.io/v1/images/production/node@v2.1.0
    Qualifiers:
        axis    σ-axis preference for cross-axis resolution
        format  content format filter (applied to labels)
        tier    data tier filter (applied to labels)
        (additional qualifiers are key=value label selectors)
    Subpath:
        Path within composite artifacts (layers within manifests,
        files within bundles).
---
## Appendix C: Conformance Test Vector Corpus
### C.1 Content Vectors
    Input: empty byte array (0 bytes)
    κ-label (sha256): sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
    κ-label (blake3): blake3:af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262
    Input: "hello" (5 bytes, UTF-8)
    κ-label (sha256): sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
    κ-label (blake3): blake3:ea8f163db38682925e4491c5e58d4bb3506ef8c14eb78a86e908c5624a67200f
    Input: 16 MiB of zeros
    κ-label (sha256): sha256:2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae
    (actual value varies; implementations MUST compute and compare)
### C.2 Tag Vectors
    Path: "test.example.com/v1/widgets/default/foo"
    Tag: "latest"
    κ-label: (computed from test content)
    Operation: tag_set → tag_get → compare → must match
### C.3 Edge Vectors
    Source: sha256:aaa... (test content A)
    Target: sha256:bbb... (test content B)
    Relation: "derives-from"
    Operation: edge_put → edge_get(source, "derives-from", Outbound)
               → must return edge with target = sha256:bbb...
    Operation: edge_get(target, "derives-from", Inbound)
               → must return edge with source = sha256:aaa...
### C.4 GC Vectors
    Setup:
        put(κ_A, content_A)         # root content
        put(κ_B, content_B)         # derived content
        edge_put(κ_A, "owns", κ_B)  # A owns B
        pin(κ_A, ttl=0)             # pin A permanently
    Sweep: κ_A and κ_B both survive (B reachable via owns edge)
    Unpin: unpin(κ_A)
    Sweep: both κ_A and κ_B evicted (neither pinned, no tags)
    Setup with tag:
        put(κ_C, content_C)
        tag_set("test/v1/widgets/ns/w", "v1", κ_C)
    Sweep: κ_C survives (tagged)
    tag_delete("test/v1/widgets/ns/w", "v1")
    Sweep: κ_C evicted (no tags, no pins)
### C.5 Composition Vectors
    Operands: κ_A = sha256:aaa..., κ_B = sha256:bbb...
    G2 product: compose_g2_product(κ_A, κ_B)
    Result: κ_C = sha256:ccc... (deterministic)
    Verify: compose_g2_product(κ_B, κ_A) == κ_C (commutative)
    Verify: edge exists (κ_C, "composed-of", κ_A)
    Verify: edge exists (κ_C, "composed-of", κ_B)
---
## Appendix D: CRD Self-Hosting Schema
The CRD registration mechanism is self-hosting: the schema for CRD
schemas is itself a CRD stored at `_crds/_system/v1/crddefinitions`.
    {
        "type": "object",
        "required": ["group", "version", "resource", "kind", "schema"],
        "properties": {
            "group": {
                "type": "string",
                "pattern": "^[a-z0-9]([a-z0-9-]*[a-z0-9])?(\\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)*$"
            },
            "version": {
                "type": "string",
                "pattern": "^v[0-9]+(alpha[0-9]+|beta[0-9]+)?$"
            },
            "resource": {
                "type": "string",
                "pattern": "^[a-z][a-z0-9]*$"
            },
            "kind": {
                "type": "string",
                "pattern": "^[A-Z][a-zA-Z0-9]*$"
            },
            "schema": {
                "type": "object",
                "description": "OpenAPI v3 structural schema"
            }
        }
    }
---
## Appendix E: OTEL Span Schema
    span.name:                  "kappa.tag.update"
    span.trace_id:              generated (UUID)
    span.span_id:               generated (UUID)
    span.start_time:            HLC timestamp (nanoseconds)
    span.attributes:
        kappa.group:            string (CRD group)
        kappa.version:          string (CRD version)
        kappa.resource:         string (CRD resource, plural)
        kappa.namespace:        string (resource namespace)
        kappa.name:             string (resource name)
        kappa.tag:              string (tag name)
        kappa.label:            string (71-char κ-label)
        kappa.previous:         string (71-char κ-label, empty if new)
        kappa.operation:        string ("set" | "delete")
        kappa.consensus.domain: string (CRD group, consensus scope)
        kappa.consensus.round:  u64 (consensus round number)
        kappa.site:             string (site identifier for multi-site)

---

## Appendix F: Multi-Site Conflict Resolution

### F.1 Tag Conflict Detection

When two sites independently update the same tag within the
replication window, both updates replicate to all sites. The
receiving site detects a conflict when it receives a tag update
whose `previous` field does not match the current local tag value.

    Site A: tag_set("calibrations/run3/alignment", "latest", κ_1)
            previous = κ_0
    Site B: tag_set("calibrations/run3/alignment", "latest", κ_2)
            previous = κ_0

Both updates have the same `previous` value (κ_0). When Site A
receives Site B's update, it sees `previous = κ_0` but the local
tag already points to κ_1. This is a conflict.

### F.2 Resolution Algorithm

Conflict resolution is deterministic and does not require
cross-site coordination:

    1. Compare HLC timestamps of the conflicting updates.
    2. The update with the strictly higher HLC timestamp wins.
    3. If HLC timestamps are equal (within the bounded skew
       window), compare κ-labels lexicographically.
    4. The update with the lexicographically greater κ-label wins.

This resolution is:
- Deterministic: all sites arrive at the same winner
- Commutative: order of update reception does not matter
- Idempotent: applying the same update twice has no effect

The losing update's content is NOT deleted — it remains in
KappaStore and is reachable via its κ-label. Only the tag binding
is resolved. Applications that need to detect conflicts can
subscribe to a conflict OTEL span:

    span.name: "kappa.tag.conflict"
    span.attributes:
        kappa.winner:    κ-label of winning update
        kappa.loser:     κ-label of losing update
        kappa.tag:       tag name
        kappa.site.a:    site that produced the winner
        kappa.site.b:    site that produced the loser

### F.3 Conflict Avoidance

The preferred pattern for conflict avoidance is site-scoped tags:

    Site A writes: tag "latest@site-a" → κ_1
    Site B writes: tag "latest@site-b" → κ_2

A controller at each site reconciles the site-scoped tags into
a global "latest" tag using application-specific merge logic.
This pattern moves conflict resolution from the protocol layer
(deterministic but potentially surprising) to the application
layer (explicit and domain-aware).

---

## Appendix G: Capacity Planning

### G.1 Storage Overhead per Object

    Component                Size          Notes
    ---------                ----          -----
    Content bytes            variable      the payload
    RADOS object metadata    ~200 bytes    BlueStore RocksDB entry
    Two-hex prefix sharding  0             encoded in object name
    κ-label in object name   71 bytes      ASCII, RocksDB key
    Witness (xattr)          ~2 KiB        postcard-serialized
    Labels (xattr)           ~100 bytes    typical CBOR label map
    Total overhead per obj   ~2.4 KiB      independent of content size

At 1 billion objects, the metadata overhead is ~2.4 TiB. This is
stored in the `kappa-index` pool on NVMe.

### G.2 Index Sizing

Each namespace index object contains omap keys for tags, pins,
and labels. Sizing per namespace:

    Component              Size per entry    Typical count     Total
    ---------              --------------    -------------     -----
    Tag entry              71 + 256 bytes    100 tags          ~32 KiB
    Pin entry              71 + 8 bytes      50 pins           ~4 KiB
    Label cache entry      71 + 100 bytes    100 objects       ~17 KiB
    Total per namespace    —                 —                 ~53 KiB

At 1.2M namespaces: 1.2M × 53 KiB = ~63 GiB of index data. This
fits in the NVMe-backed `kappa-index` pool with room to spare.

### G.3 Graph Sizing

Each graph edge is a separate RADOS object in the `kappa-graph`
pool:

    Edge object size        ~300 bytes       object name + omap
    Average edges/object    5-10             typical graph density

At 1 billion objects with 7 edges each: 7 billion edge objects ×
300 bytes = ~2.1 TiB of graph data. This is stored in the
`kappa-graph` pool on NVMe.

### G.4 Consensus Log Sizing

Each tag update produces a consensus log entry of ~500 bytes. At
100K tag updates/day, the consensus log grows at ~50 MiB/day. Log
compaction retains only the most recent entry per key, bounding
log size to (number of active tags × 500 bytes).

At 10M active tags: 10M × 500 = ~5 GiB of consensus log. This
is stored on Optane DCPMM for minimum-latency access.

### G.5 Network Bandwidth

Cross-site replication bandwidth for metadata (tags, edges):

    Component           Size/update    Rate              Bandwidth
    ---------           -----------    ----              ---------
    Tag replication     ~500 bytes     100K/day          ~580 KiB/s
    Edge replication    ~300 bytes     500K/day          ~1.7 MiB/s
    Total metadata      —              —                 ~2.3 MiB/s

Content replication bandwidth depends on ingestion rate and is
dominated by blob size, not protocol overhead.

---

## Appendix H: Monitoring and Observability

### H.1 Metrics

κ-Distribution exposes Prometheus-compatible metrics at
`/v2/_metrics`:

    kappa_store_objects_total
        gauge: total objects in KappaStore
        labels: axis, pool

    kappa_store_bytes_total
        gauge: total content bytes stored
        labels: axis, pool

    kappa_store_put_total
        counter: total put operations
        labels: axis, status (created, exists, conflict)

    kappa_store_get_total
        counter: total get operations
        labels: axis, status (found, not_found)

    kappa_index_tags_total
        gauge: total active tags
        labels: group, version, resource

    kappa_index_tag_set_total
        counter: total tag_set operations
        labels: group, version, resource, status

    kappa_graph_edges_total
        gauge: total edges
        labels: relation

    kappa_gc_sweep_duration_seconds
        histogram: sweep duration
        labels: window

    kappa_gc_objects_evicted_total
        counter: total objects evicted by GC

    kappa_gc_pins_active
        gauge: active pin count
        labels: type (permanent, ttl, finalizer, upload)

    kappa_consensus_round_duration_seconds
        histogram: consensus round latency
        labels: protocol (raft, caspaxos, dare)

    kappa_consensus_tag_updates_total
        counter: tag updates submitted to consensus
        labels: group, status (committed, failed)

    kappa_replication_lag_seconds
        gauge: estimated replication lag per peer site
        labels: peer_site

    kappa_federation_fetch_total
        counter: federation fetches triggered
        labels: peer_site, status (success, failed)

    kappa_admission_total
        counter: admission webhook calls
        labels: webhook, status (allowed, denied)

### H.2 Health Endpoints

    GET /v2/_health/live
        Response: 200 OK (process is alive)

    GET /v2/_health/ready
        Response: 200 OK (backend connected, consensus quorum met)
        Response: 503 Service Unavailable (not ready)

    GET /v2/_health/startup
        Response: 200 OK (initialization complete)
        Response: 503 Service Unavailable (still initializing)

### H.3 Audit Log

All mutating operations emit structured audit log entries:

    {
        "timestamp": "2026-07-19T14:30:00.123456789Z",
        "operation": "tag_set",
        "path": "atlas.cern.ch/v1/calibrations/run3/alignment",
        "tag": "latest",
        "kappa": "sha256:abcdef...",
        "previous": "sha256:789012...",
        "identity": "user:physicist@cern.ch",
        "consensus_round": 42,
        "site": "geneva"
    }

Audit log entries are themselves κ-labeled and chained using the
mechanism defined in CROSSPOLLINATION.md §8.4. The audit chain
links κ-Distribution operations to the transport-layer audit
chain when content crosses the transport boundary.

---

## Appendix I: Error Codes

    Code    HTTP    Name                        Description
    ----    ----    ----                        -----------
    K001    404     ContentNotFound             κ-label does not exist in store
    K002    409     ContentExists               put with existing κ-label (non-idempotent: different content)
    K003    409     KappaLabelMismatch          put content does not match provided κ-label
    K004    404     TagNotFound                 tag does not exist at path
    K005    404     EdgeNotFound                edge does not exist
    K006    409     TagContentAbsent            tag_set for κ-label not in store
    K007    422     SchemaValidationFailed      content fails CRD schema validation
    K008    409     SchemaConflict              CRD schema already registered with different content
    K009    409     PinConflict                 unpin blocked by outstanding finalizers
    K010    503     ConsensusUnavailable        consensus quorum not met
    K011    503     BackendUnavailable          storage backend unreachable
    K012    413     ContentTooLarge             content exceeds backend size limit
    K013    400     InvalidKappaLabel           κ-label fails format validation
    K014    400     InvalidTagName              tag name contains forbidden characters
    K015    400     InvalidPath                 GVR path does not parse
    K016    403     Unauthorized                identity not authorized for operation
    K017    429     RateLimited                 operation rate limit exceeded
    K018    404     WitnessNotFound             witness not stored for κ-label
    K019    400     InvalidComposition          composition operands invalid
    K020    404     GroupNotFound               CRD group not registered
    K021    409     FinalizedContent            content has outstanding finalizer pins
    K022    202     GCSweepAccepted             sweep triggered (async)
    K023    404     UploadSessionNotFound       upload session UUID not found
    K024    400     UploadSessionExpired        upload session TTL exceeded
    K025    409     QualifierMismatch           resolved content labels do not match qualifiers

Error responses MUST include a JSON body:

    {
        "code": "K003",
        "message": "κ-label sha256:abc... does not match content hash sha256:def..."
    }

---

## Appendix J: Wire Format Summary

### J.1 κ-Label on the Wire

In HTTP headers and JSON response bodies, κ-labels appear as
71-character ASCII strings:

    X-Kappa-Label: sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855

In RADOS object names, κ-labels appear as components of the
hierarchical object name:

    kappa:blob:sha256:e3:sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855

In transport wire frames (CROSSPOLLINATION.md §4.4), κ-labels
appear as 72-byte length-prefixed fields:

    offset 0:  label_len  (u8, value = 71)
    offset 1:  label_bytes (71 bytes ASCII)

In PURL serialization, κ-labels appear after the `@` delimiter:

    pkg:kappa/group/v1/resources/ns/name@sha256:e3b0c442...

All four representations carry the same 71 bytes. No encoding
or decoding is required between representations — the κ-label
is the same ASCII string in every context.

### J.2 Metadata on the Wire

Metadata values in HTTP responses are CBOR-encoded (RFC 8949).
The `Content-Type` header for metadata endpoints is
`application/cbor`.

Witness values are postcard-serialized. The `Content-Type` header
for witness endpoints is `application/octet-stream`.

Graph edge metadata in HTTP responses is CBOR-encoded within the
JSON response body (base64-encoded CBOR in the `metadata` field).

### J.3 Request/Response Content Types

    Endpoint                    Request Content-Type      Response Content-Type
    --------                    --------------------      ---------------------
    Blob push/pull              application/octet-stream  application/octet-stream
    Manifest push/pull          per manifest mediaType    per manifest mediaType
    Tag list                    —                         application/json
    Graph query                 —                         application/json
    Edge create                 application/cbor          —
    Witness retrieve            —                         application/octet-stream
    Composition                 application/json          application/json
    GC operations               application/json          application/json
    CRD register/retrieve       application/json          application/json
    Discovery                   —                         application/json
    Metrics                     —                         text/plain (Prometheus)
    Health                      —                         text/plain
