pub fn linear_to_dbfs(fr: f32) -> f32 {
    if fr <= 1e-9 { -120.0 } else { 20.0 * fr.max(1e-9).log10() }
}
pub fn dbfs_to_linear(db: f32) -> f32 {
    (10.0_f32).powf(db / 20.0)
}
