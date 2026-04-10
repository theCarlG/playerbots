# `cmangos` — safe wrappers + `MockWorld` for playerbot tests

`cmangos` is the safe-API crate in the playerbots Rust workspace. It wraps
the raw FFI exported by `cmangos-sys` (bindgen on `cpp_wrapper/botffi.h`)
behind a single `World` trait, provides RAII guards (`OwnedList<T>` and
type aliases like `AuraList`, `UnitList`, `QuestLog`, …) for every list
the C++ side hands back, and ships a pure-Rust `MockWorld` so the AI crate
can be tested without CMaNGOS, a server, or a database.

Two implementations of `World` live in this crate:

- **`VtableWorld`** — production. Wraps a `*const BotCallbacks` + a
  `BotHandle`. Every list-returning method constructs an `OwnedList` whose
  `Drop` calls the matching `free_*` callback exactly once. `!Send +
  !Sync` by construction, so a borrow cannot cross a thread boundary.
- **`MockWorld`** — feature-gated under `#[cfg(any(test, feature =
  "mock"))]`. A pure-Rust in-memory `World` backed by `RefCell<MockState>`,
  with a recorded event log (`Vec<MockEvent>`) that tests assert on. Every
  `World` method has a real behaviour-producing body — no `unimplemented!`
  paths.

---

## Using `MockWorld` in your tests

### 1. Enable the `mock` feature in dev-dependencies

```toml
[dev-dependencies]
cmangos = { path = "../cmangos", features = ["mock"] }
```

The `playerbot` crate in this workspace already does this — unit tests in
`crates/playerbot/src/**/*.rs` and integration tests in
`crates/playerbot/tests/*.rs` get `MockWorld` for free.

### 2. Construct a `MockWorld`

Two construction paths, pick whichever reads better at the call site:

**Fluent chain** — convenience ctors for the common "one aura / one flag"
cases. Mirrors the legacy `TestInterface` API:

```rust
use cmangos::{MockWorld, SpellId};

let world = MockWorld::new()
    .with_aura(SpellId(11958))  // global aura: has_aura returns true for any unit
    .with_safe_pos()             // get_safe_position returns Some(...)
    .with_unit_dist(5.0);        // default distance for any unfiltered unit
```

**Builder** — fine-grained control for richer scenarios:

```rust
use cmangos::{MockWorld, BotWorldSnapshot, UnitHandle, SpellId};

let world = MockWorld::builder()
    .world_snap(BotWorldSnapshot::default())
    .knows_spell(133)                       // Fireball
    .spell_cooldown(133, 0)
    .item_in_bags(6948, 1)                  // Hearthstone, 1x
    .nearby_hostile(vec![UnitHandle(42)])
    .free_talent_points(51)
    .build();
```

Both paths return an owned `MockWorld`; tests pass `&world` wherever a
`&dyn World` is required.

### 3. Observe bot behaviour via the event log

Every action method on `MockWorld` (`cast_spell`, `move_to`, `say`,
`learn_spell`, `set_skill`, `destroy_all`, …) pushes a `MockEvent`
variant into an internal log. Tests read the log after driving the AI:

```rust
use cmangos::MockEvent;

// ... drive a BT tick that casts Ice Block ...

assert!(world.events().iter().any(|e| matches!(
    e,
    MockEvent::CastSpell { spell, .. } if *spell == SpellId(11958)
)));

world.clear_events();   // reset between ticks if needed
let last = world.last_event();
```

`MockEvent` variants include `CastSpell`, `CastSpellPos`, `MoveTo`,
`Follow`, `StopMoving`, `Attack`, `AutoAttack`, `Say`, `Whisper`,
`UseItem`, `LearnSpell`, `SetSkill`, `SetReputation`, `InventoryAddItem`,
`DestroyAll`, `RemoveAura`, and friends — see `mock.rs` for the full list.

### 4. Advance state between ticks

```rust
world.tick(500);                                    // advance simulated time 500ms,
                                                     // decay per-spell cooldowns
world.inject_aura(UnitHandle(42), aura_info);       // push an aura onto a unit
world.set_self_target(UnitHandle(42));              // update the bot's current target

// Escape hatch for anything the helpers don't cover:
world.with_state(|s| {
    s.world_snap.self_.hp_pct = 0.3;
});
```

---

## End-to-end example: tick a BT against `MockWorld`

The integration test at `crates/playerbot/tests/encounter_smoke.rs`
demonstrates the full pattern for driving an encounter FSM's BT against
a `MockWorld` and asserting on the event log. Simplified:

```rust
use cmangos::{BotRole, MockEvent, MockWorld, SpellId};
use playerbot_rs::bot::state::PlayerClass;
use playerbot_rs::encounters::{
    EncounterEvent, EncounterFsm,
    molten_core::baron_geddon::{AURA_LIVING_BOMB, BaronGeddonFsm},
};
use playerbot_rs::engine::{bt_nodes::{BtNode, BtResult}, macro_fsm::ActiveFsm};

let mut fsm = BaronGeddonFsm::default();
fsm.update(&EncounterEvent::CombatStarted, 1.0, 0);
let bt = fsm.phase_bt(ActiveFsm::Combat).unwrap();

let world = MockWorld::new().with_aura(AURA_LIVING_BOMB);
let mut owned = /* SmokeCtx with Blackboard / BotTimers / Throttles / BotSettings */;
let mut ctx = owned.make_ctx(&world, &fsm, PlayerClass::Mage, BotRole::DPS);

assert_eq!(bt.tick(&mut ctx), BtResult::Success);
assert!(world.events().iter().any(|e| matches!(
    e, MockEvent::CastSpell { spell, .. } if *spell == SpellId(11958)
)));
```

The `SmokeCtx` helper (owning the mutable bits of a `TickContext`) is
shown in full in `crates/playerbot/tests/encounter_smoke.rs`. In-tree
unit tests under `crates/playerbot/src/encounters/**` use
`engine::context::tests::{TestCtxOwned, make_encounter_ctx}` instead —
that pair is gated on `#[cfg(test)]` so it's only visible to unit tests
in the same crate, not to integration tests.

---

## More examples

- `crates/playerbot/tests/encounter_smoke.rs` — end-to-end smoke coverage
  across a handful of encounters, including class-gated BT branches.
- `crates/playerbot/src/encounters/molten_core/baron_geddon.rs` — in-tree
  `#[cfg(test)]` module with the full `make_encounter_ctx` pattern.
- `crates/playerbot/src/encounters/molten_core/ragnaros.rs` — multi-phase
  FSM tests driven purely on event sequences (no BT tick needed).
- `crates/playerbot/src/factory/**/*.rs` — `with_tx(&mut world, |tx|
  …)` pattern for testing `FactoryTransaction` bodies against `MockWorld`.
