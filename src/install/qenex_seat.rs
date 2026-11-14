//! QENEX seat binding.
//!
//! A QENEX "seat" is one bot's private workspace on a seat host. When
//! `QENEX_SEAT` is set, Bun keeps every root it writes to inside that seat's
//! workspace (`$QENEX_SEAT_WORKSPACE`), so one bot can never write into
//! another bot's state or into the host's global state.
//!
//! The rules implemented here come from the shared QENEX seat binding
//! contract:
//!
//! * R1 - a process is seat-bound iff `QENEX_SEAT` is set and non-empty.
//! * R2 - when seat-bound, the writable roots resolve under the seat
//!   workspace, and a write outside of it is refused.
//! * R3 - reads stay global. Only writes are confined.
//! * R4 - the seat identity is observable: `seat=<name> workspace=<path>`.
//!
//! `QENEX_SEAT_ALLOW_GLOBAL=1` disables the confinement. It is the escape
//! hatch for operator and maintenance use: the process stays seat-bound (so it
//! still reports its identity), but it writes to the legacy global roots
//! again.

use bun_core::env_var;
use bun_core::fmt as bun_fmt;
use bun_paths::resolve_path::ParentEqual;

/// The name of the environment variable that names the seat.
pub const SEAT_ENV: &str = "QENEX_SEAT";

/// The name of the environment variable that holds the seat workspace.
pub const WORKSPACE_ENV: &str = "QENEX_SEAT_WORKSPACE";

/// The name of the environment variable that disables the confinement.
pub const ALLOW_GLOBAL_ENV: &str = "QENEX_SEAT_ALLOW_GLOBAL";

/// The value of `QENEX_SEAT_ALLOW_GLOBAL` that disables the confinement.
pub const ALLOW_GLOBAL_VALUE: &str = "1";

/// The seat a process runs in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeatBinding {
    /// The seat name, from `QENEX_SEAT`.
    pub name: Vec<u8>,
    /// The seat workspace, from `QENEX_SEAT_WORKSPACE`. It is the only
    /// writable root of a seat-bound process.
    pub workspace: Vec<u8>,
}

/// The writable roots of a seat-bound process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeatRoots {
    /// The package cache, `<workspace>/.cache`.
    pub cache_dir: Vec<u8>,
    /// The configuration directory, `<workspace>/.config`.
    pub config_dir: Vec<u8>,
    /// The state directory, `<workspace>/.data`.
    pub state_dir: Vec<u8>,
    /// The global install root, `<workspace>/.data/install`.
    pub install_root: Vec<u8>,
    /// The global bin directory, `<workspace>/.data/bin`.
    pub bin_dir: Vec<u8>,
    /// The scratch directory, `<workspace>/.tmp`.
    pub temp_dir: Vec<u8>,
}

/// A write that a seat-bound process is not allowed to make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeatWriteDenied {
    /// The seat that refused the write.
    pub seat: Vec<u8>,
    /// The path that was refused.
    pub path: Vec<u8>,
}

impl std::fmt::Display for SeatWriteDenied {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "qenex seat {}: refusing to write outside the seat workspace: {}\nset {}={} to allow global writes",
            bun_fmt::s(&self.seat),
            bun_fmt::s(&self.path),
            ALLOW_GLOBAL_ENV,
            ALLOW_GLOBAL_VALUE,
        )
    }
}

/// The seat environment of a process.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SeatEnv {
    /// `QENEX_SEAT`, when it is set and not empty.
    pub seat: Option<Vec<u8>>,
    /// `QENEX_SEAT_WORKSPACE`, when it is set and not empty.
    pub workspace: Option<Vec<u8>>,
    /// `QENEX_SEAT_ALLOW_GLOBAL=1`.
    pub allow_global: bool,
}

impl SeatEnv {
    /// Reads the seat environment of this process.
    pub fn read() -> Self {
        Self {
            seat: env_var::QENEX_SEAT::get()
                .filter(|value| !value.is_empty())
                .map(|value| value.to_vec()),
            workspace: env_var::QENEX_SEAT_WORKSPACE::get()
                .filter(|value| !value.is_empty())
                .map(|value| value.to_vec()),
            allow_global: env_var::QENEX_SEAT_ALLOW_GLOBAL::get()
                .is_some_and(|value| value == ALLOW_GLOBAL_VALUE.as_bytes()),
        }
    }

    /// R1: a process is seat-bound iff `QENEX_SEAT` is set and non-empty.
    pub fn is_seat_bound(&self) -> bool {
        self.seat.is_some()
    }

    /// The seat binding of this process, when it is seat-bound and the
    /// workspace is known.
    pub fn binding(&self) -> Option<SeatBinding> {
        Some(SeatBinding {
            name: self.seat.clone()?,
            workspace: self.workspace.clone()?,
        })
    }

    /// The writable roots of a seat-bound process.
    ///
    /// `None` when the process is not seat-bound, when the workspace is
    /// unknown, or when the confinement is disabled with
    /// `QENEX_SEAT_ALLOW_GLOBAL=1`. In those cases the legacy, global roots
    /// are used.
    pub fn roots(&self) -> Option<SeatRoots> {
        if self.allow_global {
            return None;
        }
        let binding = self.binding()?;
        Some(SeatRoots {
            cache_dir: join(&binding.workspace, b".cache"),
            config_dir: join(&binding.workspace, b".config"),
            state_dir: join(&binding.workspace, b".data"),
            install_root: join(&binding.workspace, b".data/install"),
            bin_dir: join(&binding.workspace, b".data/bin"),
            temp_dir: join(&binding.workspace, b".tmp"),
        })
    }

    /// R2: refuse a write outside of the seat workspace.
    pub fn ensure_writable(&self, path: &[u8]) -> Result<(), SeatWriteDenied> {
        let Some(binding) = self.binding() else {
            return Ok(());
        };
        if self.allow_global {
            return Ok(());
        }
        if is_inside(&binding.workspace, path) {
            return Ok(());
        }
        Err(SeatWriteDenied {
            seat: binding.name,
            path: path.to_vec(),
        })
    }
}

/// The writable roots of this process, when it is seat-bound and confined.
pub fn roots() -> Option<SeatRoots> {
    SeatEnv::read().roots()
}

/// Whether this process is seat-bound.
pub fn is_seat_bound() -> bool {
    SeatEnv::read().is_seat_bound()
}

/// The seat identity of this process, as `seat=<name> workspace=<path>`.
///
/// `None` when the process is not seat-bound, so an unbound Bun never claims
/// a seat.
pub fn identity_line() -> Option<Vec<u8>> {
    let binding = SeatEnv::read().binding()?;
    let mut line = Vec::new();
    line.extend_from_slice(b"seat=");
    line.extend_from_slice(&binding.name);
    line.extend_from_slice(b" workspace=");
    line.extend_from_slice(&binding.workspace);
    line.push(b'\n');
    Some(line)
}

/// R2: refuse a write outside of the seat workspace of this process.
pub fn ensure_writable(path: &[u8]) -> Result<(), SeatWriteDenied> {
    SeatEnv::read().ensure_writable(path)
}

/// Prints the refusal and exits. Used where the package manager cannot hand
/// the error back to its caller.
pub fn refuse_write(denied: &SeatWriteDenied) -> ! {
    bun_core::Output::err_generic("{}", (denied,));
    bun_core::Global::exit(1);
}

/// Joins a workspace path with a relative part.
fn join(base: &[u8], part: &[u8]) -> Vec<u8> {
    let mut joined = Vec::with_capacity(base.len() + part.len() + 1);
    joined.extend_from_slice(base);
    while joined.last() == Some(&b'/') {
        joined.pop();
    }
    joined.push(b'/');
    joined.extend_from_slice(part);
    joined
}

/// Whether `candidate` is `workspace` itself or lives under it.
///
/// The comparison is lexical: both paths are normalized first, so `..` and
/// `.` cannot be used to escape the workspace.
fn is_inside(workspace: &[u8], candidate: &[u8]) -> bool {
    let mut workspace_buf = vec![0u8; workspace.len() + 1];
    let mut candidate_buf = vec![0u8; candidate.len() + 1];
    let normalized_workspace = bun_paths::resolve_path::normalize_buf::<bun_paths::platform::Auto>(
        workspace,
        &mut workspace_buf,
    );
    let normalized_candidate = bun_paths::resolve_path::normalize_buf::<bun_paths::platform::Auto>(
        candidate,
        &mut candidate_buf,
    );
    !matches!(
        bun_paths::resolve_path::is_parent_or_equal(normalized_workspace, normalized_candidate),
        ParentEqual::Unrelated
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seat_env(seat: Option<&[u8]>, workspace: Option<&[u8]>, allow_global: bool) -> SeatEnv {
        SeatEnv {
            seat: seat.map(|value| value.to_vec()),
            workspace: workspace.map(|value| value.to_vec()),
            allow_global,
        }
    }

    /// R7.1: an unbound process keeps the legacy, global behaviour.
    #[test]
    fn unbound_keeps_the_legacy_global_behaviour() {
        let env = seat_env(None, None, false);
        assert!(!env.is_seat_bound());
        assert!(env.binding().is_none());
        assert!(env.roots().is_none());
        assert!(env.ensure_writable(b"/root/.bun/install/cache").is_ok());
    }

    /// R7.2: a seat-bound process resolves its roots inside the workspace.
    #[test]
    fn bound_resolves_its_roots_inside_the_workspace() {
        let workspace = b"/var/lib/qenex/seats/coder/workspace";
        let env = seat_env(Some(&b"coder"[..]), Some(&workspace[..]), false);
        assert!(env.is_seat_bound());
        let roots = env.roots().expect("a seat-bound process has roots");
        for root in [
            &roots.cache_dir,
            &roots.config_dir,
            &roots.state_dir,
            &roots.install_root,
            &roots.bin_dir,
            &roots.temp_dir,
        ] {
            assert!(is_inside(workspace, root), "{root:?} is outside the workspace");
        }
        assert!(env.ensure_writable(&roots.cache_dir).is_ok());
    }

    /// R7.3: a seat-bound process refuses to write outside the workspace.
    #[test]
    fn bound_refuses_a_write_outside_the_workspace() {
        let env = seat_env(
            Some(&b"coder"[..]),
            Some(&b"/var/lib/qenex/seats/coder/workspace"[..]),
            false,
        );
        let denied = env
            .ensure_writable(b"/root/.bun/install/cache")
            .expect_err("a write outside the workspace is refused");
        assert_eq!(denied.seat, b"coder".to_vec());
        assert_eq!(denied.path, b"/root/.bun/install/cache".to_vec());
        assert_eq!(
            denied.to_string(),
            format!(
                "qenex seat {}: refusing to write outside the seat workspace: {}\nset {}={} to allow global writes",
                bun_fmt::s(&denied.seat),
                bun_fmt::s(&denied.path),
                ALLOW_GLOBAL_ENV,
                ALLOW_GLOBAL_VALUE,
            )
        );
    }

    /// The escape hatch disables the confinement but keeps the identity.
    #[test]
    fn allow_global_disables_the_confinement() {
        let env = seat_env(
            Some(&b"coder"[..]),
            Some(&b"/var/lib/qenex/seats/coder/workspace"[..]),
            true,
        );
        assert!(env.is_seat_bound());
        assert!(env.roots().is_none());
        assert!(env.ensure_writable(b"/root/.bun/install/cache").is_ok());
    }

    /// A path that only looks like the workspace is outside of it.
    #[test]
    fn a_sibling_of_the_workspace_is_outside_of_it() {
        let env = seat_env(
            Some(&b"coder"[..]),
            Some(&b"/var/lib/qenex/seats/coder/workspace"[..]),
            false,
        );
        assert!(env
            .ensure_writable(b"/var/lib/qenex/seats/coder/workspace-other/cache")
            .is_err());
        assert!(env
            .ensure_writable(b"/var/lib/qenex/seats/coder/workspace/../other/cache")
            .is_err());
    }
}
