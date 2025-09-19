/// A bounded range from `(lo, hi]`
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Range {
    pub lo: f32,
    pub hi: f32,
}

impl Range {
    /// Swap the `lo` and `hi` bounds.
    pub fn invert(self) -> Range {
        Self { lo: self.hi, hi: self.lo }
    }

    /// Return a pair of the `lo` and `hi` bounds.
    pub fn bounds(self) -> (f32, f32) {
        (self.lo, self.hi)
    }

    /// Sort the `lo` and `hi` bounds.
    pub fn sort(self) -> Self {
        if self.lo < self.hi { self } else { self.invert() }
    }
}

macro_rules! impl_from {
    ($($ty:ident),*) => {
        $(
            impl From<std::ops::Range<$ty>> for Range {
                fn from(r: std::ops::Range<$ty>) -> Self {
                    Self {
                        lo: r.start as f32,
                        hi: r.end as f32,
                    }
                }
            }
        )*
    };
}

impl_from!(f32, i32);
