# API Patches

Client-side corrections for known permanent bugs in the GW2 API.

## Why this exists

The GW2 API has a small number of well-documented, long-standing bugs where
endpoints consistently return incorrect data. Rather than exposing corrupt data
to callers, this module intercepts and corrects known-bad values transparently
after deserialization — so `client.minis().get(599)` returns the correct
`"Mini Spooky Skimmer"`, not `"((208738))"`.

The mechanism is the `Patchable` trait in `resource/mod.rs`. Every resource
type gets a no-op default impl from `#[gw2_resource]`. Types with known bugs
override `patch(&mut self)` here.

## When to add a patch

A patch is appropriate when **all three** of the following are true:

1. The bug is **documented** — on the GW2 wiki API talk pages, the developer
   forum archive, or a reproducible curl command showing the wrong value.
2. The bug is **permanent** — not a transient cache miss or propagation delay.
   If ANet has acknowledged it and it has persisted for months, it qualifies.
3. The correct value is **known** — either from another endpoint, the wiki, or
   a community-verified source. Don't guess.

Do not add patches for data that is merely absent, missing, or ambiguous —
only for cases where the API returns a specific wrong value and the right value
is verifiable.

## How to document a bug

Each patch file must open with a module-level doc comment (`//!`) that covers:

- **Which endpoint** is affected (`/v2/minis`)
- **What the bug is** — the exact wrong value and why it's wrong
- **How many entries** are affected (if it's a set of IDs)
- **What the correct value is** and where it comes from
- **A source link** — GW2 wiki API page, forum archive, or other reference
- **Last verified date** — so future maintainers know when it was last checked

Example structure:

```rust
//! Corrections for known bugs in the `/v2/endpoint` endpoint.
//!
//! ## Bug: Short description of the bug
//!
//! Detailed explanation of what is wrong and why. Include the exact wrong
//! values so the bug is clear without needing to hit the API.
//!
//! Affects N entries. The correct data is available from `/v2/other-endpoint`.
//!
//! Source: <https://wiki.guildwars2.com/wiki/API:2/endpoint>
```

## How to implement a patch

### Step 1 — Add `no_default_patch` to the resource macro

In the type definition, suppress the auto-generated no-op `Patchable` impl so
your manual one doesn't conflict:

```rust
// types/widgets.rs
#[gw2_resource(path = "widgets", paged, id_type = WidgetId, no_default_patch)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Widget {
    pub id: WidgetId,
    pub name: String,
    // ...
}
```

### Step 2 — Create `patches/widgets.rs`

```rust
//! Corrections for known bugs in the `/v2/widgets` endpoint.
//!
//! ## Bug: Widget 42 always returns name "MISSING_STRING"
//!
//! The localization key for widget 42 was never populated. The correct name
//! is "Fancy Widget" — verified via `/v2/items` (item #99999 references this
//! widget in its details) and the GW2 wiki.
//!
//! Source: <https://wiki.guildwars2.com/wiki/API:2/widgets>
//! Last verified: 2026-03-02

use crate::resource::Patchable;
use crate::types::widgets::Widget;

/// Corrections for widgets with broken names.
///
/// Each entry is `(widget_id, correct_name)`.
const NAME_PATCHES: &[(u32, &str)] = &[
    (42, "Fancy Widget"),
];

impl Patchable for Widget {
    fn patch(&mut self) {
        if let Some(&(_, name)) = NAME_PATCHES.iter().find(|&&(id, _)| id == self.id.0) {
            self.name = name.to_owned();
        }
    }
}
```

Keep the correction data in a `const` array — it keeps the `patch` function
logic simple and makes the data easy to audit at a glance.

### Step 3 — Register the module

In `patches/mod.rs`:

```rust
pub mod widgets;
```

That's it. `get()` and `get_many()` call `patch()` automatically. No changes
needed at any call site.

## Correction data format

Use `const` arrays of tuples — one tuple per affected ID. Choose the tuple
fields to match exactly what is being corrected. Common patterns:

| What's wrong | Tuple shape |
|---|---|
| Name only | `(id, &str)` |
| Name + one ID field | `(id, &str, u32)` |
| Multiple fields | `(id, CorrectData)` where `CorrectData` is a struct |

Align columns for readability. The array should be sorted by ID ascending.

## Keeping patches up to date

Each patch file ships with a `#[cfg(feature = "verify-patches")]` test module
that makes live API calls to verify the patch table is still accurate. Run it
whenever you update a patch table or want to confirm the bug is still present:

```sh
cargo test -p gw2-api --features verify-patches
```

Three tests run per patch file:

| Test | What it checks |
|---|---|
| `bug_still_present_in_*` | The bugged entries still return the wrong sentinel values. Fails if ANet fixed the bug — remove the fixed entries from the table. |
| `correct_values_still_match_*` | The correct values in the table still match what the authoritative endpoint returns. Fails if ANet renamed something. |
| `no_new_bugged_*` | No new entries with the buggy sentinel have appeared that the table doesn't cover. Fails if the bug spread to new IDs. |

When a test fails, read its output — it tells you exactly which IDs are affected
and what needs to change. After updating the table, re-run to confirm.

Always update the `Last verified` date in the module doc comment after a
successful run.

### When writing verification tests

Each test fetches raw JSON directly via `reqwest` (bypassing the patch layer)
so it sees the uncorrected API data. The raw structs used in tests are defined
locally in the `verify` module — they only need the fields the test cares about.

Note that not all API endpoints provide a clean back-reference to verify
correctness. For example, `/v2/items` for mini unlock items returns
`Consumable/Unlock` type with no `minipet_id` field — the authoritative source
for the correct name is the item name itself, not a structural link. Document
these limitations in the test file when they apply.
