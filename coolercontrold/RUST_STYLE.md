# Rust Style for coolercontrold

This is the authoritative style guide for the `coolercontrold` Rust daemon.

Design goals in priority order: **safety, performance, developer experience.**

Applies to all code under `coolercontrold/`. Does NOT apply to `coolercontrol-ui/` (Vue/TypeScript)
or `coolercontrol/` (Qt C++).

## Project Conventions

- Always write tests for the daemon when changing anything. Always verify and refactor.
- Prefer `.not()` (from `std::ops::Not`) over the `!` operator for boolean negation. It reads
  left-to-right and is the standard style in this codebase.
- Use plain `pub` visibility. Do not use `pub(crate)` or other restricted visibility modifiers.
- Comments are short and concise.

## Safety

### Control Flow

- Use **only simple, explicit control flow**. Avoid recursion unless the call stack depth is
  provably bounded by a compile-time constant.
- **Put a limit on everything.** Every loop, queue, and bounded collection must have a fixed upper
  bound. Unbounded growth is a latency spike and a memory hazard. Enforce limits with `assert!` or
  return an error at the boundary.
- Handle all errors. Never use `.unwrap()` or `.expect()` in production paths, only in tests or
  where the invariant is documented with a comment explaining why it cannot fail. Prefer `?`
  propagation and typed errors.
- **Do not react directly to external events.** The daemon runs on its own tick/batch cycle via
  Tokio. Avoid spawning unbounded tasks in response to external input; route through bounded queues
  or channels.
- Compound conditions are hard to reason about. Split `if a && b && c` into nested `if` branches so
  each case is individually visible and assertable.
- State invariants positively. `if index < length { /* ok */ }` is clearer than
  `if index >= length { /* not ok */ }`.

### Assertions

- **Assert pre/postconditions and invariants.** A function must not operate blindly on data it has
  not validated. The assertion density should average at least two per non-trivial function.
- Use `debug_assert!` for invariants that are too expensive to check in release (hot paths). Use
  `assert!` for invariants that must hold always.
- **Pair assertions:** validate data before writing it (e.g. to config, to hardware), and assert the
  expected state after reading it back.
- Split compound assertions: prefer `assert!(a); assert!(b);` over `assert!(a && b)`. The former
  gives a more precise failure location.
- Assert relationships of compile-time constants to document and enforce invariants:
  ```rust
  const _: () = assert!(MAX_FANS <= u8::MAX as usize);
  ```
- Assert the **positive space** (what is expected) AND the **negative space** (what must never
  happen). Tests must cover both valid and invalid inputs, and the transitions between them.

### Memory and Allocation

- Rust's ownership model prevents use-after-free. Lean on it. Avoid `unsafe` except in thin FFI
  wrappers (`liquidctl`, `nvml`, `libdrm`), and document every `unsafe` block with why it is sound.
- Avoid unbounded dynamic allocation in hot paths (e.g. inside sensor polling loops). Prefer
  pre-allocated `Vec`s with `.clear()` + `.extend()` over creating new collections per iteration.
- Avoid holding `Arc<Mutex<T>>` locks across `.await` points. Release locks before yielding to the
  executor.

### Prefer Static and Upfront Allocation

- **Allocate at startup, not at runtime.** Collections whose maximum size is known (device counts,
  channel counts, profile limits) should be allocated once at init and reused. Avoid growing
  collections during the main event loop.
- **Prefer stack over heap when size is compile-time known.** Use `[T; N]` arrays instead of
  `Vec<T>` when the length is a constant. Stack allocation is free; heap allocation has overhead and
  can fragment memory.
- **Always provide capacity hints.** Use `Vec::with_capacity(n)`, `HashMap::with_capacity(n)`, and
  `String::with_capacity(n)` whenever the expected size is known at construction. Unexpected
  reallocation is a hidden cost.
  ```rust
  // Good: no reallocation on push
  let mut duties = Vec::with_capacity(channel_count);
  // Bad: grows and reallocates unpredictably
  let mut duties = Vec::new();
  ```
- **Reuse buffers across iterations.** In polling loops, declare the buffer once before the loop and
  call `.clear()` + `.extend()` (or `.truncate(0)`) rather than allocating a new collection each
  tick.
  ```rust
  // Good: buffer lives outside the loop
  let mut readings: Vec<SensorReading> = Vec::with_capacity(MAX_SENSORS);
  loop {
      readings.clear();
      readings.extend(poll_sensors());
      process(&readings);
  }
  ```
- **Use `OnceLock` / `LazyLock` for shared static state.** For read-mostly data initialized once
  (regex patterns, constant lookup tables, device capability maps), prefer `std::sync::OnceLock` or
  `std::sync::LazyLock` over allocating on every access.
- **Prefer `Box<[T]>` over `Vec<T>` for fixed-after-init collections.** After building a collection
  that will never grow or shrink, convert it with `.into_boxed_slice()` to communicate immutability
  of size and drop the excess capacity.
- **Prefer arrays or fixed-size structs in engine hot paths.** The engine processors run on every
  tick. Use fixed-size arrays for fan duty tables and sensor snapshots so the compiler can reason
  about size and eliminate bounds checks.
- **Distinguish init-time and runtime allocation in code review.** A `Vec::new()` in `fn new()` or
  startup code is fine. The same call inside `async fn poll()` or a `loop {}` body is a red flag.
  Comment or pre-allocate.

### Variable Scope

- Declare variables at the **smallest possible scope**. Minimize the number of names in scope at any
  point to reduce misuse.
- **Do not introduce variables before they are needed.** Calculate and validate values close to
  where they are used (reduces place-of-check-to-place-of-use bugs).

### Function Length

- **Hard limit: 70 lines per function.** Art is born of constraints. Split functions so control flow
  lives in the parent and pure computation in helpers.
  - Push `if`/`match` **up** into parent functions.
  - Push `for`/iterator chains **down** into helper functions.
  - Keep leaf functions pure where possible; let the parent own state.

### Compiler Warnings

- The daemon compiles with `#![deny(warnings)]` (or equivalent `RUSTFLAGS=-D warnings`). All
  warnings are errors. Never suppress a warning without a documented reason.

## Performance

- Think about performance **at design time**. Back-of-the-envelope math before implementation
  catches order-of-magnitude mistakes that profiling cannot fix cheaply.
- Optimize in resource order: **network, disk, memory, CPU.** The hardware polling loop and SSE
  fan-out are the hottest paths; measure before micro-optimizing Tokio task overhead.
- **Batch work.** Amortize sensor reads, config writes, and SSE broadcasts. One syscall for 20
  channels beats 20 syscalls for one channel each.
- Be explicit. Do not rely on the compiler or Tokio runtime to do the right thing invisibly. If you
  need a specific thread pool, spawn it explicitly. If you need backpressure, implement it
  explicitly with bounded channels.
- Distinguish **control plane** (profile changes, config updates) from **data plane** (sensor
  polling, fan commands). The control plane can afford more overhead; the data plane must be
  predictable.
- In hot loops (e.g. `engine/` processors), extract the loop body into a standalone function with
  primitive arguments, not `&self`. This helps the compiler cache fields in registers and makes
  redundant computations visible.

## Developer Experience

### Naming

- `snake_case` for functions, variables, modules, files. `CamelCase` for types and traits. Follow
  Rust conventions.
- **Include units and qualifiers in names, sorted by descending significance:**
  - `temp_celsius_max` not `max_temp`
  - `duty_percent` not `duty`
  - `interval_ms` not `interval`
- Do not abbreviate unless the abbreviation is a well-known domain term (e.g. `rpm`, `pwm`, `nvml`).
  Prefer `temperature` over `temp` in struct fields.
- Use proper acronym capitalization: `NvmlDevice`, `HwmonSensor`, `SseEvent`, not `NVMLDevice`.
- When choosing related names, match character counts so they align in source:
  - `source` / `target` over `src` / `dst`
  - `current_duty` / `target_duty` align nicely
- When a function has a callback or helper, prefix it: `poll_sensors` / `poll_sensors_inner`.
- **Infuse names with meaning.** `channel_uid: UID` tells the reader what kind of value it is.
  `val: u32` does not.
- Order struct fields: data fields first, then type aliases, then `impl` methods.

### Function Ordering

- **`pub fn new` first.** When a struct has a constructor, it leads the `impl` block.
- **Then core logic and hot paths.** Functions central to the type's purpose, and anything on the
  data plane or per-tick path, sit above seldom-used helpers.
- **Auxiliary functions at the bottom.** Cluster infrequent setters, debug helpers, and rarely-used
  API methods at the bottom of the `impl`.

### Comments and Commits

- **Always say why.** Code explains what; comments explain why. If a decision is non-obvious,
  explain the rationale. If an `unsafe` block is required, document exactly why it is sound.
- **Always say how in tests.** Each test function should open with a comment describing its goal and
  methodology, so a reader can understand intent without reverse-engineering the assertions.
- Comments are sentences: capital letter, full stop (or colon if introducing code). End-of-line
  comments may be phrases.
- Write **descriptive commit messages** explaining why the change was made, not just what changed.
  The commit message is permanent; the PR description is not.

### Error Handling

- Use typed, structured errors (via `thiserror` or similar). Match on variants explicitly; do not
  stringify errors to pass them between layers.
- Return `Result` from functions that can fail due to operating errors (I/O, hardware, auth). Use
  `panic!`/`assert!` only for programmer errors (invariant violations).
- Simplify return types where possible. `()` beats `bool`, `bool` beats `Option<u64>`, `Option<u64>`
  beats `Result<Option<u64>, E>`. Each extra dimension at the call site multiplies the branches
  callers must handle.

### Off-By-One Errors

- **`index`, `count`, and `size` are distinct types.** Treat them as such:
  - `index` is 0-based; `count` is 1-based (`index + 1 == count` at the boundary).
  - `size` = `count × unit`.
- Use newtypes or at minimum named fields to prevent mixing these. A `ChannelIndex(usize)` cannot be
  accidentally added to a `ChannelCount(usize)`.
- Be explicit about integer division rounding:
  ```rust
  // Ceiling division, state intent clearly:
  let chunks = (len + chunk_size - 1) / chunk_size;
  // Or use a named helper:
  fn div_ceil(a: usize, b: usize) -> usize { (a + b - 1) / b }
  ```

### Dependencies

- coolercontrold has necessary dependencies (Tokio, Axum, serde, etc.). Do not add new dependencies
  casually. Ask: does this dependency justify its supply chain risk, compile time, and maintenance
  burden? Prefer implementing small utilities in-house over pulling in a crate for a single
  function.
- When a dependency is added, pin or document the minimum acceptable version and why.

### Formatting

- Run `cargo fmt` (via `make ci-fmt`) before every commit. Formatting is not negotiable.
- **Hard line length limit: 100 columns.** Trunk enforces this; do not fight it with `#[allow]`.
- 4-space indentation. Never tabs.
- Always add braces to `if`/`else` bodies unless the entire statement fits on one line. Prevents
  "goto fail" class bugs.

## Rust-Specific Patterns for coolercontrold

- **Async boundary hygiene:** Keep `async fn` at the API/repository layer. Pure computation and
  hardware math should be synchronous. Do not make a function `async` just because its caller is.
- **Prefer `moro_local::Scope::spawn` over `tokio::task::spawn_local` when a scope reference is
  available _and_ the spawn site is not itself inside an already-spawned task on that scope.**
  `tokio::task::spawn_local` requires the spawned future to be `'static`, which forces `Rc::clone`
  on every captured value. `moro_local::Scope::spawn` only requires the future to outlive the scope,
  so spawned tasks can borrow `&Rc<T>` instead of cloning. The main loop carries a
  `&'s Scope<'s, 's, Result<()>>` through `engine.process_scheduled_speeds`,
  `commander.update_speeds`, `fire_preloads`, `fire_lcd_update`, and similar paths. Use the scope
  for top-level spawn sites (a sync function called from the scope's body before any await point)
  like those listed above.
- **Nested-spawn caveat (moro_local 0.4.0):** `Scope::poll_jobs` holds `RefCell::borrow_mut()` on
  its `futures` field for the entire duration of `FuturesUnordered::poll_next`. Any task being
  polled in that scope that calls `scope.spawn` on the **same** scope panics with
  `RefCell already borrowed`. Creating a fresh `async_scope!` inside the polled task does not fix
  this, because awaiting the new scope blocks the caller; and nesting two scopes (sibling or
  parent/child) doesn't fix it because the outer scope's `borrow_mut` is still held while the inner
  scope's tasks are polled. For spawn sites that live **inside** a future that is itself spawned on
  the main-loop scope (e.g. `calibration::dispatch`'s deferred sustain task), use
  `tokio::task::spawn_local` with the necessary `Rc::clone`. Document the clone where it lands.
- **Actor pattern (`src/api/actor/`):** Messages passed through channels must have bounded queues.
  Document the expected queue depth and what happens when it fills.
- **Repository pattern (`src/repositories/`):** Each repository wraps a hardware access layer.
  Validate all data coming back from hardware before returning it up the stack. Hardware lies.
- **Engine processors (`src/engine/`):** These run on every tick. No allocations, no locks held
  across yields, no unbounded iteration. Assert loop bounds.
- **Config (`src/config/`):** Validate config at load time with explicit assertions on all fields. A
  config error caught at startup is cheaper than a panic at 3 AM.
- **gRPC/REST handlers:** Validate all inputs at the API boundary before passing into the engine or
  repositories. Do not let invalid external data reach business logic.
