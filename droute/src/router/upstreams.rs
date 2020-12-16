// Copyright 2020 LEXUGE
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

//! Upstream defines how droute resolves queries ultimately.

/// Module which contains builtin client implementations and the trait for implement your own.
pub mod client_pool;
/// Module which contains the error type for the `upstreams` section.
pub mod error;
#[cfg(feature = "serde-cfg")]
pub mod parsed;
mod resp_cache;
mod upstream;

pub use upstream::*;

use self::error::{Result, UpstreamError};
#[cfg(feature = "serde-cfg")]
use self::parsed::ParsedUpstream;
use crate::Label;
use futures::future::{select_ok, BoxFuture, FutureExt};
use hashbrown::{HashMap, HashSet};
use trust_dns_client::op::Message;

/// `Upstream` aggregated, used to create `Router`.
pub struct Upstreams {
    upstreams: HashMap<Label, Upstream>,
}

impl Upstreams {
    /// Create a new `Upstreams` by passing a bunch of `Upstream`s, with their respective labels, and cache capacity.
    pub fn new(upstreams: Vec<(Label, Upstream)>) -> Result<Self> {
        let mut r = HashMap::new();
        for u in upstreams {
            // Check if there is multiple definitions being passed in.
            match r.get(&u.0) {
                Some(_) => return Err(UpstreamError::MultipleDef(u.0)),
                None => {
                    r.insert(u.0, u.1);
                }
            };
        }
        let u = Self { upstreams: r };
        u.check()?;
        Ok(u)
    }

    /// Create a new `Upstreams` with a set of ParsedUpstream.
    #[cfg(feature = "serde-cfg")]
    pub async fn with_parsed(upstreams: Vec<ParsedUpstream>, size: usize) -> Result<Self> {
        Self::new({
            let mut v = Vec::new();
            for u in upstreams {
                v.push((u.tag.clone(), Upstream::with_parsed(u, size).await?));
            }
            v
        })
    }

    // Check any upstream types
    // tag: current upstream node's tag
    // l: visited tags
    fn traverse(&self, l: &mut HashSet<Label>, tag: &Label) -> Result<()> {
        if l.contains(tag) {
            return Err(UpstreamError::HybridRecursion(tag.clone()));
        } else {
            l.insert(tag.clone());

            if let Some(v) = &self
                .upstreams
                .get(tag)
                .ok_or_else(|| UpstreamError::MissingTag(tag.clone()))?
                .try_hybrid()
            {
                // Check if it is empty.
                if v.is_empty() {
                    return Err(UpstreamError::EmptyHybrid(tag.clone()));
                }

                // Check if it is recursively defined.
                for t in v {
                    self.traverse(l, t)?
                }
            }
        }

        Ok(())
    }

    /// Check if the upstream is legitimate. This is automatically done when you create a new `Upstreams`.
    pub fn check(&self) -> Result<bool> {
        for (tag, _) in self.upstreams.iter() {
            self.traverse(&mut HashSet::new(), tag)?
        }
        Ok(true)
    }

    // Make it only visible in side `router`
    pub(super) fn exists(&self, tag: &Label) -> Result<bool> {
        if self.upstreams.contains_key(tag) {
            Ok(true)
        } else {
            Err(UpstreamError::MissingTag(tag.clone()))
        }
    }

    // Write out in this way to allow recursion for async functions
    // Should no be accessible from external crates
    pub(super) fn resolve<'a>(
        &'a self,
        tag: &'a Label,
        msg: &'a Message,
    ) -> BoxFuture<'a, Result<Message>> {
        async move {
            let u = self.upstreams.get(tag).unwrap();
            Ok(if let Some(v) = u.try_hybrid() {
                let v = v.iter().map(|t| self.resolve(t, msg));
                let (r, _) = select_ok(v.clone()).await?;
                r
            } else {
                u.resolve(msg).await?
            })
        }
        .boxed()
    }
}
