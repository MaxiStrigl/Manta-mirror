use tree_sitter::Language;

pub mod generated {
    include!(concat!(env!("OUT_DIR"), "/treesitter_types_generated.rs"));
}

pub static HIGHLIGHTS_QUERY: &str = include_str!("queries/highlights.scm");

unsafe extern "C" {
    fn tree_sitter_org() -> *const std::ffi::c_void;
}

pub fn language() -> Language {
    unsafe { Language::from_raw(tree_sitter_org() as *const _) }
}
