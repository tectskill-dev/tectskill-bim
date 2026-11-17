use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

macro_rules! id_type {
    ($name:ident, $prefix:literal) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub struct $name(pub u64);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}{}", $prefix, self.0)
            }
        }
    };
}

id_type!(ElementId, "E");
id_type!(ElementTypeId, "T");
id_type!(LevelId, "L");
id_type!(ViewId, "V");
id_type!(GroupId, "G");
id_type!(SystemId, "S");
id_type!(FilterId, "F");
id_type!(TemplateId, "VT");
id_type!(PhaseId, "PH");
id_type!(PhaseFilterId, "PF");
id_type!(LineStyleId, "LS");
id_type!(OptionSetId, "OS");
id_type!(OptionId, "OP");
id_type!(FillPatternId, "FP");
id_type!(ProfileId, "PR");
id_type!(BrowserSchemeId, "BO");

impl Default for PhaseId {
    /// Unassigned: read as the last phase until one is given.
    fn default() -> Self {
        PhaseId(0)
    }
}

/// Globally unique, persistent identifier of a model object (128 bits),
/// kept across saves so exports such as IFC keep stable ids. Zero means
/// "not assigned yet".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Uid(pub u128);

impl Uid {
    /// A new random identifier (version 4 UUID layout).
    pub fn generate() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let count = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
        // Each RandomState is seeded from the operating system's randomness.
        let half = |salt: u64| {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u64(nanos);
            h.write_u64(count);
            h.write_u64(salt);
            h.finish()
        };
        let n = (u128::from(half(1)) << 64) | u128::from(half(2));
        let n = (n & !(0xF << 76)) | (0x4 << 76);
        let n = (n & !(0b11 << 62)) | (0b10 << 62);
        Self(n)
    }

    pub fn is_nil(self) -> bool {
        self.0 == 0
    }
}

impl fmt::Display for Uid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:032x}", self.0)
    }
}

impl Serialize for Uid {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Uid {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        u128::from_str_radix(&text, 16).map(Uid).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uids_are_unique_and_round_trip() {
        let (a, b) = (Uid::generate(), Uid::generate());
        assert_ne!(a, b);
        assert!(!a.is_nil());
        assert_eq!(a.to_string().len(), 32);
        let text = ron::to_string(&a).unwrap();
        assert_eq!(ron::from_str::<Uid>(&text).unwrap(), a);
    }
}
