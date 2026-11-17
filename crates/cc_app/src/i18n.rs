//! Localization lives in the engine (`cc_engine::i18n`), so the GUI, the
//! command line and the MCP server name things alike; this module re-exports
//! it and defines the app's `t!` macro.

pub use cc_engine::i18n::*;

/// `t!("key")` or `t!("key", "name" => value, ...)`.
#[macro_export]
macro_rules! t {
    ($key:literal) => {
        $crate::i18n::tr($key)
    };
    ($key:literal, $($name:literal => $value:expr),+ $(,)?) => {
        $crate::i18n::tr_args($key, &[$(($name, $value.to_string())),+])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `t!("...")` key used in the sources must exist in the translations.
    #[test]
    fn all_keys_translated() {
        let messages = parse_messages(Language::PtBr.source());
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut missing = Vec::new();
        // The app's sources and the engine's, which share the messages.
        let dirs = [root.join("src"), root.join("../cc_engine/src"), root.join("../cc_engine/src/commands")];
        let entries = dirs.iter().filter(|d| d.is_dir()).flat_map(|d| std::fs::read_dir(d).unwrap());
        for entry in entries {
            let path = entry.unwrap().path();
            if path.is_dir() || path.file_name().is_some_and(|n| n == "i18n.rs") {
                continue; // macro docs mention placeholder keys
            }
            let src = std::fs::read_to_string(path).unwrap();
            for (pos, _) in src.match_indices("t!(\"") {
                // Skip other macros ending in `t!`, such as `format!`.
                if src[..pos].chars().last().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let key = src[pos + 4..].split('"').next().unwrap();
                if !messages.contains_key(key) {
                    missing.push(key.to_string());
                }
            }
        }
        // Keys built at runtime from the model crates.
        let runtime = cc_core::Category::ALL
            .map(|c| c.key())
            .into_iter()
            .chain(cc_core::ViewCategory::ALL.map(|c| c.key()))
            .chain(cc_core::FoundationKind::ALL.map(|k| k.key()))
            .chain(cc_core::pile::PileKind::ALL.map(|k| k.key()));
        missing.extend(runtime.filter(|k| !messages.contains_key(*k)).map(String::from));
        let project = cc_core::Project::from_template();
        let geom = cc_geom::ModelGeometry::build(&project);
        for c in cc_core::Category::ALL {
            for h in cc_views::schedule(&project, &geom, c, &Default::default()).headers {
                if !messages.contains_key(h) {
                    missing.push(h.to_string());
                }
            }
            let fields = cc_core::ScheduleField::available(c);
            missing.extend(fields.iter().map(|f| f.key()).filter(|k| !messages.contains_key(*k)).map(String::from));
        }
        let cube = crate::view_cube::KEYS;
        let materials = crate::material_browser::KEYS
            .into_iter()
            .chain(cc_core::MaterialClass::ALL.map(|c| c.key()))
            .chain(cc_core::Pattern::ALL.map(|p| p.key()))
            .chain(cc_core::material::Finish::ALL.map(|f| f.key()))
            .chain(cc_core::material::Behavior::ALL.map(|b| b.key()));
        // Structural profiles: materials, shapes and catalog series.
        let profiles: Vec<String> = cc_core::ProfileMaterial::ALL
            .map(|m| m.key().to_string())
            .into_iter()
            .chain(cc_core::ProfileShape::CODES.iter().map(|c| crate::profiles_dialog::shape_key(c)))
            .chain(cc_core::profile_catalog::catalog().iter().map(|s| s.name_key.to_string()))
            .chain(
                cc_core::ProfileShape::CODES
                    .iter()
                    .filter_map(|c| cc_core::ProfileShape::default_for(c))
                    .flat_map(|s| s.params())
                    .map(|(n, _)| crate::profiles_dialog::param_key(&n)),
            )
            .collect();
        missing.extend(profiles.into_iter().filter(|k| !messages.contains_key(k.as_str())));
        for k in ["value-yes", "value-no"].into_iter().chain(cube).chain(materials) {
            if !messages.contains_key(k) {
                missing.push(k.to_string());
            }
        }
        missing.sort();
        missing.dedup();
        assert!(missing.is_empty(), "missing translations: {missing:?}");
    }
}
