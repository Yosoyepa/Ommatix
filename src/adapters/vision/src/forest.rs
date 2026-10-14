/// Disjoint-set forest over `u32` indices (one per pixel or per box), with path halving.
/// A root is always the smallest index of its set, so grouping order is deterministic.
#[derive(Debug, Clone)]
pub(crate) struct Forest {
    parent: Vec<u32>,
}

impl Forest {
    /// `n` must fit in `u32`; callers check their sizes before building a forest.
    pub(crate) fn new(n: usize) -> Self {
        let n = u32::try_from(n).expect("forest size fits in u32");
        Self {
            parent: (0..n).collect(),
        }
    }

    pub(crate) fn find(&mut self, i: usize) -> usize {
        let mut i = i as u32;
        while self.parent[i as usize] != i {
            let grandparent = self.parent[self.parent[i as usize] as usize];
            self.parent[i as usize] = grandparent;
            i = grandparent;
        }
        i as usize
    }

    pub(crate) fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
            self.parent[hi] = lo as u32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roots_are_the_smallest_member() {
        let mut f = Forest::new(6);
        f.union(4, 2);
        f.union(5, 4);
        f.union(1, 3);
        assert_eq!(f.find(5), 2);
        assert_eq!(f.find(3), 1);
        assert_eq!(f.find(0), 0);
    }
}
