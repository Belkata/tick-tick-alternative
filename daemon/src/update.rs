//! Self-update from GitHub releases: the platform-neutral half (plan/15, macOS auto-update).
//!
//! Parsing the releases list, choosing the newest release that has a macOS zip and its
//! signature, comparing versions, reading an `Info.plist` and verifying the detached OpenPGP
//! signature against the repository key. `update_mac.rs` does the downloading and the
//! bundle swap, macOS only.

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]
use anyhow::{Context, Result, bail};
use pgp::composed::{Deserializable, DetachedSignature, SignedPublicKey};
use pgp::types::KeyDetails;
use semver::Version;
use serde::Deserialize;

/// The releases list (newest first), no authentication.
pub const RELEASES_URL: &str = "https://api.github.com/repos/Belkata/utre/releases?per_page=30";
/// Downloads must come from this repository's releases, whatever the JSON says.
const DOWNLOAD_PREFIX: &str = "https://github.com/Belkata/utre/releases/download/";
/// The repository's signing key (also the pacman repo's).
const KEY_ASC: &str = include_str!("../../packaging/arch/utre-repo.asc");
/// Its fingerprint: the embedded key must have exactly this.
pub const KEY_FINGERPRINT: &str = "8d9a9e543bb6d88237d174a78be15c9843add341";
/// The bundle id of the outer app.
pub const APP_BUNDLE_ID: &str = "dev.utre.utre";

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

/// A release that can be installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    pub zip_url: String,
    pub sig_url: String,
}

/// `vX.Y.Z` (exactly three numbers) as a version; anything else (`arch-repo`, `v1.2`,
/// `v1.2.3-rc1`) is not a release of the app.
fn tag_version(tag: &str) -> Option<Version> {
    let v = Version::parse(tag.strip_prefix('v')?).ok()?;
    (v.pre.is_empty() && v.build.is_empty()).then_some(v)
}

/// The highest `vX.Y.Z` release (pre-releases included, drafts not) that has both
/// `utre-X.Y.Z-macos.zip` and its `.sig`.
pub fn pick_latest(json: &str) -> Result<Option<Release>> {
    let releases: Vec<GhRelease> = serde_json::from_str(json).context("parsing the releases")?;
    let mut best: Option<Release> = None;
    for r in releases.into_iter().filter(|r| !r.draft) {
        let Some(version) = tag_version(&r.tag_name) else {
            continue;
        };
        let zip_name = format!("utre-{version}-macos.zip");
        let sig_name = format!("{zip_name}.sig");
        let url = |name: &str| {
            r.assets
                .iter()
                .find(|a| a.name == name && a.browser_download_url.starts_with(DOWNLOAD_PREFIX))
                .map(|a| a.browser_download_url.clone())
        };
        let (Some(zip_url), Some(sig_url)) = (url(&zip_name), url(&sig_name)) else {
            continue;
        };
        if best.as_ref().is_none_or(|b| version > b.version) {
            best = Some(Release {
                version,
                zip_url,
                sig_url,
            });
        }
    }
    Ok(best)
}

/// The release to install, if it is newer than what is installed.
pub fn newer_than(release: Option<Release>, installed: &Version) -> Option<Release> {
    release.filter(|r| &r.version > installed)
}

/// `(CFBundleIdentifier, CFBundleShortVersionString)` of an `Info.plist` (XML or binary).
pub fn parse_info_plist(bytes: &[u8]) -> Result<(String, Version)> {
    let value = plist::Value::from_reader(std::io::Cursor::new(bytes)).context("Info.plist")?;
    let dict = value.as_dictionary().context("Info.plist is not a dictionary")?;
    let get = |key: &str| {
        dict.get(key)
            .and_then(|v| v.as_string())
            .with_context(|| format!("Info.plist has no {key}"))
    };
    let version = Version::parse(get("CFBundleShortVersionString")?)
        .context("CFBundleShortVersionString is not a version")?;
    Ok((get("CFBundleIdentifier")?.to_string(), version))
}

/// Check `sig` (binary or armored, detached) over `data` against the repository key.
pub fn verify_signature(data: &[u8], sig: &[u8]) -> Result<()> {
    let (key, _) = SignedPublicKey::from_string(KEY_ASC).context("the embedded key")?;
    let fingerprint: String = key
        .fingerprint()
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if fingerprint != KEY_FINGERPRINT {
        bail!("the embedded key is not the repository key ({fingerprint})");
    }
    let signature = DetachedSignature::from_bytes(sig)
        .or_else(|_| DetachedSignature::from_armor_single(sig).map(|(s, _)| s))
        .context("reading the signature")?;
    signature
        .verify(&key, data)
        .context("the signature does not match")
}

#[cfg(test)]
mod tests {
    use super::*;

    const RELEASES: &str = include_str!("../tests/fixtures/releases.json");
    const PAYLOAD: &[u8] = include_bytes!("../tests/fixtures/payload.bin");
    const SIG: &[u8] = include_bytes!("../tests/fixtures/payload.bin.sig");
    const OTHER_SIG: &[u8] = include_bytes!("../tests/fixtures/payload.bin.othersig");

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn picks_the_highest_release_with_both_assets() {
        // v0.9.0 has no .sig, v0.10.0 is a draft, v0.8.12 is highest complete,
        // and `arch-repo`/`v0.8` are ignored.
        let r = pick_latest(RELEASES).unwrap().unwrap();
        assert_eq!(r.version, v("0.8.12"));
        assert!(r.zip_url.ends_with("/v0.8.12/utre-0.8.12-macos.zip"));
        assert!(r.sig_url.ends_with("/v0.8.12/utre-0.8.12-macos.zip.sig"));
    }

    #[test]
    fn compares_numerically_not_as_text() {
        assert!(newer_than(pick_latest(RELEASES).unwrap(), &v("0.8.9")).is_some());
        assert!(newer_than(pick_latest(RELEASES).unwrap(), &v("0.8.12")).is_none());
        assert!(newer_than(pick_latest(RELEASES).unwrap(), &v("0.9.0")).is_none());
        assert!(newer_than(None, &v("0.1.0")).is_none());
    }

    #[test]
    fn rejects_foreign_download_hosts() {
        let json = r#"[{"tag_name":"v1.0.0","assets":[
            {"name":"utre-1.0.0-macos.zip","browser_download_url":"https://evil.example/a.zip"},
            {"name":"utre-1.0.0-macos.zip.sig","browser_download_url":"https://evil.example/a.sig"}]}]"#;
        assert!(pick_latest(json).unwrap().is_none());
    }

    #[test]
    fn reads_info_plist() {
        let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>dev.utre.utre</string>
<key>CFBundleShortVersionString</key><string>0.8.11</string>
</dict></plist>"#;
        assert_eq!(
            parse_info_plist(xml).unwrap(),
            (APP_BUNDLE_ID.to_string(), v("0.8.11"))
        );
        assert!(parse_info_plist(b"nope").is_err());
    }

    #[test]
    fn accepts_a_signature_by_the_repository_key() {
        verify_signature(PAYLOAD, SIG).unwrap();
    }

    #[test]
    fn rejects_a_tampered_file() {
        let mut bad = PAYLOAD.to_vec();
        bad[0] ^= 1;
        assert!(verify_signature(&bad, SIG).is_err());
    }

    #[test]
    fn rejects_a_signature_by_another_key() {
        assert!(verify_signature(PAYLOAD, OTHER_SIG).is_err());
    }

    #[test]
    fn rejects_garbage_signatures() {
        assert!(verify_signature(PAYLOAD, b"not a signature").is_err());
    }
}
