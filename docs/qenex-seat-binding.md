# QENEX seat binding

A QENEX **seat** is one bot's private workspace on a seat host. Each bot owns a
private working tree, and Bun must never write into another bot's state or into
the host's global state.

When Bun runs inside a seat, every root it writes to resolves inside that seat's
workspace, and a write outside of it is refused.

## Environment

| Variable | Meaning |
| --- | --- |
| `QENEX_SEAT` | The name of the seat, e.g. `coder`. Set and non-empty means seat-bound. |
| `QENEX_SEAT_WORKSPACE` | The absolute path of the seat workspace. It is the only writable root of a seat-bound Bun. |
| `QENEX_SEAT_ALLOW_GLOBAL` | Set to `1` to disable the confinement. The escape hatch for operator and maintenance use. |

## Behaviour

**Seat-bound** (`QENEX_SEAT` is set and not empty):

| Root | Resolves to |
| --- | --- |
| Package cache | `$QENEX_SEAT_WORKSPACE/.cache` |
| Configuration | `$QENEX_SEAT_WORKSPACE/.config` |
| State | `$QENEX_SEAT_WORKSPACE/.data` |
| Global install root | `$QENEX_SEAT_WORKSPACE/.data/install` |
| Global bin directory | `$QENEX_SEAT_WORKSPACE/.data/bin` |
| Scratch / temporary directory | `$QENEX_SEAT_WORKSPACE/.tmp` |

A root that is explicitly configured outside of the workspace
(`BUN_INSTALL_CACHE_DIR`, `BUN_INSTALL_GLOBAL_DIR`, `BUN_INSTALL_BIN`, or
`--global-dir`) is refused, and so is a path that only escapes the workspace
through `..`:

```
error: qenex seat coder: refusing to write outside the seat workspace: /root/.bun/install/cache
set QENEX_SEAT_ALLOW_GLOBAL=1 to allow global writes
```

Reading is not affected: system and toolchain locations stay readable, only
writes are confined.

**Seat identity.** A seat-bound Bun reports its seat in the version output:

```
$ bun --version
1.3.0
seat=coder workspace=/var/lib/qenex/seats/coder/workspace
```

An unbound Bun prints neither a seat nor a workspace, and its behaviour is
exactly the legacy, global one.

**Escape hatch.** `QENEX_SEAT_ALLOW_GLOBAL=1` disables the confinement: the
roots go back to their legacy global locations. The process stays seat-bound,
so it still reports its seat identity.

## Implementation

* `src/install/qenex_seat.rs` - the seat detection, the path resolution and
  the refusal.
* `src/install/PackageManager/PackageManagerDirectories.rs` - the package
  cache and the scratch directory.
* `src/install/PackageManager/PackageManagerOptions.rs` - the global install
  root and the global bin directory.
* `src/runtime/cli/mod.rs` - the seat identity in `bun --version`.

The unit tests in `src/install/qenex_seat.rs` cover the three states: unbound
(legacy behaviour), bound (the roots resolve inside the workspace) and bound
with a write outside of the workspace (refused).
