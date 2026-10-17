//! On-demand, local research diagnostics. Never called by the observation or MCP path.

use super::Engine;
use std::collections::BTreeMap;

impl Engine {
    /// Cardinalities of retained engine collections, excluding port implementations.
    /// Counts are not allocated bytes, RSS, peak memory or a bound on external evidence.
    /// The prototype byte estimate includes vector storage and string capacities only.
    /// This local API is deliberately absent from MCP and collects no screen content.
    pub fn retained_state_sizes(&self) -> BTreeMap<&'static str, usize> {
        let published = self.store.published();
        let registry = published
            .map(|p| &p.registry)
            .or(self.purged_identity.as_ref());
        let cache = self.config.icons.as_ref().map(|c| c.cache.prototypes());
        let (journal, evictable) = self.journal.sizes();
        let (windows, window_targets) = published
            .and_then(|p| p.windows.as_ref())
            .map_or((0, 0), super::freshness::WindowSnapshot::sizes);
        BTreeMap::from([
            ("base_frames", usize::from(self.store.base().is_some())),
            (
                "base_pixel_bytes",
                self.store.base().map_or(0, |b| b.pixels().len()),
            ),
            ("snapshots", usize::from(published.is_some())),
            ("pending_tiles", self.store.pending_dirty().len()),
            ("live_tracks", registry.map_or(0, |r| r.tracks().count())),
            (
                "retired_ids_logical",
                registry.map_or(0, ommatix_domain::identity::Registry::retired_count),
            ),
            (
                "registry_entries",
                registry.map_or(0, ommatix_domain::identity::Registry::stored_identity_count),
            ),
            ("journal_entries", journal),
            ("journal_evictable", evictable),
            (
                "journal_payload_storage_bytes",
                self.journal.retained_payload_bytes(),
            ),
            (
                "controls",
                published.map_or(0, |p| p.evidence.controls.len()),
            ),
            ("words", published.map_or(0, |p| p.evidence.words.len())),
            (
                "accessible",
                published.map_or(0, |p| p.evidence.accessible.len()),
            ),
            ("relations", published.map_or(0, |p| p.relations.len())),
            ("icon_labels", published.map_or(0, |p| p.icons.len())),
            ("icon_prototypes", cache.map_or(0, <[_]>::len)),
            (
                "icon_prototype_storage_bytes",
                self.config
                    .icons
                    .as_ref()
                    .map_or(0, |c| c.cache.retained_storage_bytes()),
            ),
            ("elements", published.map_or(0, |p| p.elements.len())),
            (
                "accessibility_gaps",
                published.map_or(0, |p| p.accessibility_gaps.len()),
            ),
            (
                "changes_created",
                published.map_or(0, |p| p.changes.created.len()),
            ),
            (
                "changes_updated",
                published.map_or(0, |p| p.changes.updated.len()),
            ),
            (
                "changes_retired",
                published.map_or(0, |p| p.changes.retired.len()),
            ),
            (
                "changes_reasons",
                published.map_or(0, |p| p.changes.reasons.len()),
            ),
            ("freshness_windows", windows),
            ("freshness_targets", window_targets),
            (
                "inference_shapes",
                self.measured
                    .as_ref()
                    .and_then(|m| m.plan.input_shapes.as_ref())
                    .map_or(0, Vec::len),
            ),
        ])
    }
}
