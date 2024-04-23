use rustc_hash::FxHashMap;
use tracing::debug;

use distribution_types::{DecomposedUrl, UvSource, Verbatim};
use pep508_rs::{MarkerEnvironment, VerbatimUrl};
use uv_distribution::is_same_reference;
use uv_normalize::PackageName;

use crate::{Manifest, ResolveError};

/// A map of package names to their associated, required URLs.
#[derive(Debug, Default)]
pub(crate) struct Urls(FxHashMap<PackageName, DecomposedUrl>);

impl Urls {
    pub(crate) fn from_manifest(
        manifest: &Manifest,
        markers: &MarkerEnvironment,
    ) -> Result<Self, ResolveError> {
        let mut urls: FxHashMap<PackageName, DecomposedUrl> = FxHashMap::default();

        // Add the urls themselves to the list of required URLs.
        for (editable, metadata, _) in &manifest.editables {
            if let Some(previous) = urls.insert(metadata.name.clone(), editable.url.clone()) {
                if !is_equal(&previous, &editable.url) {
                    if is_same_reference(&previous, &editable.url) {
                        debug!("Allowing {} as a variant of {previous}", editable.url);
                    } else {
                        return Err(ResolveError::ConflictingUrlsDirect(
                            metadata.name.clone(),
                            previous.verbatim().to_string(),
                            editable.verbatim().to_string(),
                        ));
                    }
                }
            }
        }

        // Add all direct requirements and constraints. If there are any conflicts, return an error.
        for requirement in manifest.requirements(markers) {
            match &requirement.source {
                UvSource::Registry { .. } => {}
                UvSource::Url { url, .. } => {
                    if let Some(previous) = urls.insert(requirement.name.clone(), url.clone()) {
                        if !is_equal(&previous, url) {
                            return Err(ResolveError::ConflictingUrlsDirect(
                                requirement.name.clone(),
                                previous.verbatim().to_string(),
                                url.verbatim().to_string(),
                            ));
                        }
                    }
                }
                UvSource::Git { url, .. } => {
                    if let Some(previous) = urls.insert(requirement.name.clone(), url.clone()) {
                        if !is_equal(&previous, url) {
                            if is_same_reference(&previous, url) {
                                debug!("Allowing {url} as a variant of {previous}");
                            } else {
                                return Err(ResolveError::ConflictingUrlsDirect(
                                    requirement.name.clone(),
                                    previous.verbatim().to_string(),
                                    url.verbatim().to_string(),
                                ));
                            }
                        }
                    }
                }
                UvSource::Path { url, .. } => {
                    if let Some(previous) = urls.insert(requirement.name.clone(), url.clone()) {
                        if !is_equal(&previous, url) {
                            return Err(ResolveError::ConflictingUrlsDirect(
                                requirement.name.clone(),
                                previous.verbatim().to_string(),
                                url.verbatim().to_string(),
                            ));
                        }
                    }
                }
            }
        }

        Ok(Self(urls))
    }

    /// Return the [`DecomposedUrl`] associated with the given package name, if any.
    pub(crate) fn get(&self, package: &PackageName) -> Option<&DecomposedUrl> {
        self.0.get(package)
    }

    /// Returns `true` if the provided URL is compatible with the given "allowed" URL.
    pub(crate) fn is_allowed(expected: &DecomposedUrl, provided: &DecomposedUrl) -> bool {
        #[allow(clippy::if_same_then_else)]
        if is_equal(&expected.to_verbatim_url(), &provided.to_verbatim_url()) {
            // If the URLs are canonically equivalent, they're compatible.
            true
        } else if is_same_reference(&expected.to_verbatim_url(), &provided.to_verbatim_url()) {
            // If the URLs refer to the same commit, they're compatible.
            true
        } else {
            // Otherwise, they're incompatible.
            false
        }
    }
}

/// Returns `true` if the [`DecomposedUrl`] is compatible with the previous [`DecomposedUrl`].
///
/// Accepts URLs that map to the same [`CanonicalUrl`].
fn is_equal(previous: &VerbatimUrl, url: &VerbatimUrl) -> bool {
    cache_key::CanonicalUrl::new(previous.raw()) == cache_key::CanonicalUrl::new(url.raw())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_compatibility() -> Result<(), url::ParseError> {
        // Same repository, same tag.
        let previous = VerbatimUrl::parse_url("git+https://example.com/MyProject.git@v1.0")?;
        let url = VerbatimUrl::parse_url("git+https://example.com/MyProject.git@v1.0")?;
        assert!(is_equal(&previous, &url));

        // Same repository, different tags.
        let previous = VerbatimUrl::parse_url("git+https://example.com/MyProject.git@v1.0")?;
        let url = VerbatimUrl::parse_url("git+https://example.com/MyProject.git@v1.1")?;
        assert!(!is_equal(&previous, &url));

        // Same repository (with and without `.git`), same tag.
        let previous = VerbatimUrl::parse_url("git+https://example.com/MyProject@v1.0")?;
        let url = VerbatimUrl::parse_url("git+https://example.com/MyProject.git@v1.0")?;
        assert!(is_equal(&previous, &url));

        // Same repository, no tag on the previous URL.
        let previous = VerbatimUrl::parse_url("git+https://example.com/MyProject.git")?;
        let url = VerbatimUrl::parse_url("git+https://example.com/MyProject.git@v1.0")?;
        assert!(!is_equal(&previous, &url));

        // Same repository, tag on the previous URL, no tag on the overriding URL.
        let previous = VerbatimUrl::parse_url("git+https://example.com/MyProject.git@v1.0")?;
        let url = VerbatimUrl::parse_url("git+https://example.com/MyProject.git")?;
        assert!(!is_equal(&previous, &url));

        Ok(())
    }
}
