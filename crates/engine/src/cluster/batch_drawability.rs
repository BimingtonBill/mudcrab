//! Render-to-main demand observations; never permission to destroy fallback draws.
//!
//! This is demand, not a drawability lease. The render-side complementary filter
//! remains authoritative for every camera, required prepass and shadow consumer.

use super::reader::CertifiedPackage;
use bevy::prelude::*;
use std::{collections::BTreeSet, sync::Arc};

/// Changed-only observations from the renderer, scoped to one package and lazy layout.
/// An absent batch, absent producer or mismatched scope always requires stock fallback.
#[derive(Resource, Default)]
pub(crate) struct BatchDrawability {
    package: Option<Arc<CertifiedPackage>>,
    layout_scope: u64,
    everywhere: BTreeSet<u32>,
    changed: BTreeSet<u32>,
    reset: bool,
}

#[allow(
    dead_code,
    reason = "#2136 supplies the producer; resident fallback never retires from observations"
)]
impl BatchDrawability {
    /// Publish changed aggregate facts through the extraction bridge. `true` requires
    /// at least one applicable consumer and readiness in ALL applicable consumers.
    /// Publish `false` when the last consumer disappears. Calling with an empty
    /// update iterator still withdraws all old facts when package/layout changes.
    pub(crate) fn publish(
        &mut self,
        package: &Arc<CertifiedPackage>,
        layout_scope: u64,
        updates: impl IntoIterator<Item = (u32, bool)>,
    ) {
        if !self.matches(package, layout_scope) {
            self.package = Some(package.clone());
            self.layout_scope = layout_scope;
            self.everywhere.clear();
            self.changed.clear();
            self.reset = true;
        }
        for (batch, drawable) in updates {
            let changed = if drawable {
                self.everywhere.insert(batch)
            } else {
                self.everywhere.remove(&batch)
            };
            if changed {
                self.changed.insert(batch);
            }
        }
    }

    pub(crate) fn matches(&self, package: &Arc<CertifiedPackage>, layout_scope: u64) -> bool {
        self.layout_scope == layout_scope
            && self
                .package
                .as_ref()
                .is_some_and(|old| Arc::ptr_eq(old, package))
    }

    pub(crate) fn drawable(
        &self,
        package: &Arc<CertifiedPackage>,
        layout_scope: u64,
        batch: u32,
    ) -> bool {
        self.matches(package, layout_scope) && self.everywhere.contains(&batch)
    }

    pub(crate) fn take_changes(&mut self) -> (bool, BTreeSet<u32>) {
        (
            std::mem::take(&mut self.reset),
            std::mem::take(&mut self.changed),
        )
    }
}
