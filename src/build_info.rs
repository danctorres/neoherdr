//! Build identity helpers.

pub const BASE_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn channel() -> &'static str {
    non_empty(option_env!("HERDR_BUILD_CHANNEL")).unwrap_or("stable")
}

pub fn build_id() -> Option<&'static str> {
    non_empty(option_env!("HERDR_BUILD_ID"))
}

pub fn version() -> String {
    match channel() {
        "stable" => BASE_VERSION.to_string(),
        channel => match build_id() {
            Some(build_id) => format!("{BASE_VERSION}-{channel}.{build_id}"),
            None => format!("{BASE_VERSION}-{channel}"),
        },
    }
}

pub fn is_preview() -> bool {
    channel() == "preview"
}

/// Where released builds of this binary are published.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateSource {
    /// Upstream herdr's hosted stable/preview manifests at herdr.dev.
    // neoherdr never constructs this outside tests; the variant keeps the
    // hosted-release code paths compiled so upstream changes rebase cleanly.
    #[allow(dead_code)]
    HerdrDev,
}

/// How users of a build without an update source upgrade it.
pub const FROM_SOURCE_UPDATE_HINT: &str =
    "neoherdr is built from source; update with: cargo install --locked --git https://github.com/danctorres/neoherdr";

/// The release channel this build updates from, if any.
///
/// neoherdr is built from source and publishes no releases, so it has no
/// update source: self-update, background version checks, release notes, and
/// release-asset downloads for remotes are all disabled. Agent-detection
/// manifest checks are independent of this.
pub fn update_source() -> Option<UpdateSource> {
    #[cfg(test)]
    if let Some(source) = test_update_source::get() {
        return source;
    }
    None
}

#[cfg(test)]
pub(crate) mod test_update_source {
    use std::cell::Cell;

    use super::UpdateSource;

    thread_local! {
        static OVERRIDE: Cell<Option<Option<UpdateSource>>> = const { Cell::new(None) };
    }

    pub(super) fn get() -> Option<Option<UpdateSource>> {
        OVERRIDE.with(Cell::get)
    }

    /// Overrides `update_source()` on the current thread until dropped, so
    /// tests can exercise the hosted-release paths this build compiles out.
    pub(crate) struct Guard(Option<Option<UpdateSource>>);

    pub(crate) fn set(source: Option<UpdateSource>) -> Guard {
        Guard(OVERRIDE.with(|cell| cell.replace(Some(source))))
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            OVERRIDE.with(|cell| cell.set(self.0));
        }
    }
}

fn non_empty(value: Option<&'static str>) -> Option<&'static str> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn stable_version_defaults_to_cargo_version() {
        assert!(!super::version().is_empty());
    }

    #[test]
    fn fork_builds_have_no_update_source() {
        assert_eq!(super::update_source(), None);
    }

    #[test]
    fn test_override_restores_previous_update_source() {
        {
            let _guard = super::test_update_source::set(Some(super::UpdateSource::HerdrDev));
            assert_eq!(super::update_source(), Some(super::UpdateSource::HerdrDev));
        }
        assert_eq!(super::update_source(), None);
    }
}
