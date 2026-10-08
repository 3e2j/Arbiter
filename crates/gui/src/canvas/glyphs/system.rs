//! The fonts installed on the system, found through fontconfig, DirectWrite
//! or Core Text, for the characters none of the app's fonts have.

use fontique::{
    Blob, Collection, CollectionOptions, FallbackKey, QueryStatus, Script, SourceCache,
};
use unicode_script::UnicodeScript;

/// The system's own fallback list per script, loaded the first time it's
/// asked, since reading it can take a while.
#[derive(Default)]
pub struct SystemFonts {
    loaded: Option<(Collection, SourceCache)>,
}

/// A font file the system has, and which font in it when it's a collection.
pub struct SystemFont {
    pub data: Blob<u8>,
    pub index: u32,
}

impl SystemFonts {
    /// The first font in the system's fallback list for `c`'s script that
    /// has `c`.
    pub fn find(&mut self, c: char) -> Option<SystemFont> {
        let raw = <[u8; 4]>::try_from(c.script().short_name().as_bytes()).ok()?;
        let (collection, cache) = self.loaded.get_or_insert_with(|| {
            let options = CollectionOptions {
                shared: false,
                system_fonts: true,
            };
            (Collection::new(options), SourceCache::default())
        });
        let mut query = collection.query(cache);
        query.set_fallbacks(FallbackKey::new(Script::from_bytes(raw), None));
        let mut found = None;
        query.matches_with(|font| {
            if font.charmap().and_then(|charmap| charmap.map(c)).is_none() {
                return QueryStatus::Continue;
            }
            found = Some(SystemFont {
                data: font.blob.clone(),
                index: font.index,
            });
            QueryStatus::Stop
        });
        found
    }
}
