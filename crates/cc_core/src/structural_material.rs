//! The structural material of a column or beam type: concrete, steel
//! (rolled or cold-formed) or timber. It comes from the type's profile,
//! else from the type's own material (a legacy section that makes no
//! outline).

use crate::structural_profile::ProfileMaterial;
use crate::{Category, ElementType, Project, TypeData};

/// What a column or beam is made of, as the Structure tab groups its tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum StructuralMaterial {
    #[default]
    Concrete,
    Steel,
    Timber,
}

impl StructuralMaterial {
    pub const ALL: [StructuralMaterial; 3] = [Self::Concrete, Self::Steel, Self::Timber];

    pub fn code(self) -> &'static str {
        match self {
            Self::Concrete => "concrete",
            Self::Steel => "steel",
            Self::Timber => "timber",
        }
    }

    pub fn from_code(c: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.code().eq_ignore_ascii_case(c.trim()))
    }

    /// The i18n key of its name (shared with the profile materials).
    pub fn key(self) -> &'static str {
        match self {
            Self::Concrete => "profile-material-concrete",
            Self::Steel => "profile-material-steel",
            Self::Timber => "profile-material-timber",
        }
    }

    /// The structural material of a profile's: cold-formed steel is steel.
    pub fn of_profile(m: ProfileMaterial) -> Self {
        match m {
            ProfileMaterial::Steel | ProfileMaterial::ColdFormed => Self::Steel,
            ProfileMaterial::Timber => Self::Timber,
            ProfileMaterial::Concrete => Self::Concrete,
        }
    }
}

impl Project {
    /// The structural material of a column or beam type: its profile's,
    /// else its material's class (metal is steel, wood timber, the rest
    /// concrete). `None` for other types.
    pub fn structural_material(&self, data: &TypeData) -> Option<StructuralMaterial> {
        let (TypeData::Column { material, .. } | TypeData::Beam { material, .. }) = data else { return None };
        let profile = match self.type_profile_def(data) {
            Some(def) => def.material,
            None => self.profile_material_for(material),
        };
        Some(StructuralMaterial::of_profile(profile))
    }

    /// The types of `category` made of `material`, in id order.
    pub fn types_of_material(
        &self,
        category: Category,
        material: StructuralMaterial,
    ) -> impl Iterator<Item = &ElementType> + '_ {
        self.types_of(category).filter(move |t| self.structural_material(&t.data) == Some(material))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structural_profile::{ProfileDef, ProfileShape};
    use crate::{ColumnSection, MaterialClass, ProfileId, Uid};

    fn with_profile(p: &mut Project, material: ProfileMaterial) -> TypeData {
        let id = ProfileId(p.alloc_id());
        let shape = ProfileShape::Rect { b: 0.2, h: 0.4, chamfer: 0.0 };
        let name = format!("P{}", id.0);
        let def =
            ProfileDef { id, uid: Uid::generate(), name, material, shape, catalog: None, description: String::new() };
        p.profiles.insert(id, def);
        TypeData::Column {
            section: ColumnSection::Rectangular { width: 0.2, depth: 0.4 },
            material: "none".into(),
            profile: Some(id),
            profile_rotation_deg: 0.0,
            profile_mirror: false,
        }
    }

    #[test]
    fn codes_round_trip() {
        for m in StructuralMaterial::ALL {
            assert_eq!(StructuralMaterial::from_code(m.code()), Some(m));
        }
        assert_eq!(StructuralMaterial::from_code(" STEEL "), Some(StructuralMaterial::Steel));
        assert_eq!(StructuralMaterial::from_code("gold"), None);
    }

    #[test]
    fn profiles_decide_the_material() {
        let mut p = Project::from_template();
        for (pm, want) in [
            (ProfileMaterial::Steel, StructuralMaterial::Steel),
            (ProfileMaterial::ColdFormed, StructuralMaterial::Steel),
            (ProfileMaterial::Timber, StructuralMaterial::Timber),
            (ProfileMaterial::Concrete, StructuralMaterial::Concrete),
        ] {
            let data = with_profile(&mut p, pm);
            assert_eq!(p.structural_material(&data), Some(want), "{pm:?}");
        }
        assert_eq!(p.structural_material(&TypeData::Room {}), None);
    }

    #[test]
    fn a_type_without_profile_reads_its_material_class() {
        let p = Project::from_template();
        let named = |class| p.materials.iter().find(|m| m.identity.class == class).map(|m| m.name.clone());
        let column = |material: String| TypeData::Column {
            section: ColumnSection::Rectangular { width: 0.2, depth: 0.2 },
            material,
            profile: None,
            profile_rotation_deg: 0.0,
            profile_mirror: false,
        };
        let metal = named(MaterialClass::Metal).expect("the template has a metal");
        assert_eq!(p.structural_material(&column(metal)), Some(StructuralMaterial::Steel));
        if let Some(wood) = named(MaterialClass::Wood) {
            assert_eq!(p.structural_material(&column(wood)), Some(StructuralMaterial::Timber));
        }
        assert_eq!(p.structural_material(&column("unknown".into())), Some(StructuralMaterial::Concrete));
    }

    #[test]
    fn template_types_by_material() {
        let p = Project::from_template();
        assert!(p.types_of_material(Category::Column, StructuralMaterial::Concrete).next().is_some());
        assert!(p.types_of_material(Category::Column, StructuralMaterial::Steel).next().is_some());
        assert!(p.types_of_material(Category::Beam, StructuralMaterial::Steel).next().is_some());
        assert!(p.types_of_material(Category::Column, StructuralMaterial::Timber).next().is_none());
    }
}
