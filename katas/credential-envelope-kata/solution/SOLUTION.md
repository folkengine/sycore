# Solution walkthrough

The four functions are short — the lesson is in *why* each line is the way it is.
The reference code is in [`src/lib.rs`](src/lib.rs).

## `build_credential` — salt at construction

```rust
Envelope::new(musician)
    .add_assertion_salted("assignedTo", concert, true)
    .add_assertion_salted("orchestra", orchestra, true)
    .add_assertion_salted("instrument", instrument, true)
```

The musician id is the **subject**; each fact is a salted **assertion**. The
`true` flag is the whole point of the `low_entropy_fields_are_salted` test: salt
is random, so two builds of identical facts produce different digest trees. Drop
the salt and the construction becomes deterministic — and any concert/orchestra/
instrument you later elide could be recovered by hashing the (small) set of
candidate values until one matches the leftover digest.

## `issue` — wrap, *then* sign

```rust
credential.wrap().add_signature(issuer)
```

`add_signature` on a bare envelope signs only the **subject**. By calling
`wrap()` first, the signature covers the *entire* credential — subject plus all
assertions — as one wrapped-root digest. That is precisely what lets the
signature survive elision: the holder later removes an assertion *inside* the
wrap, the wrapped-root digest is unchanged, so the signature over it still
verifies.

## `cas_address` — BLAKE3 over the bytes

```rust
let bytes = envelope.tagged_cbor().to_cbor_data();
*blake3::hash(&bytes).as_bytes()
```

This is the *other* hash. It is computed over the **serialized bytes**, not the
digest tree — so it answers a different question ("where does this exact blob
live?") and behaves differently under elision: when the bytes change, it changes.

## `elide_concert` — elision by digest

```rust
let concert_assertion = credential
    .assertion_with_predicate("assignedTo")
    .expect("credential has an assignedTo assertion");
signed.elide_removing_target(&concert_assertion)
```

The subtlety: by the time we hold `signed`, its top-level assertion is the
**signature**, not `assignedTo` (that assertion is now buried inside the wrap).
But elision works by **digest**, and a subtree's digest is identical wherever it
appears. So we take the `assignedTo` assertion's digest from the *original
unsigned* `credential` and ask the signed envelope to elide that target;
`elide_removing_target` walks the tree and replaces the matching subtree with its
digest.

## The invariant the tests prove

Put together:

- `cas_address(signed) != cas_address(elided)` — elision rewrote the bytes, so
  the **BLAKE3** address moved.
- `signed.digest() == elided.digest()` — elision left every ancestor **SHA-256**
  digest intact, so the root (the seal) is invariant.
- `elided.verify_signature_from(&verifier).is_ok()` — because the root didn't
  move, the issuer's signature still verifies.

One credential, one SHA-256 seal, many BLAKE3 addresses. **BLAKE3 says *where*;
SHA-256 says *which*.** That is the two-hash boundary, and it is the foundation
the whole credential layer is built on.
