//! Algorithm 3A of the thesis: perceptual cache with rejection (chapter 04 §4.4, ADR-0004).
//!
//! An icon crop, resized to 9×8 greyscale, becomes a 128-bit masked descriptor: 64 gradient
//! signs and 64 validity bits. Two descriptors are compared only where both have a valid
//! gradient, and either polarity matches, so an ideal luminance inversion still matches.
//! The cache only recovers labels that were curated for the same context; anything else,
//! including a near tie between two labels, is `unknown`. Similarity is not intent.
//!
//! Reference: `gradient_descriptor` and `descriptor_distance` in the Python oracle
//! (OR-29..OR-33).

use thiserror::Error;

/// Version of the descriptor and of the crop normalisation that feeds it. Prototypes built
/// with another version are never compared.
pub const DESCRIPTOR_VERSION: u32 = 1;

/// Pixels of the 9×8 greyscale grid a descriptor is computed from.
pub const GRID_LEN: usize = 72;

#[derive(Debug, Error, Clone, Copy, PartialEq)]
pub enum AppearanceError {
    #[error("descriptor input must be {GRID_LEN} greyscale values (9x8), got {0}")]
    WrongShape(usize),
    #[error("gradient threshold must be in 1..=255")]
    InvalidTau,
    #[error("support policy needs min_support >= 1 and min_overlap in [0, 1]")]
    InvalidSupportPolicy,
    #[error("cache policy needs max_distance and min_margin in [0, 1]")]
    InvalidCachePolicy,
}

/// Signs of the horizontal gradients (`bits`) and which of them are valid (`mask`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GradientDescriptor {
    pub bits: u64,
    pub mask: u64,
}

impl GradientDescriptor {
    /// Number of valid gradients.
    pub const fn support(&self) -> u32 {
        self.mask.count_ones()
    }
}

/// Descriptor of a 9×8 greyscale grid, row-major. A gradient is valid when it reaches `tau`
/// in either direction; its bit is set when the left pixel is brighter.
pub fn gradient_descriptor(gray: &[u8], tau: u8) -> Result<GradientDescriptor, AppearanceError> {
    if gray.len() != GRID_LEN {
        return Err(AppearanceError::WrongShape(gray.len()));
    }
    if tau == 0 {
        return Err(AppearanceError::InvalidTau);
    }
    let (mut bits, mut mask) = (0u64, 0u64);
    for r in 0..8 {
        for c in 0..8 {
            let diff = i16::from(gray[r * 9 + c]) - i16::from(gray[r * 9 + c + 1]);
            let bit = 1u64 << (r * 8 + c);
            if diff.unsigned_abs() >= u16::from(tau) {
                mask |= bit;
                if diff > 0 {
                    bits |= bit;
                }
            }
        }
    }
    Ok(GradientDescriptor { bits, mask })
}

/// How much common support two descriptors need before they are compared.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SupportPolicy {
    /// Valid gradients both descriptors must share.
    pub min_support: u32,
    /// Shared support over the union of supports.
    pub min_overlap: f64,
}

impl Default for SupportPolicy {
    /// The thesis's initial values: eight shared gradients, 0.70 overlap.
    fn default() -> Self {
        Self {
            min_support: 8,
            min_overlap: 0.7,
        }
    }
}

/// Fraction of shared gradients that disagree, taking the better polarity, or `None` when
/// the shared support is too small to say anything.
pub fn descriptor_distance(
    a: &GradientDescriptor,
    b: &GradientDescriptor,
    policy: SupportPolicy,
) -> Result<Option<f64>, AppearanceError> {
    if policy.min_support < 1 || !(0.0..=1.0).contains(&policy.min_overlap) {
        return Err(AppearanceError::InvalidSupportPolicy);
    }
    let common = a.mask & b.mask;
    let n = common.count_ones();
    if n < policy.min_support
        || f64::from(n) / f64::from((a.mask | b.mask).count_ones()) < policy.min_overlap
    {
        return Ok(None);
    }
    let mismatches = ((a.bits ^ b.bits) & common).count_ones();
    Ok(Some(
        f64::from(mismatches.min(n - mismatches)) / f64::from(n),
    ))
}

/// Where a prototype's label comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PrototypeSource {
    /// Reviewed by a person for this context: may confirm a label.
    Curated,
    /// Suggested by a fallback classifier: kept for review, never used to confirm.
    Unreviewed,
}

/// A labelled appearance, valid in one context (an application or a screen of it).
#[derive(Debug, Clone, PartialEq)]
pub struct Prototype {
    pub label: String,
    pub context: String,
    pub descriptor_version: u32,
    pub descriptor: GradientDescriptor,
    pub source: PrototypeSource,
}

/// Acceptance rule of the cache. The thesis fixes no thresholds: they are calibrated on the
/// corpus before the test (hypothesis H4), so callers must choose them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CachePolicy {
    pub support: SupportPolicy,
    /// Largest distance accepted as a match.
    pub max_distance: f64,
    /// Smallest gap between the best label and the next different one.
    pub min_margin: f64,
}

impl CachePolicy {
    pub fn new(max_distance: f64, min_margin: f64) -> Result<Self, AppearanceError> {
        let unit = 0.0..=1.0;
        if !unit.contains(&max_distance) || !unit.contains(&min_margin) {
            return Err(AppearanceError::InvalidCachePolicy);
        }
        Ok(Self {
            support: SupportPolicy::default(),
            max_distance,
            min_margin,
        })
    }
}

/// Why the cache answered `unknown`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnknownReason {
    /// The crop (or its overlap with every prototype) has too few valid gradients.
    InsufficientSupport,
    /// No curated prototype exists for this context and descriptor version.
    NoPrototypeForContext,
    /// The best prototype is further than the policy allows.
    TooFar,
    /// Two different labels are too close to tell apart.
    Ambiguous,
}

/// Result of a lookup.
#[derive(Debug, Clone, PartialEq)]
pub enum IconMatch {
    Known { label: String, distance: f64 },
    Unknown(UnknownReason),
}

/// Curated prototypes, partitioned by context and descriptor version.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct IconCache {
    prototypes: Vec<Prototype>,
}

impl IconCache {
    pub const fn new() -> Self {
        Self {
            prototypes: Vec::new(),
        }
    }

    /// Adds a prototype. Several prototypes may share a label; a collision between labels
    /// is kept as two candidates, never resolved by overwriting.
    pub fn insert(&mut self, prototype: Prototype) {
        self.prototypes.push(prototype);
    }

    pub fn prototypes(&self) -> &[Prototype] {
        &self.prototypes
    }

    /// Retained vector storage and string capacities, excluding allocator overhead.
    pub fn retained_storage_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.prototypes.capacity() * std::mem::size_of::<Prototype>()
            + self
                .prototypes
                .iter()
                .map(|p| p.label.capacity() + p.context.capacity())
                .sum::<usize>()
    }

    /// The curated label for `query` in `context`, or why there is none.
    pub fn lookup(
        &self,
        query: &GradientDescriptor,
        context: &str,
        policy: CachePolicy,
    ) -> Result<IconMatch, AppearanceError> {
        if query.support() < policy.support.min_support {
            return Ok(IconMatch::Unknown(UnknownReason::InsufficientSupport));
        }
        let eligible: Vec<&Prototype> = self
            .prototypes
            .iter()
            .filter(|p| {
                p.context == context
                    && p.descriptor_version == DESCRIPTOR_VERSION
                    && p.source == PrototypeSource::Curated
            })
            .collect();
        if eligible.is_empty() {
            return Ok(IconMatch::Unknown(UnknownReason::NoPrototypeForContext));
        }
        // Best distance per label, sorted by (distance, label).
        let mut best: Vec<(f64, &str)> = Vec::new();
        for p in eligible {
            let Some(d) = descriptor_distance(query, &p.descriptor, policy.support)? else {
                continue;
            };
            match best.iter_mut().find(|(_, label)| *label == p.label) {
                Some(entry) => entry.0 = entry.0.min(d),
                None => best.push((d, &p.label)),
            }
        }
        best.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(b.1)));
        Ok(match best.as_slice() {
            [] => IconMatch::Unknown(UnknownReason::InsufficientSupport),
            [(d, _), ..] if *d > policy.max_distance => IconMatch::Unknown(UnknownReason::TooFar),
            [(d, _), (next, _), ..] if next - d < policy.min_margin => {
                IconMatch::Unknown(UnknownReason::Ambiguous)
            }
            [(d, label), ..] => IconMatch::Known {
                label: (*label).to_owned(),
                distance: *d,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The original 64-bit dHash of the dossier (`I(delta >= tau)`), kept only to reproduce
    /// its counterexample.
    fn legacy_dhash(gray: &[u8], tau: i16) -> u64 {
        let mut hash = 0;
        for r in 0..8 {
            for c in 0..8 {
                if i16::from(gray[r * 9 + c]) - i16::from(gray[r * 9 + c + 1]) >= tau {
                    hash |= 1 << (r * 8 + c);
                }
            }
        }
        hash
    }

    fn legacy_distance(a: u64, b: u64) -> u32 {
        let d = (a ^ b).count_ones();
        d.min(64 - d)
    }

    /// The oracle's striped icon: eight rows of `[0, 10, 10, 0, 0, 10, 10, 0, 0]`.
    fn stripes() -> Vec<u8> {
        [0, 10, 10, 0, 0, 10, 10, 0, 0].repeat(8)
    }

    fn inverted(gray: &[u8]) -> Vec<u8> {
        gray.iter().map(|v| 255 - v).collect()
    }

    fn descriptor(gray: &[u8]) -> GradientDescriptor {
        gradient_descriptor(gray, 3).unwrap()
    }

    /// A pseudo-random 9×8 crop, different for each seed and with almost every gradient
    /// valid.
    fn icon(seed: u32) -> Vec<u8> {
        let mut state = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
        (0..GRID_LEN)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 24) as u8
            })
            .collect()
    }

    fn curated(label: &str, context: &str, gray: &[u8]) -> Prototype {
        Prototype {
            label: label.to_owned(),
            context: context.to_owned(),
            descriptor_version: DESCRIPTOR_VERSION,
            descriptor: descriptor(gray),
            source: PrototypeSource::Curated,
        }
    }

    fn policy() -> CachePolicy {
        CachePolicy::new(0.1, 0.1).unwrap()
    }

    /// UT-27 (OR-29): the original dHash puts an ideal inversion 32 bits away.
    #[test]
    fn ut_27_legacy_dhash_misses_an_inverted_theme() {
        let a = stripes();
        let b = inverted(&a);
        assert_eq!(
            legacy_distance(legacy_dhash(&a, 3), legacy_dhash(&b, 3)),
            32
        );
    }

    /// UT-28 (OR-30): the masked descriptor matches the ideal inversion at distance zero.
    #[test]
    fn ut_28_masked_descriptor_matches_ideal_inversion() {
        let a = stripes();
        assert_eq!(
            descriptor_distance(
                &descriptor(&a),
                &descriptor(&inverted(&a)),
                SupportPolicy::default()
            ),
            Ok(Some(0.0))
        );
    }

    /// UT-29 (OR-31, OR-33): flat crops and low support are rejected, not matched.
    #[test]
    fn ut_29_insufficient_support_is_rejected() {
        let flat = descriptor(&[0; GRID_LEN]);
        assert_eq!(flat.support(), 0);
        assert_eq!(
            descriptor_distance(&flat, &flat, SupportPolicy::default()),
            Ok(None)
        );
        let one = GradientDescriptor { bits: 1, mask: 1 };
        assert_eq!(
            descriptor_distance(&one, &one, SupportPolicy::default()),
            Ok(None)
        );

        let mut cache = IconCache::new();
        cache.insert(curated("Guardar", "app", &stripes()));
        assert_eq!(
            cache.lookup(&flat, "app", policy()),
            Ok(IconMatch::Unknown(UnknownReason::InsufficientSupport))
        );
    }

    /// UT-30 (OR-32): the input must be 9×8 values; the type keeps them in 0..=255.
    #[test]
    fn ut_30_descriptor_input_is_validated() {
        assert_eq!(
            gradient_descriptor(&[0; 71], 3),
            Err(AppearanceError::WrongShape(71))
        );
        assert_eq!(
            gradient_descriptor(&[0; 73], 3),
            Err(AppearanceError::WrongShape(73))
        );
        assert_eq!(
            gradient_descriptor(&[0; GRID_LEN], 0),
            Err(AppearanceError::InvalidTau)
        );
        let invalid = SupportPolicy {
            min_support: 0,
            min_overlap: 0.7,
        };
        let a = descriptor(&stripes());
        assert_eq!(
            descriptor_distance(&a, &a, invalid),
            Err(AppearanceError::InvalidSupportPolicy)
        );
    }

    /// UT-31: the cache answers within its context and version, with a margin, and never
    /// confirms an unknown or unreviewed appearance.
    #[test]
    fn ut_31_cache_respects_context_version_and_margin() {
        let save = icon(1);
        let close = icon(2);
        let q = descriptor(&save);
        let mut cache = IconCache::new();
        cache.insert(curated("Guardar", "editor", &save));
        cache.insert(curated("Cerrar", "editor", &close));

        // Known in its context, also under an inverted theme.
        assert_eq!(
            cache.lookup(&descriptor(&inverted(&save)), "editor", policy()),
            Ok(IconMatch::Known {
                label: "Guardar".to_owned(),
                distance: 0.0
            })
        );
        // Another context has no curated meaning for it.
        assert_eq!(
            cache.lookup(&q, "navegador", policy()),
            Ok(IconMatch::Unknown(UnknownReason::NoPrototypeForContext))
        );
        // A different appearance is too far from every prototype.
        assert_eq!(
            cache.lookup(&descriptor(&icon(5)), "editor", policy()),
            Ok(IconMatch::Unknown(UnknownReason::TooFar))
        );

        // Two labels for the same appearance: rejected, neither overwrites the other.
        cache.insert(curated("Eliminar", "editor", &save));
        assert_eq!(
            cache.lookup(&q, "editor", policy()),
            Ok(IconMatch::Unknown(UnknownReason::Ambiguous))
        );
        assert_eq!(cache.prototypes().len(), 3);

        // Unreviewed and other-version prototypes never confirm.
        let mut cache = IconCache::new();
        cache.insert(Prototype {
            source: PrototypeSource::Unreviewed,
            ..curated("Guardar", "editor", &save)
        });
        cache.insert(Prototype {
            descriptor_version: DESCRIPTOR_VERSION + 1,
            ..curated("Guardar", "editor", &save)
        });
        assert_eq!(
            cache.lookup(&q, "editor", policy()),
            Ok(IconMatch::Unknown(UnknownReason::NoPrototypeForContext))
        );
    }

    #[test]
    fn same_label_twice_is_not_a_conflict() {
        let save = icon(1);
        let mut cache = IconCache::new();
        cache.insert(curated("Guardar", "editor", &save));
        cache.insert(curated("Guardar", "editor", &inverted(&save)));
        assert!(matches!(
            cache.lookup(&descriptor(&save), "editor", policy()),
            Ok(IconMatch::Known { ref label, .. }) if label == "Guardar"
        ));
    }

    #[test]
    fn cache_policy_is_validated() {
        assert_eq!(
            CachePolicy::new(1.5, 0.1),
            Err(AppearanceError::InvalidCachePolicy)
        );
        assert_eq!(
            CachePolicy::new(0.1, f64::NAN),
            Err(AppearanceError::InvalidCachePolicy)
        );
    }
}
