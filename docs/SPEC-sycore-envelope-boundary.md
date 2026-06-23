# SyCore — The Kernel/Envelope Boundary

## Reconciling the Gordian-Envelope Credential Layer with the Pure Domain Kernel

Status: draft for review. **Companion to `SPEC-credential-envelope.md`** (FolkEngine
Part 4). That document records FolkEngine's decision to adopt **Gordian Envelope
as-is** as its credential and selective-disclosure format. This document does not
change that decision; it answers a narrower, downward-facing question:

> Part 4 is written for the FolkEngine **substrate** (`folkcore`: CAS, BLAKE3,
> ledger, FROST, identity). SyCore is the **pure scheduling kernel that sits
> beneath that substrate**. Which parts of Part 4, if any, reach down to the
> kernel — and what must the kernel expose (or never do) so the envelope layer can
> wrap it cleanly later?

The short answer: **almost none of Part 4 is kernel work, and that is the correct
result.** The envelope layer lives entirely in the substrate. The kernel's
obligation is to stay the kind of pure core the substrate can build envelopes *on
top of* — and to avoid three specific things that would make it un-wrappable. The
rest of this document states that boundary precisely.

Relationship to other documents:

- **Reads:** `SPEC-credential-envelope.md` (the decision being reconciled);
  `src/lib.rs`, `src/view.rs`, `src/state.rs`, `src/entity.rs`, `src/time.rs`,
  `src/ids.rs` (the kernel as it stands today).
- **Changes:** nothing. This is documentary. Where it recommends a future kernel
  action (e.g., a canonical-encoding test), it says so explicitly and defers it.

---

## Part 1 — Architecture framing

### 1.1 Three layers, envelopes in the middle one

```
  ┌──────────────────────────────────────────────────────────────┐
  │ Ecosystem        issuers · holders · verifiers                │
  │                  (consume signed / elided envelopes)          │
  ├──────────────────────────────────────────────────────────────┤
  │ Substrate        CAS (BLAKE3) · ledger (FROST) · identity (DID)│
  │ (folkcore)       ▸ §4.3 envelope module: SHA-256, dCBOR,       │
  │                    bc-envelope — the ONLY home of Part 4        │
  ├──────────────────────────────────────────────────────────────┤
  │ Kernel           SyCore: apply · query · view                 │
  │ (sycore)         pure scheduling truth — no I/O, no crypto,    │
  │                  no serialization in the public API           │
  └──────────────────────────────────────────────────────────────┘
```

Everything Part 4 introduces — the SHA-256 digest tree, dCBOR, `bc-envelope`,
elision, salting, the `'verifiedBy'` issuer signature — lives in the substrate's
envelope module. The kernel is one layer further down than the boundary Part 4
§4.3 draws, and the consequence is sharper than confinement: **the kernel does not
see envelopes even as opaque bytes.** The substrate stores and addresses bytes; the
kernel produces the *facts* those bytes are later built from, and never touches the
bytes themselves.

### 1.2 Two framings inherited from Part 4, both above the kernel

- **The two-hash boundary (§4.2).** BLAKE3 answers *where a disclosure's bytes
  live*; SHA-256 answers *which credential it is a disclosure of*. The kernel
  computes **neither** hash. It emits facts; the substrate addresses them (BLAKE3)
  and seals them (SHA-256). Elision changing the CAS address but not the envelope
  root is entirely a substrate-internal property.

- **`view` ↔ elision as two ends of one disclosure seam.** SyCore already performs
  selective disclosure — but *structurally*. The `view_for_*` projections return
  types deliberately narrower than `Federation`, so over-disclosure is impossible
  *at compile time* and trusted *because the kernel produced it*. Gordian elision
  performs selective disclosure *cryptographically*: from a fully-signed document,
  strip subtrees to digests, and a stranger who does not trust the kernel can still
  verify the issuer's signature over what remains. **Same disclosure boundaries,
  two enforcement mechanisms at two layers.** The kernel narrows by type; the
  substrate authenticates and discloses. They are complementary, not redundant —
  see §2 obligation 3 and the §4.9 disposition for why elision must not be framed
  as *replacing* the kernel's views.

---

## Part 2 — The seam contract

The obligations the kernel must honor so the substrate's envelope layer can wrap it.
Numbered for reference from the disposition table (Part 3). Several are **already
satisfied** by SyCore as written; those are "keep doing this," not "go do this."

### What SyCore MUST guarantee

1. **Stay pure and deterministic.** `apply` is a total, deterministic function of
   `(state, command)` — no clock, randomness, network, filesystem, or environment
   access. *Why it is load-bearing:* the substrate's canonical dCBOR encoding,
   BLAKE3 CAS addressing, and the §4.2 conformance vectors all assume the kernel
   produces byte-reproducible output for identical inputs. Non-determinism anywhere
   below the seam would make those vectors flaky and the CAS address unstable.
   *Status: satisfied* (`lib.rs` states the purity guarantee; `Federation` is plain
   data).

2. **Keep public types canonically encodable without ambiguity.** The kernel does
   not serialize, but every type that can cross the seam — `event::Event`, the
   `view::*` projections, and the `entity` / `ids` / `time` types they contain —
   must have *one* unambiguous logical form so the substrate can dCBOR-encode it
   deterministically. Concretely: no floats; no map-iteration-order leaking into
   ordering; stable field semantics. *Status: largely satisfied* — `Federation`
   uses `BTreeMap` (ordered, not `HashMap`), and the `view_for_*` functions build
   their `Vec`s by iterating those ordered maps, so output order is already
   deterministic. The integer-backed `time` primitives (`Time(u16)`, no `chrono`,
   no floats) are exactly right. This obligation makes that *implicit* property
   *explicit and testable* (see Part 4, Q1).

3. **Expose disclosure boundaries as discrete fields, aligned with elision
   targets.** The `view_for_*` projections already define per-actor boundaries;
   those projections are the units the substrate will sign-and-elide. Keep each
   disclosable fact a separate field or struct, not fused into a derived blob, so
   the substrate can elide at exactly the granularity the views imply. The existing
   `RedactedBusy { musician, slot }` is the model: two separable fields, so the
   substrate could envelope a busy-record and elide `musician` for one verifier and
   `slot` for another. *Status: satisfied* by the current `view` design.

4. **Keep identity as opaque, substrate-mappable IDs.** The `*Id` newtypes wrap a
   `String` and carry no identity semantics. The kernel must keep them opaque and
   must adopt **no** identity scheme of its own — not DIDs, not XIDs, not keys. The
   substrate maps these IDs to FolkEngine DIDs at the boundary (§4.5). *Status:
   satisfied* — `ids.rs` is pure string-newtypes; this is "keep doing this."

5. **Surface low-entropy fields un-fused.** Elision over a deterministic hash lets a
   low-entropy elided field be brute-forced, so the substrate must salt such fields
   at issuance (§4.6). The kernel cannot salt — salting is randomness, and
   randomness is impure — but it must not *defeat* salting by pre-combining
   low-entropy fields into a single composite value the substrate cannot separate
   and salt individually. SyCore's domain is dense with low-entropy fields; keeping
   them as discrete struct fields (which it already does) is what makes per-field
   salting possible later. *Status: satisfied structurally;* see the inventory in
   Part 4.

### What SyCore MUST NOT do

6. **No envelope, crypto, or substrate types in the kernel.** Never import or
   reference `bc-envelope` / `bc-components`, SHA-256, dCBOR, CAS, BLAKE3, FROST, or
   DIDs in the public API or core modules. This is §4.3 confinement pushed one layer
   down: where the substrate treats an envelope as opaque bytes, the kernel does not
   handle the bytes at all.

7. **No signing or hashing.** Both signatures in §4.4 — the ledger's FROST-Ed25519
   signature and the envelope's issuer signature — live above the kernel. The kernel
   emits facts; *attestation* of those facts is the substrate's job. A kernel that
   signed would duplicate a substrate responsibility and blur the §4.4
   domain-separation discipline.

8. **No serialization in the default / public API.** Keep `serde` behind an opt-in
   feature (today, `seed`) and out of the public type contract, so the substrate's
   dCBOR remains the **single** encoding of record. A kernel-owned serialization
   running alongside dCBOR re-creates exactly the "two ways to encode the same data"
   hazard that §4.1 forbids and that determinism exists to prevent.

9. **No revocation, TTL, or capability logic.** Credential revocation (§4.7) is
   share-revocation (UCAN / TTL) or ledger supersession — both substrate. The kernel
   may model *domain* supersession (e.g., unassigning a player) but must not conflate
   that with credential revocation: a `PlayerUnassigned` event is a scheduling fact,
   not a statement about a signed credential's validity.

---

## Part 3 — §-by-§ disposition

Verdicts: **No-impact** — kernel does nothing and sees nothing (pure FolkEngine);
**Constrains-seam** — kernel must expose / guarantee / avoid something now;
**Forward-looking** — no impact today, but names something the seam must be ready
for.

| Spec § | Topic | Verdict | Seam obligation / note |
|---|---|---|---|
| **§4.1** | "As-is" commitments (SHA-256 tree, dCBOR, `bc-envelope`) | **Split** | SHA-256 + `bc-envelope` → No-impact. The **dCBOR** resolution → Constrains-seam (**2, 8**): cross-seam types must be deterministically dCBOR-encodable, and the kernel must not run a rival encoding. |
| **§4.2** | Two-hash boundary (BLAKE3 address vs SHA-256 root) | **No-impact** | Kernel computes neither hash. Relies only on kernel **determinism (1)** so the substrate's conformance vectors are reproducible. |
| **§4.3** | Confinement of SHA-256 to one module | **Constrains-seam (foundational)** | The rule this whole reconciliation generalizes. Kernel sits *below* the confinement boundary → must never reference envelopes / crypto even as bytes (**6, 7**). |
| **§4.4** | Credential `domain-entry`; two nested signatures | **No-impact** | Both signatures and the ledger are above the kernel; kernel must not sign (**7**). Forward-looking: kernel `Event` / `view` types are candidate envelope *contents* (**3**). |
| **§4.5** | Identity boundary — no XIDs, DID-only | **Constrains-seam (already satisfied)** | Maps to **4**: kernel keeps opaque string-newtype IDs; substrate maps to DIDs. SyCore's `*Id` types are exactly the right shape. |
| **§4.6** | Salting mandatory for low-entropy fields | **Constrains-seam (genuinely new)** | Maps to **5**: kernel cannot salt (impure) but must keep low-entropy fields un-fused so the substrate can salt per field at issuance. |
| **§4.7** | Revocation at the FolkEngine layer, not the envelope | **No-impact** | Maps to **9**: no revocation / TTL / capability in the kernel. Nuance: kernel *domain* supersession (unassign) must not be conflated with credential revocation. |
| **§4.8** | Relationship to zk `ProofEnvelope` | **No-impact (deferred)** | Both elision and zk are substrate / ecosystem. Kernel facts are the *subject* of proofs, but neither mode reaches into the kernel. |
| **§4.9** | Application fit (medical records, credentials) | **Forward-looking** | **Key finding:** SyCore has *no* credential-shaped data today. Names candidate analogs — signed *assignment attestation*, *roster-membership credential*, *availability attestation* — so the seam (**3**) is designed for them. |
| **§4.10** | Implement-now / defer + open edges | **No-impact (roadmap)** | None is kernel work. Two items cast a kernel shadow: dCBOR resolution → **2**, salting rule → **5**. Open edges are substrate (one has a faint kernel echo — Part 4, Q3). |

**Distribution:** of ten sections, six are No-impact, three Constrain-the-seam (one
already satisfied, one foundational, one genuinely new), one is purely
Forward-looking. That distribution *is* the answer to "which parts of Part 4 apply
to SyCore": almost none directly. A reconciliation that found substantial kernel
work would mean the layering was wrong — the value here is the three seam
constraints plus the discipline to keep the other six from leaking downward.

---

## Part 4 — Open questions & forward-looking notes

### Q1 — Canonical-encodability as a *tested* property (obligation 2)

Today the kernel's deterministic, unambiguous encodability is *implicit* in its type
choices (`BTreeMap`, integer time, no floats). Open question: should SyCore add a
**test** (not production code) that asserts its cross-seam types have a deterministic
logical encoding — a golden ordering / golden-bytes fixture — mirroring the spec's
"invariant as a test, not prose" ethos (§4.2, §3.6)? *Recommendation:* yes, but
**deferred** until the substrate's dCBOR profile exists to test against; recorded
here as a standing recommendation, not done now (this deliverable is documentary).

### Q2 — Low-entropy field inventory (obligation 5)

The substrate must salt every elidable field whose value space is small enough to
enumerate. Below is the kernel's inventory as it stands, so the substrate has a
ready checklist when issuance paths are built. **None of this is kernel work** — the
kernel's only obligation is to keep these fields discrete (which it does); salting
happens at issuance, above the seam.

| Entropy class | Fields | Salting note |
|---|---|---|
| **Booleans (1 bit)** | `Venue.has_pit`, `Venue.has_organ`, `Venue.loading_dock`, `Program.requires_organ`, `Program.requires_pit` | Trivially brute-forced if elided; always salt. |
| **Small enums** | `Tier {Core,Sub,Extra}`, `Chair {Concertmaster,Principal,Section}`, `EventKind {Rehearsal,Performance}` | ≤ 3 values each; always salt. |
| **Dates & times** | `Date {year,month,day}`, `Time(minutes)`, `TimeSlot {date,start,duration_min}`, `Musician.unavailable[]`, `call_time`, `downbeat` | Calendar space is enumerable; salt when elided. The *most* sensitive class — busy-window times are precisely what `view_for_orchestra` already redacts structurally. |
| **Small integers** | `availability_pct` (0–100), `players_required`, `Work.duration_min`, `Venue.capacity` | Small or guessable-in-context ranges; salt when elided. |
| **Identifiers** | `MusicianId`, `OrchestraId`, `VenueId`, `ConcertId`, `EventId` | Often structured/low-entropy (`"M001"`, `"VEN-01"`) and enumerable; salt when elided. Also correlation handles — see Q3. |
| **Constrained strings** | `primary_instrument`, `instrument`, `series`, `stage_type`, `Work.forces` | Drawn from small real-world vocabularies; treat as low-entropy and salt. (`name`, `title`, `composer` are higher-entropy but cheap to salt anyway.) |

### Q3 — The correlator echo (§4.10 open edge)

The SHA-256 root is, by design, stable across a credential's disclosures (§4.2) —
which is exactly what a verifier colluding across contexts can correlate on. That is
a pure substrate concern. But the kernel's stable IDs flow *into* enveloped contents,
so they are correlation handles too. The kernel's opaque-ID discipline (obligation 4)
already leaves the substrate free to pseudonymize an ID per context at the boundary;
the note for the kernel is simply the contrapositive: **do not make an ID
load-bearing in a way that blocks per-context wrapping** — keep IDs opaque, comparable
only within their type, and never derive kernel behavior from an ID's internal
structure.

### Q4 — The first credential-shaped datum (§4.9)

SyCore has no issuer-signed credential today; it is pure scheduling. Naming the first
kernel fact to be wrapped as an envelope would let the seam be validated against a
concrete "credential proving ground" (§4.9). Candidates, in rough order of fit:

- **Assignment attestation** — "orchestra `O` attests musician `M` is assigned to
  concert `C`," wrapping a `PlayerAssigned` fact. A musician could disclose it (with
  the orchestra and concert elided or salted) to prove a commitment without revealing
  for whom.
- **Roster-membership credential** — "musician `M` is a `Tier`/`Chair` member of
  orchestra `O`," wrapping a `RosterEntry`.
- **Availability attestation** — a signed, selectively-disclosable blackout window.

This is a FolkEngine roadmap decision, recorded here as an open question; it is **not**
kernel work, and none of these requires a change to SyCore beyond the seam contract
already stated.

---

The thread, continued: Part 4 admitted a second hash, SHA-256, into FolkEngine —
sealed inside the substrate's envelope module so BLAKE3 monoculture survives
everywhere else. This companion pushes the same instinct one layer deeper. The
**kernel** admits *nothing* of Part 4: no envelope, no hash, no signature, no
serialization. Its contribution to the credential layer is to remain the pure,
deterministic, narrowly-typed core that the substrate can sign, address, and elide
*on top of* — and to keep three doors open (canonical-encodability, discrete
low-entropy fields, opaque mappable IDs) that a less disciplined kernel would have
quietly closed. The kernel narrows by type; the substrate seals and discloses by
crypto; and the cleanest evidence that the layering is right is that, asked what the
kernel must *do* for Part 4, the honest answer is: keep being what it already is.

*Companion to `SPEC-credential-envelope.md` (FolkEngine Part 4). Documentary; changes
no code. Reconciles the envelope credential layer with the SyCore pure kernel.*
