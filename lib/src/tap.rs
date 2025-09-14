pub trait Tap {
    fn tap(self, f: impl Fn(&Self)) -> Self;
}
impl<T> Tap for T {
    fn tap(self, f: impl Fn(&Self)) -> Self {
        f(&self);
        self
    }
}

pub trait TapMut {
    fn tap_mut(self, f: impl Fn(&mut Self)) -> Self;
}
impl<T> TapMut for T {
    fn tap_mut(mut self, f: impl Fn(&mut Self)) -> Self {
        f(&mut self);
        self
    }
}
