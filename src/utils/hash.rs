use std::hash::{DefaultHasher, Hash, Hasher};

pub fn hash(value: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}
