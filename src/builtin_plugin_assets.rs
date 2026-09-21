use std::path::{Path, PathBuf};

/// `(plugin id, manifest, [(bin file name, contents)])`.
type BundledPlugin = (
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
);

const PLUGINS: &[BundledPlugin] = &[(
    "worktrunk",
    include_str!("../plugins/worktrunk/herdr-plugin.toml"),
    &[("open", include_str!("../plugins/worktrunk/bin/open"))],
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

pub(crate) fn materialize() -> std::io::Result<Vec<PathBuf>> {
    PLUGINS
        .iter()
        .map(|(id, manifest, files)| {
            let root = crate::plugin_paths::builtin_plugin_dir(id);
            // bin/ is owned by Herdr; rebuild it so scripts dropped from a
            // bundled plugin do not linger after an upgrade.
            match std::fs::remove_dir_all(root.join("bin")) {
                Err(err) if err.kind() != std::io::ErrorKind::NotFound => return Err(err),
                _ => {}
            }
            std::fs::create_dir_all(root.join("bin"))?;
            std::fs::write(root.join("herdr-plugin.toml"), manifest)?;
            for (name, contents) in *files {
                let path = root.join("bin").join(name);
                std::fs::write(&path, contents)?;
                set_executable(&path)?;
            }
            Ok(root)
        })
        .collect()
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
    for (_, contents) in files_for(id) {
        assert!(contents.contains("plugin pane open"));
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
        let manifest: toml::Value = manifest_for("worktrunk").parse().expect("manifest");
        let switch = manifest["panes"]
            .as_array()
            .expect("panes")
            .iter()
            .find(|pane| pane["id"].as_str() == Some("switch"))
            .expect("switch pane");
        let command = switch["command"]
            .as_array()
            .expect("command")
            .iter()
            .filter_map(|part| part.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(command.contains("wt switch"), "{command}");
        assert!(command.contains("worktree open"), "{command}");
        assert!(command.contains("--focus"), "{command}");
    }
}
