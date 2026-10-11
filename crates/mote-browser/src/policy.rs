use anyhow::{bail, Result};
use serde::Deserialize;
use std::collections::HashSet;
use url::Url;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BrowserEffect {
    Read,
    Fill,
    Commit,
    Blocked,
}

#[derive(Clone, Debug)]
pub struct ApprovedOrigins(HashSet<String>);

impl ApprovedOrigins {
    pub fn new<I, S>(origins: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut approved = HashSet::new();
        for value in origins {
            let url = Url::parse(value.as_ref())?;
            if !matches!(url.scheme(), "http" | "https")
                || !url.username().is_empty()
                || url.password().is_some()
                || url.path() != "/"
                || url.query().is_some()
                || url.fragment().is_some()
            {
                bail!("allowlist entries must be bare HTTP origins");
            }
            approved.insert(url.origin().ascii_serialization());
        }
        if approved.is_empty() {
            bail!("origin allowlist cannot be empty");
        }
        Ok(Self(approved))
    }

    pub fn contains(&self, url: &str) -> bool {
        Url::parse(url).is_ok_and(|url| {
            url.username().is_empty()
                && url.password().is_none()
                && self.0.contains(&url.origin().ascii_serialization())
        })
    }
}

#[derive(Clone, Debug)]
pub struct BrowserPolicy {
    origins: ApprovedOrigins,
    read_paths: HashSet<String>,
}

impl BrowserPolicy {
    pub fn new(origins: ApprovedOrigins) -> Self {
        Self {
            origins,
            read_paths: HashSet::new(),
        }
    }

    pub fn with_read_paths<I, S>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.read_paths
            .extend(paths.into_iter().map(|path| path.as_ref().to_string()));
        self
    }

    pub fn request(&self, url: &str, method: &str) -> BrowserEffect {
        if !self.origins.contains(url) {
            return BrowserEffect::Blocked;
        }
        if method != "GET" {
            return BrowserEffect::Commit;
        }
        let Ok(url) = Url::parse(url) else {
            return BrowserEffect::Blocked;
        };
        if url.query().is_some() {
            return BrowserEffect::Commit;
        }
        if self.read_paths.contains(url.path()) {
            BrowserEffect::Read
        } else {
            BrowserEffect::Commit
        }
    }

    pub fn response(&self, url: &str, content_disposition: Option<&str>) -> BrowserEffect {
        if content_disposition.is_some() {
            BrowserEffect::Commit
        } else {
            self.request(url, "GET")
        }
    }

    pub fn verdict(&self, json: &str) -> BrowserEffect {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Verdict {
            effect: BrowserEffect,
        }
        serde_json::from_str::<Verdict>(json)
            .map(|v| v.effect)
            .unwrap_or(BrowserEffect::Commit)
    }
}
