//! Executable resolution for launches that run through the user's shell.
//!
//! Agent tabs and configured TUIs start through the user's interactive shell,
//! so an executable counts as installed when it is on the `PATH` that shell
//! ends up with after its rc files run, not only on the server's own `PATH`.
//! That `PATH` is probed once per server session (and again after a config
//! reload), off the event loop. Until the probe answers, or when it fails,
//! resolution uses the server's `PATH`, so callers never block.

use std::ffi::OsString;
use std::sync::Arc;

use super::App;
use crate::events::AppEvent;

#[cfg(not(test))]
const USER_SHELL_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// Probes the `PATH` of `shell` (login when the flag is set). Injected so tests
/// never start the developer's real shell.
pub(crate) type UserShellPathProbe = Arc<dyn Fn(&str, bool) -> Option<OsString> + Send + Sync>;

pub(crate) struct UserShellPath {
    state: ProbeState,
    generation: u64,
    probe: UserShellPathProbe,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProbeState {
    NotStarted,
    InFlight,
    /// `None` when there is no supported shell or the probe failed.
    Ready(Option<OsString>),
}

impl UserShellPath {
    pub(crate) fn new(probe: UserShellPathProbe) -> Self {
        Self {
            state: ProbeState::NotStarted,
            generation: 0,
            probe,
        }
    }

    #[cfg(not(test))]
    pub(crate) fn production() -> Self {
        Self::new(Arc::new(|shell, login| {
            crate::platform::probe_user_shell_path(shell, login, USER_SHELL_PROBE_TIMEOUT)
        }))
    }

    /// Forgets the probed `PATH` so the next resolution probes again.
    pub(crate) fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.state = ProbeState::NotStarted;
    }

    fn probed_path(&self) -> Option<&OsString> {
        match &self.state {
            ProbeState::Ready(path) => path.as_ref(),
            ProbeState::NotStarted | ProbeState::InFlight => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn set_probed_path_for_test(&mut self, path: Option<OsString>) {
        self.state = ProbeState::Ready(path);
    }

    #[cfg(test)]
    pub(crate) fn probe_started_for_test(&self) -> bool {
        self.state != ProbeState::NotStarted
    }
}

/// Whether `executable` resolves on the user shell's `PATH` or, as a fallback,
/// on `server_has` (the server process's own `PATH` lookup).
pub(crate) fn executable_resolves(
    executable: &str,
    user_shell_path: Option<&OsString>,
    server_has: impl Fn(&str) -> bool,
) -> bool {
    user_shell_path.is_some_and(|path| {
        crate::platform::executable_on_search_path(executable, Some(path.as_os_str()))
    }) || server_has(executable)
}

impl App {
    /// Resolves `executable` the way a launch through the user's shell finds
    /// it, starting the background `PATH` probe when none has run yet.
    pub(crate) fn user_shell_executable_resolves(&mut self, executable: &str) -> bool {
        self.ensure_user_shell_path_probe();
        executable_resolves(
            executable,
            self.user_shell_path.probed_path(),
            crate::platform::executable_on_path,
        )
    }

    fn ensure_user_shell_path_probe(&mut self) {
        if self.user_shell_path.state != ProbeState::NotStarted {
            return;
        }
        let Some((shell, login)) = crate::pane::user_shell(crate::pane::PaneShellConfig::new(
            &self.state.default_shell,
            self.state.shell_mode,
        )) else {
            self.user_shell_path.state = ProbeState::Ready(None);
            return;
        };
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            self.user_shell_path.state = ProbeState::Ready(None);
            return;
        };
        self.user_shell_path.state = ProbeState::InFlight;
        let generation = self.user_shell_path.generation;
        let probe = self.user_shell_path.probe.clone();
        let event_tx = self.event_tx.clone();
        runtime.spawn(async move {
            let path = tokio::task::spawn_blocking(move || probe(&shell, login))
                .await
                .ok()
                .flatten();
            let _ = event_tx
                .send(AppEvent::UserShellPathProbed { generation, path })
                .await;
        });
    }

    pub(crate) fn handle_user_shell_path_probed(
        &mut self,
        generation: u64,
        path: Option<OsString>,
    ) {
        if generation != self.user_shell_path.generation {
            return;
        }
        if path.is_none() {
            tracing::debug!("user shell PATH probe failed; resolving from the server PATH");
        }
        self.user_shell_path.state = ProbeState::Ready(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executable_resolves_on_the_shell_path_or_the_server_path() {
        let dir =
            std::env::temp_dir().join(format!("herdr-user-shell-path-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("shell-only-tool"), "").unwrap();
        let shell_path = OsString::from(&dir);

        assert!(executable_resolves(
            "shell-only-tool",
            Some(&shell_path),
            |_| false
        ));
        assert!(!executable_resolves("shell-only-tool", None, |_| false));
        assert!(executable_resolves(
            "server-tool",
            Some(&shell_path),
            |exe| { exe == "server-tool" }
        ));
        assert!(!executable_resolves("missing", Some(&shell_path), |_| {
            false
        }));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn stale_probe_results_are_ignored_after_invalidation() {
        let mut cache = UserShellPath::new(Arc::new(|_, _| None));
        cache.state = ProbeState::InFlight;
        let stale = cache.generation;
        cache.invalidate();
        assert_eq!(cache.state, ProbeState::NotStarted);
        assert_ne!(cache.generation, stale);
    }
}
