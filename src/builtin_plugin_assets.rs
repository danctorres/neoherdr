use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::api::schema::InstalledPluginInfo;

/// `(plugin id, manifest, [(bin file name, contents)])`.
type BundledPlugin = (
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
);

const PLUGINS: &[BundledPlugin] = &[(
    "worktrunk",
    include_str!("../plugins/worktrunk/herdr-plugin.toml"),
    &[
        ("open", include_str!("../plugins/worktrunk/bin/open")),
        ("run", include_str!("../plugins/worktrunk/bin/run")),
    ],
)];

/// Default which-key entries for bundled plugin actions, as
/// `(menu id, key, plugin action id, description)`. Keeping them next to the
/// bundled plugin table keeps plugin specifics out of the core keymap.
pub(crate) fn default_group_members(
) -> &'static [(&'static str, &'static str, &'static str, &'static str)] {
    // The worktrunk manifest only supports Linux and macOS.
    if cfg!(windows) {
        return &[];
    }
    &[
        (
            "git",
            "s",
            "worktrunk.switch",
            "worktrunk: switch or create",
        ),
        ("git", "l", "worktrunk.list", "worktrunk: list worktrees"),
        ("git", "m", "worktrunk.merge", "worktrunk: merge worktree"),
    ]
}

/// Whether `plugin_id` names a plugin bundled with the binary.
pub(crate) fn is_bundled(plugin_id: &str) -> bool {
    PLUGINS.iter().any(|(id, _, _)| *id == plugin_id)
}

/// Registers every bundled plugin that is neither registered nor declined,
/// and refreshes the on-disk assets of bundled plugins the registry still
/// uses. Declined plugins, and ids taken by a plugin from another source, get
/// no files written. Returns whether `plugins` changed.
pub(crate) fn register_bundled(
    plugins: &mut Vec<InstalledPluginInfo>,
    declined: &BTreeSet<String>,
) -> bool {
    register_bundled_with(plugins, declined, crate::plugin_paths::builtin_plugin_dir)
}

fn register_bundled_with(
    plugins: &mut Vec<InstalledPluginInfo>,
    declined: &BTreeSet<String>,
    root_for: impl Fn(&str) -> PathBuf,
) -> bool {
    let mut changed = false;
    for (id, manifest, files) in PLUGINS {
        if declined.contains(*id) {
            continue;
        }
        let root = root_for(id);
        let registered = plugins.iter().find(|plugin| plugin.plugin_id == *id);
        if registered.is_some_and(|plugin| !crate::plugin_paths::is_plugin_rooted_at(plugin, &root))
        {
            continue;
        }
        if let Err(err) = materialize(&root, manifest, files) {
            tracing::warn!(plugin_id = id, err = %err, "failed to materialize built-in plugin");
            continue;
        }
        if registered.is_some() {
            continue;
        }
        match crate::app::load_plugin_manifest(&root.display().to_string(), true) {
            Ok(plugin) => {
                plugins.push(plugin);
                changed = true;
            }
            Err((_, err)) => {
                tracing::warn!(plugin_id = id, err = %err, "failed to register built-in plugin");
            }
        }
    }
    changed
}

fn materialize(
    root: &Path,
    manifest: &str,
    files: &[(&'static str, &'static str)],
) -> std::io::Result<()> {
    // bin/ is owned by Herdr; rebuild it so scripts dropped from a
    // bundled plugin do not linger after an upgrade.
    match std::fs::remove_dir_all(root.join("bin")) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => return Err(err),
        _ => {}
    }
    std::fs::create_dir_all(root.join("bin"))?;
    std::fs::write(root.join("herdr-plugin.toml"), manifest)?;
    for (name, contents) in files {
        let path = root.join("bin").join(name);
        std::fs::write(&path, contents)?;
        set_executable(&path)?;
    }
    Ok(())
}

#[cfg(unix)]
fn set_executable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions)
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
fn manifest_for(id: &str) -> &'static str {
    PLUGINS
        .iter()
        .find(|(plugin_id, _, _)| *plugin_id == id)
        .map(|(_, manifest, _)| *manifest)
        .expect("built-in plugin")
}

#[cfg(test)]
fn files_for(id: &str) -> &'static [(&'static str, &'static str)] {
    PLUGINS
        .iter()
        .find(|(plugin_id, _, _)| *plugin_id == id)
        .map(|(_, _, files)| *files)
        .expect("built-in plugin")
}

#[cfg(test)]
fn assert_plugin_assets(id: &str, action_ids: &[&str]) {
    let manifest: toml::Value = manifest_for(id).parse().expect("embedded manifest parses");
    let ids = manifest["actions"]
        .as_array()
        .expect("entry array")
        .iter()
        .map(|entry| entry["id"].as_str().expect("entry id"))
        .collect::<Vec<_>>();
    assert_eq!(ids, action_ids);
    for (name, contents) in files_for(id) {
        // `open` is the action script; the rest run inside the panes.
        if *name == "open" {
            assert!(contents.contains("plugin pane open"));
        }
        // Bundled scripts must run with only a POSIX shell.
        assert!(!contents.contains("python"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_manifest_contains_builtin_entrypoints() {
        assert_plugin_assets("worktrunk", &["switch", "list", "remove", "merge"]);
    }

    #[cfg(unix)]
    #[test]
    fn worktrunk_pane_reports_a_missing_wt_instead_of_failing_silently() {
        use std::io::Write;

        // An empty PATH hides wt and every installer, so the script can only
        // explain and wait for Enter.
        let mut child = std::process::Command::new("/bin/sh")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/plugins/worktrunk/bin/run"
            ))
            .arg("list")
            .env("PATH", "")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(b"\n").unwrap();
        let output = child.wait_with_output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("wt) is not installed"), "{stdout}");
        assert_eq!(output.status.code(), Some(127), "{stdout}");
    }

    #[test]
    fn default_group_members_reference_bundled_actions() {
        for (_, _, action, _) in default_group_members() {
            let (plugin, action) = action.split_once('.').expect("plugin.action id");
            let manifest: toml::Value = manifest_for(plugin).parse().expect("manifest");
            assert!(
                manifest["actions"]
                    .as_array()
                    .expect("actions")
                    .iter()
                    .any(|entry| entry["id"].as_str() == Some(action)),
                "{plugin}.{action}"
            );
        }
    }

    fn temp_root(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "herdr-bundled-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn register_bundled_adds_missing_plugins_and_writes_their_files() {
        let base = temp_root("register");
        let mut plugins = Vec::new();

        let changed = register_bundled_with(&mut plugins, &BTreeSet::new(), |id| base.join(id));

        assert!(changed);
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].plugin_id, "worktrunk");
        assert!(plugins[0].enabled);
        assert!(base.join("worktrunk/bin/open").is_file());
        assert!(!register_bundled_with(
            &mut plugins,
            &BTreeSet::new(),
            |id| base.join(id)
        ));
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn register_bundled_skips_declined_plugins_without_writing_files() {
        let base = temp_root("declined");
        let mut plugins = Vec::new();
        let declined = BTreeSet::from(["worktrunk".to_string()]);

        let changed = register_bundled_with(&mut plugins, &declined, |id| base.join(id));

        assert!(!changed);
        assert!(plugins.is_empty());
        assert!(!base.exists());
    }

    #[test]
    fn register_bundled_leaves_a_plugin_with_the_same_id_from_elsewhere_alone() {
        let base = temp_root("shadowed");
        let elsewhere = temp_root("user-worktrunk");
        std::fs::create_dir_all(elsewhere.join("bin")).unwrap();
        std::fs::write(
            elsewhere.join("herdr-plugin.toml"),
            manifest_for("worktrunk"),
        )
        .unwrap();
        let user =
            crate::app::load_plugin_manifest(&elsewhere.display().to_string(), false).unwrap();
        let mut plugins = vec![user.clone()];

        let changed = register_bundled_with(&mut plugins, &BTreeSet::new(), |id| base.join(id));

        assert!(!changed);
        assert_eq!(plugins, vec![user]);
        assert!(!base.exists());
        let _ = std::fs::remove_dir_all(elsewhere);
    }

    #[test]
    fn worktrunk_panes_use_large_popups() {
        let manifest: toml::Value = manifest_for("worktrunk").parse().expect("manifest");
        for pane in manifest["panes"].as_array().expect("panes") {
            assert_eq!(pane["width"].as_str(), Some("90%"));
            assert_eq!(pane["height"].as_str(), Some("90%"));
        }
    }

    #[test]
    fn worktrunk_switch_pane_opens_and_focuses_space() {
        // Every pane runs the shared `run` script, which holds the wt calls.
        let command = files_for("worktrunk")
            .iter()
            .find(|(name, _)| *name == "run")
            .expect("run script")
            .1;
        assert!(command.contains("wt switch"), "{command}");
        assert!(command.contains("worktree open"), "{command}");
        assert!(command.contains("--focus"), "{command}");
    }
}
