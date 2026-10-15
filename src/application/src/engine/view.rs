//! The agent's view of the published snapshot (thesis chapter 07 §7.3).
//!
//! Elements are sorted by target ID. A query filters the view by a normalised substring
//! of the label (and of the value, when values are shown); it never touches the registry.
//! Values are shown only when the request asks for them **and** the local policy allows
//! them. A page holds at most `limit` elements; when more remain, the page carries an
//! opaque cursor bound to the snapshot, the offset, the query and whether values are
//! shown (the request's `include_values` and the policy's together). A cursor is only
//! honoured while its snapshot is the published one and the view it paged still holds.

use ommatix_domain::fusion::normalized;

use super::Engine;
use super::journal::fnv1a64;
use super::observe::{Measured, Published, StoredElement};
use crate::contract::{
    ElementView, ErrorCode, ObservePage, ObserveRequest, ToolError, Zone, limits, truncate_chars,
};

/// Rejects arguments that schema validation should already have caught.
pub(super) fn check_request(request: &ObserveRequest) -> Result<(), ToolError> {
    if request
        .query
        .as_ref()
        .is_some_and(|q| q.chars().count() > limits::MAX_QUERY_CHARS)
    {
        return Err(ToolError::new(
            ErrorCode::LimitExceeded,
            "query is longer than 256 characters",
        ));
    }
    if !(limits::MIN_LIMIT..=limits::MAX_LIMIT).contains(&request.limit) {
        return Err(ToolError::new(
            ErrorCode::LimitExceeded,
            "limit must be between 1 and 200",
        ));
    }
    Ok(())
}

/// What a cursor binds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Cursor {
    snapshot: u64,
    offset: usize,
    include_values: bool,
    query_hash: u64,
}

impl Cursor {
    /// `<snapshot>.<offset>.<v>.<query hash>`; opaque to the agent.
    fn encode(&self) -> String {
        format!(
            "{}.{}.{}.{:016x}",
            self.snapshot,
            self.offset,
            u8::from(self.include_values),
            self.query_hash
        )
    }

    fn decode(text: &str) -> Option<Self> {
        if text.len() > limits::MAX_CURSOR_CHARS {
            return None;
        }
        let mut parts = text.split('.');
        let snapshot = canonical_u64(parts.next()?)?;
        let offset = usize::try_from(canonical_u64(parts.next()?)?).ok()?;
        let include_values = match parts.next()? {
            "0" => false,
            "1" => true,
            _ => return None,
        };
        let hash = parts.next()?;
        if hash.len() != 16 || !hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            return None;
        }
        let query_hash = u64::from_str_radix(hash, 16).ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some(Self {
            snapshot,
            offset,
            include_values,
            query_hash,
        })
    }
}

/// Decimal without sign or leading zeros, as [`Cursor::encode`] writes it.
fn canonical_u64(text: &str) -> Option<u64> {
    let canonical = !text.is_empty()
        && text.bytes().all(|b| b.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'));
    canonical.then(|| text.parse().ok()).flatten()
}

/// The normalised query, or `None` when it filters nothing.
fn query_key(request: &ObserveRequest) -> Option<String> {
    request
        .query
        .as_deref()
        .map(normalized)
        .filter(|q| !q.is_empty())
}

fn query_hash(key: Option<&str>) -> u64 {
    match key {
        None => fnv1a64(b"none"),
        Some(q) => fnv1a64(format!("q:{q}").as_bytes()),
    }
}

fn matches(element: &StoredElement, key: Option<&str>, show_values: bool) -> bool {
    let Some(key) = key else {
        return true;
    };
    normalized(&element.label).contains(key)
        || (show_values
            && element
                .value
                .as_ref()
                .is_some_and(|(text, _)| normalized(text).contains(key)))
}

fn element_view(element: &StoredElement, show_values: bool) -> ElementView {
    let mut label = element.label.clone();
    truncate_chars(&mut label, limits::MAX_LABEL_CHARS);
    let text = show_values
        .then(|| element.value.clone())
        .flatten()
        .map(|(mut text, kind)| {
            truncate_chars(&mut text, limits::MAX_TEXT_CHARS);
            (text, kind)
        });
    let (enabled, visible) = if element.text_node {
        (None, Some(true))
    } else {
        (
            Some(element.affordance.enabled),
            Some(element.affordance.visible),
        )
    };
    ElementView {
        target_id: element.target_id,
        revision: element.revision,
        role: element.role,
        label,
        label_source: element.label_source,
        actionability: element.affordance.actionability,
        enabled,
        visible,
        text,
        zone: Zone::Unknown,
    }
}

fn stale_cursor() -> ToolError {
    ToolError::new(
        ErrorCode::StaleSnapshot,
        "the cursor does not name the published snapshot with this query; observe again",
    )
}

impl Engine {
    /// The first page of the snapshot just committed.
    pub(super) fn first_page(&self, request: &ObserveRequest) -> Result<ObservePage, ToolError> {
        let published = self.store.published().ok_or_else(stale_cursor)?;
        let measured = self.measured.as_ref().ok_or_else(stale_cursor)?;
        Ok(self.page(published, measured, request, 0, false))
    }

    /// The page a cursor names, without capturing anything.
    pub(super) fn next_page(
        &self,
        request: &ObserveRequest,
        cursor: &str,
    ) -> Result<ObservePage, ToolError> {
        let cursor = Cursor::decode(cursor).ok_or_else(stale_cursor)?;
        let published = self.store.published().ok_or_else(stale_cursor)?;
        let measured = self.measured.as_ref().ok_or_else(stale_cursor)?;
        let key = query_key(request);
        if cursor.snapshot != published.registry.snapshot_id()
            || cursor.include_values != self.show_values(request)
            || cursor.query_hash != query_hash(key.as_deref())
            || cursor.offset == 0
        {
            return Err(stale_cursor());
        }
        let degraded = !self.store.actions_allowed();
        let page = self.page(published, measured, request, cursor.offset, degraded);
        // A cursor past the end was not issued by this engine.
        if cursor.offset >= page.total_elements {
            return Err(stale_cursor());
        }
        Ok(page)
    }

    /// Whether values are shown: the request asks **and** the local policy allows. The
    /// cursor binds this effective value, so a policy change invalidates it.
    fn show_values(&self, request: &ObserveRequest) -> bool {
        request.include_values && self.ports.policy.current().include_values
    }

    fn page(
        &self,
        published: &Published,
        measured: &Measured,
        request: &ObserveRequest,
        offset: usize,
        degraded: bool,
    ) -> ObservePage {
        let show_values = self.show_values(request);
        let key = query_key(request);
        let filtered: Vec<&StoredElement> = published
            .elements
            .iter()
            .filter(|e| matches(e, key.as_deref(), show_values))
            .collect();
        let limit = usize::from(request.limit.clamp(limits::MIN_LIMIT, limits::MAX_LIMIT));
        let end = offset.saturating_add(limit).min(filtered.len());
        let elements = filtered
            .get(offset..end)
            .unwrap_or_default()
            .iter()
            .map(|e| element_view(e, show_values))
            .collect();
        let next_cursor = (end < filtered.len()).then(|| {
            Cursor {
                snapshot: published.registry.snapshot_id(),
                offset: end,
                include_values: show_values,
                query_hash: query_hash(key.as_deref()),
            }
            .encode()
        });
        let age = self.ports.clock.now().saturating_sub(published.captured_at);
        ObservePage {
            session_id: self.config.session_id.clone(),
            snapshot_id: published.snapshot_id(),
            geometry_epoch: published.registry.geometry_epoch(),
            frame_seq: published.frame_seq,
            age_ms: u64::try_from(age.as_millis()).unwrap_or(u64::MAX),
            degraded,
            elements,
            total_elements: filtered.len(),
            next_cursor,
            timings: measured.timings,
            plan: measured.plan.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursors_round_trip_and_reject_anything_else() {
        let c = Cursor {
            snapshot: 12,
            offset: 100,
            include_values: true,
            query_hash: 0xdead_beef,
        };
        assert_eq!(Cursor::decode(&c.encode()), Some(c));
        for bad in [
            "",
            "12.100.1",
            "12.100.2.00000000deadbeef",
            "012.100.1.00000000deadbeef",
            "12.100.1.00000000DEADBEEF",
            "12.100.1.00000000deadbeef.x",
            "-1.100.1.00000000deadbeef",
            "s12.100.1.00000000deadbeef",
        ] {
            assert_eq!(Cursor::decode(bad), None, "{bad}");
        }
    }
}
