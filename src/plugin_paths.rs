use std::path::{Path, PathBuf};

const PLUGIN_CONFIG_PATH_COMPONENT_MAX_CHARS: usize = 120;

pub(crate) fn managed_plugins_dir() -> PathBuf {
    crate::config::config_dir().join("plugins")
}

pub(crate) fn builtin_plugin_dir(plugin_id: &str) -> PathBuf {
    managed_plugins_dir().join("builtin").join(plugin_id)
}

/// Built-in plugins are registered as local plugins rooted at their
/// materialized directory, so the root identifies them.
pub(crate) fn is_builtin_plugin(plugin: &crate::api::schema::InstalledPluginInfo) -> bool {
    is_plugin_rooted_at(plugin, &builtin_plugin_dir(&plugin.plugin_id))
}

pub(crate) fn is_plugin_rooted_at(
    plugin: &crate::api::schema::InstalledPluginInfo,
    dir: &Path,
) -> bool {
    let root = Path::new(&plugin.plugin_root);
    root == dir || canonicalize_existing_prefix(dir).is_some_and(|dir| root == dir)
}

/// Canonicalizes the deepest existing ancestor of `path` and re-appends the
/// missing tail, so a deleted directory under a symlinked parent still
/// resolves to the path its canonical form had.
fn canonicalize_existing_prefix(path: &Path) -> Option<PathBuf> {
    let mut missing = Vec::new();
    let mut current = path;
    loop {
        if let Ok(canonical) = current.canonicalize() {
            let mut resolved = crate::platform::plugin_runtime_path(&canonical);
            resolved.extend(missing.iter().rev());
            return Some(resolved);
        }
        missing.push(current.file_name()?);
        current = current.parent()?;
    }
}

pub(crate) fn managed_checkout_path(plugin_id: &str) -> PathBuf {
    managed_plugins_dir()
        .join("github")
        .join(crate::api::schema::plugin_managed_path_component(plugin_id))
}

pub(crate) fn managed_checkout_lock_path(plugin_id: &str) -> PathBuf {
    managed_plugins_dir().join(".locks").join(format!(
        ".{}.lock",
        crate::api::schema::plugin_managed_path_component(plugin_id)
    ))
}

pub(crate) fn managed_installations_dir(plugin_id: &str) -> PathBuf {
    managed_plugins_dir()
        .join("github-installations")
        .join(crate::api::schema::plugin_managed_path_component(plugin_id))
}

pub(crate) fn create_managed_installation(plugin_id: &str) -> std::io::Result<PathBuf> {
    let parent = managed_installations_dir(plugin_id);
    std::fs::create_dir_all(&parent)?;
    for generation in 0u64.. {
        let path = parent.join(format!("{}-{generation}", std::process::id()));
        match std::fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }
    Err(std::io::Error::other(
        "plugin installation directory exhausted",
    ))
}

pub(crate) fn plugin_config_dir(plugin_id: &str) -> PathBuf {
    managed_plugins_dir()
        .join("config")
        .join(plugin_config_path_component(plugin_id))
}

pub(crate) fn plugin_state_dir(plugin_id: &str) -> PathBuf {
    crate::config::state_dir()
        .join("plugins")
        .join(plugin_config_path_component(plugin_id))
}

pub(crate) fn ensure_plugin_user_dirs(plugin_id: &str) -> std::io::Result<()> {
    ensure_plugin_config_dir(plugin_id)?;
    std::fs::create_dir_all(plugin_state_dir(plugin_id))?;
    Ok(())
}

fn ensure_plugin_config_dir(plugin_id: &str) -> std::io::Result<()> {
    let config_dir = plugin_config_dir(plugin_id);
    if config_dir.exists() {
        return std::fs::create_dir_all(config_dir);
    }
    if let Some(legacy_dir) = legacy_plugin_config_dirs(plugin_id)
        .into_iter()
        .find(|path| path.is_dir())
    {
        copy_dir_all(&legacy_dir, &config_dir)?;
        return Ok(());
    }
    std::fs::create_dir_all(config_dir)
}

fn legacy_plugin_config_dirs(plugin_id: &str) -> Vec<PathBuf> {
    let plugins_dir = managed_plugins_dir();
    let old_unhashed = (!matches!(
        plugin_id,
        "config" | "github" | "github-installations" | ".locks" | "builtin"
    ))
    .then(|| plugins_dir.join(plugin_id));
    let current_hashed =
        plugins_dir.join(crate::api::schema::plugin_managed_path_component(plugin_id));
    let mut candidates = Vec::new();
    if let Some(old_unhashed) = old_unhashed {
        if old_unhashed != current_hashed {
            candidates.push(old_unhashed);
        }
    }
    candidates.push(current_hashed);
    candidates
}

fn plugin_config_path_component(value: &str) -> String {
    let mut component = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        {
            component.push(byte as char);
        } else {
            use std::fmt::Write as _;
            let _ = write!(component, "%{byte:02X}");
        }
    }
    if component.ends_with('.') {
        component.pop();
        component.push_str("%2E");
    }
    if component.is_empty() {
        return "%plugin".to_string();
    }
    if crate::api::schema::has_windows_reserved_stem_for_path_component(&component) {
        component = format!("%{component}");
    }
    if component.chars().count() > PLUGIN_CONFIG_PATH_COMPONENT_MAX_CHARS {
        let hash = crate::api::schema::short_plugin_id_hash_for_path_component(value);
        let prefix_len = PLUGIN_CONFIG_PATH_COMPONENT_MAX_CHARS - hash.len() - 1;
        let prefix = component.chars().take(prefix_len).collect::<String>();
        return format!("{prefix}-{hash}");
    }
    component
}

fn copy_dir_all(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(destination)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let destination_path = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &destination_path)?;
        } else {
            std::fs::copy(entry.path(), destination_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_config_dirs_never_point_at_the_bundled_plugin_tree() {
        let _guard = crate::config::test_config_env_lock().lock().unwrap();
        let bundled_tree = managed_plugins_dir().join("builtin");
        assert!(!legacy_plugin_config_dirs("builtin").contains(&bundled_tree));
    }

    #[cfg(unix)]
    #[test]
    fn deleted_plugin_root_under_symlinked_dir_still_matches() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let base = std::env::temp_dir().join(format!(
            "herdr-rooted-symlink-{}-{nanos}",
            std::process::id()
        ));
        let real = base.join("real");
        let link = base.join("link");
        let real_root = real.join("builtin").join("demo");
        std::fs::create_dir_all(&real_root).unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        std::fs::write(
            real_root.join("herdr-plugin.toml"),
            "id = \"demo\"\nname = \"Demo\"\nversion = \"0.1.0\"\nmin_herdr_version = \"0.6.10\"\nplatforms = [\"linux\", \"macos\"]\n",
        )
        .unwrap();
        let linked_root = link.join("builtin").join("demo");
        let plugin =
            crate::app::load_plugin_manifest(&linked_root.display().to_string(), true).unwrap();
        std::fs::remove_dir_all(&real_root).unwrap();

        assert!(is_plugin_rooted_at(&plugin, &linked_root));
        assert!(!is_plugin_rooted_at(
            &plugin,
            &link.join("builtin").join("other")
        ));
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn installations_never_reuse_or_move_existing_files() {
        let _guard = crate::config::test_config_env_lock().lock().unwrap();
        let base = std::env::temp_dir().join(format!(
            "herdr-installations-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let previous = std::env::var_os("XDG_CONFIG_HOME");
        std::env::set_var("XDG_CONFIG_HOME", &base);
        let id = format!("example.generations-{}", std::process::id());
        let first = create_managed_installation(&id).unwrap();
        std::fs::write(first.join("keep"), "original").unwrap();
        let second = create_managed_installation(&id).unwrap();
        assert_ne!(first, second);
        assert_eq!(
            std::fs::read_to_string(first.join("keep")).unwrap(),
            "original"
        );
        ensure_plugin_config_dir("github-installations").unwrap();
        assert!(
            std::fs::read_dir(plugin_config_dir("github-installations"))
                .unwrap()
                .next()
                .is_none(),
            "managed installations are not legacy user configuration"
        );
        std::fs::remove_dir_all(&first).unwrap();
        std::fs::remove_dir_all(&second).unwrap();
        std::fs::remove_dir(managed_installations_dir(&id)).unwrap();
        match previous {
            Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn plugin_config_path_component_is_readable_and_collision_free() {
        assert_eq!(
            plugin_config_path_component("examples.agent-telegram-notify"),
            "examples.agent-telegram-notify"
        );
        assert_eq!(plugin_config_path_component("example:a"), "example%3Aa");
        assert_eq!(plugin_config_path_component("Example"), "%45xample");
        assert_ne!(
            plugin_config_path_component("example:a"),
            plugin_config_path_component("example-a")
        );
        assert_ne!(
            plugin_config_path_component("Example"),
            plugin_config_path_component("example")
        );
        assert_ne!(
            plugin_config_path_component(&"A".repeat(120)),
            plugin_config_path_component(&"B".repeat(120))
        );
        assert!(plugin_config_path_component(&"A".repeat(120)).len() <= 120);
        assert_eq!(plugin_config_path_component("con"), "%con");
        assert_eq!(plugin_config_path_component("example."), "example%2E");
    }
}
