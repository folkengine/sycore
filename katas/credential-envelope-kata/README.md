# Credential Envelope Kata

> Rebuild the **two-hash boundary** that lets one signed credential be disclosed
> many ways without breaking its seal.

This kata is extracted from
[`examples/credential_envelope.rs`](../../examples/credential_envelope.rs) in
the SyCore repo, which demonstrates Part 4 of the FolkEngine spec
([`docs/SPEC-credential-envelope.md`](../../docs/SPEC-credential-envelope.md)).
You do not need the rest of the codebase to do it.

---

## The concept (from first principles)

A **credential** is a signed statement: an issuer attests some facts about a
subject. The hard part isn't signing — it's **selective disclosure**. A patient
holding a lab result wants to show a new doctor *only* the relevant fields, not
the whole record, and the doctor still wants the lab's original signature to
verify. How do you reveal a *subset* of signed data without re-signing?

The answer is a **Gordian Envelope**: a tree of `subject – predicate – object`
triples with a built-in Merkle-like **digest tree**. Each node's digest is
computed from its children's digests. The key operation is **elision**:

> Replace any subtree with *just its digest*. Every ancestor digest — including
> the root — stays the same, because the digest tree was already built from that
> subtree's digest. A signature over the root therefore still verifies.

That gives **holder-driven selective disclosure**: the issuer signs once; the
holder strips fields they don't want to share; the signature survives.

### Two hashes, two questions

This kata lives at a boundary where **two different hash functions** are in play,
and the whole design rests on understanding which operation touches which:

| Digest          | Hash    | Computed over              | Answers                          |
| --------------- | ------- | -------------------------- | -------------------------------- |
| **CAS address** | BLAKE3  | the envelope's dCBOR bytes | *where this exact blob lives*    |
| **Envelope root** | SHA-256 | the envelope's digest tree | *the seal that survives elision* |

The load-bearing consequence — the thing you will prove with tests:

> **Elision changes the CAS address but not the envelope root.** Eliding a
> subtree rewrites the serialized bytes (so the **BLAKE3 CAS address changes**),
> but is *defined* to leave every ancestor's SHA-256 digest unchanged (so the
> **SHA-256 root is invariant** and the signature still verifies).

So one credential has **one seal but many addresses**: the full disclosure and
each elided projection are distinct blobs sharing one root. BLAKE3 says *where a
disclosure lives*; SHA-256 says *which credential it is a disclosure of*.

### Why salt?

Elision relies on deterministic hashing — which means a **low-entropy** elided
field (a short id, a yes/no, a date) can be *brute-forced*: an attacker hashes
every candidate value until one matches the leftover digest, recovering the
"hidden" value. The countermeasure is **salt**: a random blob mixed into each
assertion at issuance, so its digest can't be reproduced from the value alone.
For credentials this is mandatory, not optional — the privacy property must be
*structural*, not hoped-for.

---

## The challenge

Implement the four functions in [`exercise/src/lib.rs`](exercise/src/lib.rs),
each currently `todo!()`:

| Function          | What it does                                                |
| ----------------- | ----------------------------------------------------------- |
| `build_credential` | Construct the salted, unsigned envelope from four facts.   |
| `issue`            | Wrap-then-sign with the issuer's key.                       |
| `cas_address`      | BLAKE3 over the envelope's dCBOR bytes.                     |
| `elide_concert`    | Remove the `assignedTo` assertion (holder-side disclosure). |

Then make the suite in [`exercise/tests/two_hash.rs`](exercise/tests/two_hash.rs)
pass — **in order**. The tests ramp from constructing the credential to proving
the full elision invariant.

```bash
cd exercise
cargo test          # 7 tests; all fail until you implement the stubs
```

You're done when all 7 pass. The order is load-bearing in the real flow too:
**salt → sign → elide**. The issuer salts low-entropy fields *before* signing;
the holder elides *after*. Eliding an unsalted low-entropy field would let a
verifier recover it from its digest.

### The test ramp

1. `credential_has_three_assertions` — basic construction.
2. `low_entropy_fields_are_salted` — salt is *random*, so two builds of the same
   facts must differ.
3. `signature_verifies_on_full_disclosure` — wrap-then-sign works.
4. `elision_changes_the_cas_address` — BLAKE3 changes.
5. `elision_preserves_the_envelope_root` — **SHA-256 is invariant** (the insight).
6. `signature_survives_elision` — the payoff.
7. `elided_disclosure_hides_the_concert` — the concert id is actually gone.

---

## Hints (open only if stuck)

<details>
<summary>API map — which <code>bc-envelope</code> calls you need</summary>

- `Envelope::new(subject)` — start an envelope with a subject.
- `.add_assertion_salted(predicate, object, salted: bool)` — add a triple; pass
  `true` to salt it.
- `.wrap()` — wrap the whole envelope so a signature can cover it as one digest.
- `.add_signature(&signer)` — attach a `verifiedBy` signature.
- `.digest()` — the SHA-256 envelope root (a `Digest`).
- `.tagged_cbor().to_cbor_data()` — the serialized dCBOR bytes.
- `.assertion_with_predicate("assignedTo")` — locate an assertion by predicate.
- `.elide_removing_target(&target)` — elide the subtree with that digest.
- `blake3::hash(bytes).as_bytes()` — BLAKE3 → `&[u8; 32]`.

</details>

<details>
<summary>Stuck on <code>elide_concert</code>?</summary>

The signed envelope's top-level assertion is the *signature*, not `assignedTo`
(that's now inside the wrap). Elision works by **digest**, and a subtree's
digest is identical wherever it appears. So get the `assignedTo` assertion from
the original unsigned `credential`, then call `elide_removing_target` on the
`signed` envelope — it will find and remove the matching subtree through the
wrap.

</details>

<details>
<summary>Why does the salt test compare two whole builds?</summary>

Salt is random, so `build_credential(...).digest()` differs every call. If you
forgot to salt (or used `add_assertion` instead of `add_assertion_salted`), the
construction is deterministic and the two digests would be **equal** — which is
exactly the brute-forceable state the test is there to catch.

</details>

The full worked answer is in [`solution/`](solution/).

---

## Where this lives in the real codebase

- **Source:** [`examples/credential_envelope.rs`](../../examples/credential_envelope.rs)
  — the runnable example (`cargo run --example credential_envelope`) that prints
  all six stages and asserts the same §4.2 vector.
- **Spec:** [`docs/SPEC-credential-envelope.md`](../../docs/SPEC-credential-envelope.md)
  §4.2 (the two-hash boundary), §4.6 (mandatory salting).

**How the real version differs:** in the repo, the four facts are not strings —
they come from the *pure SyCore kernel*. The example runs real `apply` calls
(`RegisterMusician → FoundOrchestra → AddToRoster → ProgramConcert →
AssignPlayer`) and pulls a typed `Event::PlayerAssigned` across the **seam**
between the kernel and the substrate. This kata simplifies that seam to plain
string inputs so you can focus on the envelope concept; everything from
"build the envelope" onward is faithful to the real code.
