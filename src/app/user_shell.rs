//! Executable resolution for launches that run through the user's shell.
//!
//! Agent tabs and configured TUIs start through the user's interactive shell,
//! so an executable counts as installed when it is on the `PATH` that shell
//! ends up with after its rc files run, not only on the server's own `PATH`.
//! That `PATH` is probed off the event loop when the server starts and again
//! after a config reload. Callers never block: until the probe answers, an
//! executable missing from the server's `PATH` is [`Resolution::Unknown`], so
//! launches go ahead and let the shell report a missing program.

use std::ffi::OsString;
use std::path::Path;
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

/// What a launch through the user's shell would find for an executable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Resolution {
    Found,
    Missing,
    /// Not on the server's `PATH`, and the user shell's `PATH` is not known
    /// yet.
    Unknown,
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

    /// Forgets the probed `PATH`; results of probes already running are
    /// ignored.
    pub(crate) fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.state = ProbeState::NotStarted;
    }

    #[cfg(test)]
    pub(crate) fn set_probed_path_for_test(&mut self, path: Option<OsString>) {
        self.state = ProbeState::Ready(path);
    }

    #[cfg(test)]
    pub(crate) fn probe_started_for_test(&self) -> bool {
        self.state != ProbeState::NotStarted
    }

    #[cfg(test)]
    pub(crate) fn probed_path_for_test(&self) -> Option<&OsString> {
        match &self.state {
            ProbeState::Ready(path) => path.as_ref(),
            ProbeState::NotStarted | ProbeState::InFlight => None,
        }
    }
}

/// Resolves `executable` the way the user's shell started in `cwd` would.
/// A path (`./bin/tool`, `/opt/tool`) is resolved against `cwd` and never
/// searched on a `PATH`; a bare name is found on `server_has` (the server
/// process's own `PATH` lookup) or on the probed user-shell `PATH`.
fn resolve_executable(
    executable: &str,
    cwd: Option<&Path>,
    probe: &ProbeState,
    server_has: impl Fn(&str) -> bool,
) -> Resolution {
    if crate::platform::executable_is_path(executable) {
        let found = match cwd {
            Some(cwd) => cwd.join(executable).is_file(),
            None => Path::new(executable).is_file(),
        };
        return if found {
            Resolution::Found
        } else {
            Resolution::Missing
        };
    }
    if server_has(executable) {
        return Resolution::Found;
    }
    match probe {
        ProbeState::Ready(Some(path))
            if crate::platform::executable_on_search_path(executable, Some(path.as_os_str())) =>
        {
            Resolution::Found
        }
        ProbeState::Ready(_) => Resolution::Missing,
        ProbeState::NotStarted | ProbeState::InFlight => Resolution::Unknown,
    }
}

impl App {
    /// Whether `executable` is known to resolve through the user's shell.
    /// For listings: an executable whose lookup is still pending counts as
    /// absent.
    pub(crate) fn user_shell_executable_resolves(&mut self, executable: &str) -> bool {
        self.resolve_user_shell_executable(executable, None) == Resolution::Found
    }

    /// Whether `executable` is known not to resolve through the user's shell
    /// started in `cwd`. For launches: while the user shell's `PATH` is still
    /// being probed this is `false`, so the launch runs and a missing program
    /// surfaces through the pane's exit notice.
    pub(crate) fn user_shell_executable_missing(
        &mut self,
        executable: &str,
        cwd: Option<&Path>,
    ) -> bool {
        self.resolve_user_shell_executable(executable, cwd) == Resolution::Missing
    }

    fn resolve_user_shell_executable(
        &mut self,
        executable: &str,
        cwd: Option<&Path>,
    ) -> Resolution {
        self.start_user_shell_path_probe();
        resolve_executable(
            executable,
            cwd,
            &self.user_shell_path.state,
            crate::platform::executable_on_path,
        )
    }

    /// Starts the background `PATH` probe for the configured shell unless one
    /// is running or has answered for the current configuration.
    pub(super) fn start_user_shell_path_probe(&mut self) {
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

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "herdr-user-shell-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn executables_resolve_on_the_shell_path_or_the_server_path() {
        let dir = temp_dir("path");
        std::fs::write(dir.join("shell-only-tool"), "").unwrap();
        let ready = ProbeState::Ready(Some(OsString::from(&dir)));
        let resolve = |executable: &str, probe: &ProbeState| {
            resolve_executable(executable, None, probe, |exe| exe == "server-tool")
        };

        assert_eq!(resolve("shell-only-tool", &ready), Resolution::Found);
        assert_eq!(resolve("server-tool", &ready), Resolution::Found);
        assert_eq!(resolve("missing", &ready), Resolution::Missing);
        assert_eq!(
            resolve("shell-only-tool", &ProbeState::Ready(None)),
            Resolution::Missing
        );
        // Until the probe answers, only the server PATH can confirm a tool.
        assert_eq!(
            resolve("server-tool", &ProbeState::InFlight),
            Resolution::Found
        );
        assert_eq!(
            resolve("shell-only-tool", &ProbeState::InFlight),
            Resolution::Unknown
        );
        assert_eq!(
            resolve("missing", &ProbeState::NotStarted),
            Resolution::Unknown
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn relative_executables_resolve_against_the_launch_cwd() {
        let project = temp_dir("project");
        std::fs::create_dir_all(project.join("bin")).unwrap();
        std::fs::write(project.join("bin").join("dev-tui"), "").unwrap();
        let resolve = |executable: &str, cwd: Option<&Path>| {
            resolve_executable(executable, cwd, &ProbeState::InFlight, |_| false)
        };
        let cwd = Some(project.as_path());

        assert_eq!(resolve("./bin/dev-tui", cwd), Resolution::Found);
        assert_eq!(resolve("bin/dev-tui", cwd), Resolution::Found);
        assert_eq!(resolve("./bin/other", cwd), Resolution::Missing);
        let absolute = project.join("bin").join("dev-tui");
        assert_eq!(
            resolve(absolute.to_str().unwrap(), Some(Path::new("/"))),
            Resolution::Found
        );

        let _ = std::fs::remove_dir_all(project);
    }

    #[cfg(unix)]
    fn test_app() -> App {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        App::new(
            &crate::config::Config::default(),
            crate::app::AppPolicy::TEST,
            None,
            api_rx,
            crate::api::EventHub::default(),
        )
    }

    #[cfg(unix)]
    async fn deliver_next_probe_result(app: &mut App) -> u64 {
        loop {
            let event =
                tokio::time::timeout(std::time::Duration::from_secs(5), app.event_rx.recv())
                    .await
                    .expect("probe result")
                    .expect("event channel open");
            if let AppEvent::UserShellPathProbed { generation, .. } = &event {
                let generation = *generation;
                app.handle_internal_event(event);
                return generation;
            }
            app.handle_internal_event(event);
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn stale_probe_results_are_ignored_after_invalidation() {
        let dir = temp_dir("stale");
        let tool = "herdr-user-shell-stale-probe-tool";
        std::fs::write(dir.join(tool), "").unwrap();
        let probed = OsString::from(&dir);
        let mut app = test_app();
        app.state.default_shell = "/bin/sh".into();
        app.state.shell_mode = crate::config::ShellModeConfig::NonLogin;
        app.user_shell_path = UserShellPath::new(Arc::new(move |_, _| Some(probed.clone())));

        // Starts a probe, then a config reload invalidates it before it answers.
        assert!(!app.user_shell_executable_resolves(tool));
        let stale = app.user_shell_path.generation;
        app.user_shell_path.invalidate();

        assert_eq!(deliver_next_probe_result(&mut app).await, stale);
        assert!(
            !app.user_shell_executable_resolves(tool),
            "a probe started before the reload must not answer for the new configuration"
        );

        // The probe this lookup started for the current configuration does.
        assert_ne!(deliver_next_probe_result(&mut app).await, stale);
        assert!(app.user_shell_executable_resolves(tool));

        let _ = std::fs::remove_dir_all(dir);
    }
}
